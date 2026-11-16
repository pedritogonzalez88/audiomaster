//! Tauri desktop shell.
//!
//! Responsibilities:
//! - own the process-wide wiring (database, library service, playout engine,
//!   audio output) and keep it alive for the lifetime of the window;
//! - expose a small, versioned IPC surface to the React UI with camelCase DTOs;
//! - run a 100 ms control ticker that advances the engine and keeps the audio
//!   device in sync with the state machine.
//!
//! The shell deliberately contains no business rules: everything below the
//! command handlers comes from the `application`, `domain` and `playout-engine`
//! crates so it stays testable without Tauri.

use application::{AppError, AppResult, LibraryQuery, LibraryService, PlayoutPort};
use domain::MediaAsset;
use infra_audio::{decode_to, AudioOutput, CpalPlayer, NullOutput, SymphoniaProbe};
use infra_db::{CartSlot, Database};
use parking_lot::Mutex;
use playout_engine::{PlayoutEngine, SystemClock};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, State};

/// Control-plane tick. 100 ms is fast enough for a UI that polls at 2 Hz while
/// staying far away from the audio callback, which runs on its own thread.
const TICK_MS: u64 = 100;

/// Re-sync the device playhead only when it drifts more than this. Keeps the
/// seek chatter (and the audible gap) at zero during normal playback.
const MAX_DRIFT_MS: u64 = 750;

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// Serialized error handed to the UI. Mirrors `IpcError` in `ui/src/types.ts`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    code: String,
    message: String,
}

impl From<AppError> for IpcError {
    fn from(error: AppError) -> Self {
        let code = match &error {
            AppError::Domain(_) => "domain",
            AppError::NotFound(_) => "not_found",
            AppError::InvalidInput(_) => "invalid_input",
            AppError::Repository(_) => "repository",
            AppError::Probe(_) => "probe",
            AppError::Audio(_) => "audio",
            AppError::Playout(_) => "playout",
            AppError::Configuration(_) => "configuration",
            AppError::Internal(_) => "internal",
        };
        Self {
            code: code.to_string(),
            message: error.to_string(),
        }
    }
}

impl IpcError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_input".into(),
            message: message.into(),
        }
    }
}

type IpcResult<T> = Result<T, IpcError>;

// ---------------------------------------------------------------------------
// DTOs (camelCase to match ui/src/types.ts)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfoDto {
    station_name: String,
    version: String,
    device_name: String,
    sample_rate: u32,
    channels: u16,
    database_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    id: i64,
    code: String,
    name: String,
    color_hex: String,
    is_commercial: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDto {
    id: i64,
    category_id: i64,
    title: String,
    artist: String,
    album: Option<String>,
    release_year: Option<i32>,
    bpm: u32,
    kind: domain::AssetKind,
    duration_ms: u32,
    gain_db: f64,
    loudness_lufs: f64,
    cue_in_ms: u32,
    intro_ms: u32,
    hook_ms: u32,
    mix_point_ms: u32,
    fade_out_ms: u32,
    cue_out_ms: u32,
    play_count: u64,
    last_played_at: Option<i64>,
    file_path: String,
}

impl From<MediaAsset> for AssetDto {
    fn from(asset: MediaAsset) -> Self {
        Self {
            id: asset.id,
            category_id: asset.category_id,
            title: asset.title,
            artist: asset.artist,
            album: asset.album,
            release_year: asset.release_year,
            bpm: asset.bpm,
            kind: asset.kind,
            duration_ms: asset.duration_ms,
            gain_db: asset.gain_db,
            loudness_lufs: asset.loudness_lufs,
            cue_in_ms: asset.cue_points.cue_in_ms,
            intro_ms: asset.cue_points.intro_ms,
            hook_ms: asset.cue_points.hook_ms,
            mix_point_ms: asset.cue_points.mix_point_ms,
            fade_out_ms: asset.cue_points.fade_out_ms,
            cue_out_ms: asset.cue_points.cue_out_ms,
            play_count: asset.play_count,
            last_played_at: asset.last_played_at,
            file_path: asset.file_path,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayoutSnapshotDto {
    is_playing: bool,
    is_paused: bool,
    stop_after_current: bool,
    current_title: Option<String>,
    current_artist: Option<String>,
    position_ms: u64,
    duration_ms: u64,
    next_title: Option<String>,
    queue_len: usize,
    talk_over: bool,
    underruns: u64,
    loop_current: bool,
    voice_over: bool,
    crossfade_progress: Option<f64>,
    incoming_title: Option<String>,
    outgoing_gain: f64,
    incoming_gain: f64,
}

impl From<application::PlayoutSnapshot> for PlayoutSnapshotDto {
    fn from(snapshot: application::PlayoutSnapshot) -> Self {
        Self {
            is_playing: snapshot.is_playing,
            is_paused: snapshot.is_paused,
            stop_after_current: snapshot.stop_after_current,
            current_title: snapshot.current_title,
            current_artist: snapshot.current_artist,
            position_ms: snapshot.position_ms,
            duration_ms: snapshot.duration_ms,
            next_title: snapshot.next_title,
            queue_len: snapshot.queue_len,
            talk_over: snapshot.talk_over,
            underruns: snapshot.underruns,
            loop_current: snapshot.loop_current,
            voice_over: snapshot.voice_over,
            crossfade_progress: snapshot.crossfade_progress,
            incoming_title: snapshot.incoming_title,
            outgoing_gain: snapshot.outgoing_gain,
            incoming_gain: snapshot.incoming_gain,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CartSlotDto {
    id: i64,
    slot_index: i64,
    hotkey: String,
    color_hex: String,
    is_loop: bool,
    gain_db: f64,
    asset_id: Option<i64>,
    title: Option<String>,
    artist: Option<String>,
    duration_ms: Option<i64>,
}

impl From<CartSlot> for CartSlotDto {
    fn from(slot: CartSlot) -> Self {
        Self {
            id: slot.id,
            slot_index: slot.slot_index,
            hotkey: slot.hotkey,
            color_hex: slot.color_hex,
            is_loop: slot.is_loop,
            gain_db: slot.gain_db,
            asset_id: slot.asset_id,
            title: slot.title,
            artist: slot.artist,
            duration_ms: slot.duration_ms,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummaryDto {
    imported: usize,
    skipped: usize,
    failed: usize,
    total: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySnapshotDto {
    pub at_utc_ms: i64,
    pub current_asset_id: Option<i64>,
    pub current_position_ms: u64,
    pub queue: Vec<i64>,
    pub stop_after_current: bool,
    pub talk_over: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryPayload {
    pub position_ms: u64,
    pub peak_l: f32,
    pub peak_r: f32,
    pub lufs: f64,
    pub underruns: u64,
}

fn default_limit() -> u32 {
    100
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryQueryDto {
    #[serde(default)]
    text: String,
    #[serde(default)]
    category_id: Option<i64>,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
}

impl From<LibraryQueryDto> for LibraryQuery {
    fn from(query: LibraryQueryDto) -> Self {
        Self {
            text: query.text,
            category_id: query.category_id,
            limit: query.limit,
            offset: query.offset,
        }
    }
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct AudioState {
    /// Asset id currently loaded in the output device.
    loaded: Option<i64>,
    /// Asset id whose decode failed; prevents a 10 Hz retry storm.
    failed: Option<i64>,
    /// Underruns reported by the output device since the last sync.
    audio_underruns_seen: u64,
    /// Asset id loaded into the second (incoming) deck.
    incoming_loaded: Option<i64>,
}

#[derive(Default)]
struct AuditState {
    current_log_id: Option<i64>,
    current_asset_id: Option<i64>,
    current_started_at: i64,
}

#[derive(Clone)]
pub struct AppState {
    database: Arc<Database>,
    library: Arc<LibraryService>,
    engine: Arc<PlayoutEngine>,
    audio: Arc<dyn AudioOutput>,
    audio_state: Arc<Mutex<AudioState>>,
    audit: Arc<dyn application::AuditRepository>,
    audit_state: Arc<Mutex<AuditState>>,
    clock: Arc<dyn application::Clock>,
    pending_recovery: Arc<Mutex<Option<RecoverySnapshotDto>>>,
    ticks_since_snapshot: Arc<Mutex<u32>>,
    station_name: Arc<str>,
}

impl AppState {
    fn bootstrap(app: &tauri::AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&data_dir)?;
        let database_path = data_dir.join("audiomaster.db");
        tracing::info!(path = %database_path.display(), "opening database");

        let database = Arc::new(Database::open(&database_path)?);
        let library = Arc::new(LibraryService::new(
            database.clone(),
            Arc::new(SymphoniaProbe::new()),
        ));

        let audio: Arc<dyn AudioOutput> = match CpalPlayer::new() {
            Ok(player) => {
                tracing::info!(
                    device = player.device_name(),
                    sample_rate = player.sample_rate(),
                    channels = player.channels(),
                    "audio output ready"
                );
                Arc::new(player)
            }
            Err(error) => {
                tracing::warn!(%error, "no audio output available; using the null sink");
                Arc::new(NullOutput::new(48_000, 2))
            }
        };

        let station_name = database
            .station_name()
            .unwrap_or_else(|_| "Radio Master FM".to_string());

        // Recovery: offer the last (unclean) snapshot to the UI.
        let pending_recovery =
            database
                .latest_snapshot(1)
                .ok()
                .flatten()
                .and_then(|(json, clean, at)| {
                    if clean {
                        None
                    } else {
                        serde_json::from_str::<RecoverySnapshotDto>(&json).ok().map(
                            |mut snapshot| {
                                snapshot.at_utc_ms = at;
                                snapshot
                            },
                        )
                    }
                });

        Ok(Self {
            database: database.clone(),
            library,
            engine: Arc::new(PlayoutEngine::new(Arc::new(SystemClock))),
            audio,
            audio_state: Arc::new(Mutex::new(AudioState::default())),
            audit: database.clone() as Arc<dyn application::AuditRepository>,
            audit_state: Arc::new(Mutex::new(AuditState::default())),
            clock: Arc::new(SystemClock),
            pending_recovery: Arc::new(Mutex::new(pending_recovery)),
            ticks_since_snapshot: Arc::new(Mutex::new(0)),
            station_name: Arc::from(station_name),
        })
    }

    /// Starts the control ticker. The thread lives until the process exits.
    fn spawn_ticker(&self) {
        let state = self.clone();
        let spawned = std::thread::Builder::new()
            .name("audiomaster-ticker".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(TICK_MS));
                state.engine.tick();
                state.sync_audio();
            });
        if let Err(error) = spawned {
            tracing::error!(%error, "could not start the control ticker");
        }
    }

    fn info(&self) -> AppInfoDto {
        AppInfoDto {
            station_name: self.station_name.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            device_name: self.audio.device_name().to_string(),
            sample_rate: self.audio.sample_rate(),
            channels: self.audio.channels(),
            database_path: self.database.path().display().to_string(),
        }
    }

    fn snapshot(&self) -> AppResult<PlayoutSnapshotDto> {
        self.engine.snapshot().map(PlayoutSnapshotDto::from)
    }

    fn sync_audio(&self) {
        if let Err(error) = self.apply_audio_state() {
            tracing::warn!(%error, "audio sync failed");
        }
        let talk_over = self.engine.snapshot().map(|s| s.talk_over).unwrap_or(false);
        let _ = self.audio.set_talk_over(talk_over);
        self.update_audit();

        // Persist a recovery snapshot periodically (every ~5 s) and on track
        // changes, so a kill -9 can be resumed.
        let mut ticks = self.ticks_since_snapshot.lock();
        *ticks += 1;
        let should_save = *ticks >= 50;
        if should_save {
            *ticks = 0;
        }
        drop(ticks);
        if should_save || self.pending_recovery.lock().is_some() {
            self.save_snapshot();
        }
    }

    /// Aligns the output device with the engine: loads the current asset when
    /// it changes, then mirrors play/pause/stop. Runs only on the ticker thread,
    /// so it owns `audio_state` exclusively.
    fn apply_audio_state(&self) -> AppResult<()> {
        let snapshot = self.engine.snapshot()?;
        let current = self.engine.current_asset();
        let mut audio_state = self.audio_state.lock();
        let desired = current.as_ref().map(|asset| asset.id);

        if desired != audio_state.loaded {
            match current {
                Some(asset) => {
                    if audio_state.failed == desired {
                        // Already reported; wait for the engine to move on.
                        self.audio.pause()?;
                        return Ok(());
                    }

                    let path = Path::new(&asset.file_path);
                    let decoded = decode_to(path, self.audio.sample_rate(), self.audio.channels());
                    match decoded {
                        Ok(audio) => {
                            self.audio.load(audio)?;
                            self.audio.set_gain_db(asset.gain_db)?;
                            audio_state.loaded = desired;
                            audio_state.failed = None;
                            tracing::debug!(
                                asset = %asset.title,
                                duration_ms = asset.duration_ms,
                                "loaded track into the audio device"
                            );
                        }
                        Err(error) => {
                            audio_state.loaded = None;
                            audio_state.failed = desired;
                            self.audio.stop()?;
                            return Err(error);
                        }
                    }
                }
                None => {
                    self.audio.stop()?;
                    self.audio.clear_incoming();
                    audio_state.loaded = None;
                    audio_state.failed = None;
                    audio_state.incoming_loaded = None;
                    return Ok(());
                }
            }
        }

        // Second deck for crossfade.
        let incoming_desired = self.engine.incoming_asset().map(|asset| asset.id);
        if incoming_desired != audio_state.incoming_loaded {
            if let Some(asset) = self.engine.incoming_asset() {
                let path = Path::new(&asset.file_path);
                if let Ok(audio) = decode_to(path, self.audio.sample_rate(), self.audio.channels())
                {
                    let _ = self.audio.load_incoming(audio);
                    let _ = self.audio.set_gain_db(asset.gain_db);
                    audio_state.incoming_loaded = incoming_desired;
                }
            } else {
                self.audio.clear_incoming();
                audio_state.incoming_loaded = None;
            }
        }
        self.audio
            .set_crossfade_progress(snapshot.crossfade_progress);

        if snapshot.is_playing {
            if !self.audio.is_playing() {
                let drift = self.audio.position_ms().abs_diff(snapshot.position_ms);
                if drift > MAX_DRIFT_MS {
                    self.audio.seek_ms(snapshot.position_ms)?;
                }
                self.audio.play()?;
            }
        } else if self.audio.is_playing() {
            self.audio.pause()?;
        }

        let device_underruns = self.audio.underruns();
        if device_underruns > audio_state.audio_underruns_seen {
            let delta = device_underruns - audio_state.audio_underruns_seen;
            for _ in 0..delta {
                self.engine.record_underrun();
            }
            audio_state.audio_underruns_seen = device_underruns;
        }

        Ok(())
    }

    /// Resets the device when the queue/current track is cleared, so the next
    /// track starts from a clean buffer.
    fn stop_audio(&self) {
        if let Err(error) = self.audio.stop() {
            tracing::warn!(%error, "could not stop the audio device");
        }
        self.audio.clear_incoming();
        let mut audio_state = self.audio_state.lock();
        audio_state.loaded = None;
        audio_state.failed = None;
        audio_state.incoming_loaded = None;

        self.end_audit("STOPPED", false);
    }

    /// Closes the audit row when the current track changes or stops. Called from
    /// the control ticker so every transition is recorded with origin MANUAL.
    fn update_audit(&self) {
        let now = self.clock.now_utc_ms();
        let current = self.engine.current_asset();
        let current_id = current.as_ref().map(|asset| asset.id);

        let mut audit_state = self.audit_state.lock();
        if current_id == audit_state.current_asset_id {
            return;
        }

        if let Some(log_id) = audit_state.current_log_id.take() {
            let duration = now.saturating_sub(audit_state.current_started_at) as u64;
            let _ = self
                .audit
                .end_playback(log_id, now, duration, Some("TRACK_CHANGED"), false);
        }

        if let Some(asset) = &current {
            let category_code = self
                .database
                .category_code(asset.category_id)
                .unwrap_or_else(|_| "MUSIC".to_string());
            let entry = application::PlayLogStart {
                station_id: 1,
                asset_id: Some(asset.id),
                raw_title: asset.title.clone(),
                raw_artist: asset.artist.clone(),
                category_code,
                origin: "MANUAL",
                started_at: now,
                loudness_lufs: Some(asset.loudness_lufs),
            };
            if let Ok(id) = self.audit.start_playback(&entry) {
                audit_state.current_log_id = Some(id);
                audit_state.current_started_at = now;
            }
        } else {
            audit_state.current_started_at = 0;
        }

        audit_state.current_asset_id = current_id;

        drop(audit_state);
        self.save_snapshot();
    }

    fn end_audit(&self, reason: &str, interrupted: bool) {
        let now = self.clock.now_utc_ms();
        let mut audit_state = self.audit_state.lock();
        if let Some(log_id) = audit_state.current_log_id.take() {
            let duration = if audit_state.current_started_at > 0 {
                now.saturating_sub(audit_state.current_started_at) as u64
            } else {
                0
            };
            let _ = self
                .audit
                .end_playback(log_id, now, duration, Some(reason), interrupted);
        }
        audit_state.current_asset_id = None;
        audit_state.current_started_at = 0;
    }

    fn save_snapshot(&self) {
        let queued: Vec<i64> = self.engine.queued().iter().map(|asset| asset.id).collect();
        let current = self.engine.current_asset();
        let snapshot = self.engine.snapshot().ok();
        let recovery = RecoverySnapshotDto {
            at_utc_ms: self.clock.now_utc_ms(),
            current_asset_id: current.as_ref().map(|asset| asset.id),
            current_position_ms: snapshot.as_ref().map(|s| s.position_ms).unwrap_or(0),
            queue: queued,
            stop_after_current: snapshot
                .as_ref()
                .map(|s| s.stop_after_current)
                .unwrap_or(false),
            talk_over: snapshot.as_ref().map(|s| s.talk_over).unwrap_or(false),
        };
        if let Ok(json) = serde_json::to_string(&recovery) {
            let _ = self.database.save_snapshot(1, &json, false);
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn app_info(state: State<'_, AppState>) -> IpcResult<AppInfoDto> {
    Ok(state.info())
}

#[tauri::command]
async fn list_categories(state: State<'_, AppState>) -> IpcResult<Vec<CategoryDto>> {
    state
        .library
        .categories()
        .map(|categories| {
            categories
                .into_iter()
                .map(|category| CategoryDto {
                    id: category.id,
                    code: category.code,
                    name: category.name,
                    color_hex: category.color_hex,
                    is_commercial: category.is_commercial,
                })
                .collect()
        })
        .map_err(IpcError::from)
}

#[tauri::command]
async fn search_library(
    state: State<'_, AppState>,
    query: LibraryQueryDto,
) -> IpcResult<Vec<AssetDto>> {
    state
        .library
        .search(LibraryQuery::from(query))
        .map(|assets| assets.into_iter().map(AssetDto::from).collect())
        .map_err(IpcError::from)
}

#[tauri::command]
async fn queue_snapshot(state: State<'_, AppState>) -> IpcResult<Vec<AssetDto>> {
    Ok(state
        .engine
        .queued()
        .into_iter()
        .map(AssetDto::from)
        .collect())
}

#[tauri::command]
async fn playout_snapshot(state: State<'_, AppState>) -> IpcResult<PlayoutSnapshotDto> {
    state.snapshot().map_err(IpcError::from)
}

fn parse_transport(value: &str) -> IpcResult<application::TransportCommand> {
    use application::TransportCommand;
    match value {
        "PLAY" => Ok(TransportCommand::Play),
        "PAUSE" => Ok(TransportCommand::Pause),
        "STOP" => Ok(TransportCommand::Stop),
        "NEXT" => Ok(TransportCommand::Next),
        "STOP_AFTER_CURRENT" => Ok(TransportCommand::StopAfterCurrent),
        other => Err(IpcError::invalid(format!(
            "unknown transport command: {other}"
        ))),
    }
}

#[tauri::command]
async fn playout_command(
    state: State<'_, AppState>,
    command: String,
) -> IpcResult<PlayoutSnapshotDto> {
    let transport = parse_transport(&command)?;
    if transport == application::TransportCommand::Stop {
        state.stop_audio();
    }
    state.engine.command(transport).map_err(IpcError::from)?;
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn enqueue_asset(state: State<'_, AppState>, asset_id: i64) -> IpcResult<PlayoutSnapshotDto> {
    let asset = state.library.get(asset_id).map_err(IpcError::from)?;
    state.engine.enqueue(asset).map_err(IpcError::from)?;
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn trigger_cart(
    state: State<'_, AppState>,
    asset_id: i64,
    r#loop: bool,
    gain_db: Option<f64>,
) -> IpcResult<PlayoutSnapshotDto> {
    let mut asset = state.library.get(asset_id).map_err(IpcError::from)?;
    if let Some(gain) = gain_db {
        asset.gain_db = gain;
    }
    state.engine.enqueue_next(asset).map_err(IpcError::from)?;
    state.engine.set_loop(r#loop).map_err(IpcError::from)?;
    state
        .engine
        .command(application::TransportCommand::Play)
        .map_err(IpcError::from)?;
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn instant_play(
    state: State<'_, AppState>,
    asset_id: i64,
    duration_ms: Option<u64>,
    r#loop: bool,
    gain_db: Option<f64>,
) -> IpcResult<PlayoutSnapshotDto> {
    let mut asset = state.library.get(asset_id).map_err(IpcError::from)?;
    if let Some(gain) = gain_db {
        asset.gain_db = gain;
    }
    state
        .engine
        .instant_play(asset, duration_ms.unwrap_or(100))
        .map_err(IpcError::from)?;
    state.engine.set_loop(r#loop).map_err(IpcError::from)?;
    state.snapshot().map_err(IpcError::from)
}
#[tauri::command]
async fn play_over(
    state: State<'_, AppState>,
    asset_id: i64,
    duration_ms: Option<u64>,
    gain_db: Option<f64>,
) -> IpcResult<PlayoutSnapshotDto> {
    let mut asset = state.library.get(asset_id).map_err(IpcError::from)?;
    if let Some(gain) = gain_db {
        asset.gain_db = gain;
    }
    state
        .engine
        .play_over(asset, duration_ms.unwrap_or(100))
        .map_err(IpcError::from)?;
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn set_loop(state: State<'_, AppState>, enabled: bool) -> IpcResult<PlayoutSnapshotDto> {
    state.engine.set_loop(enabled).map_err(IpcError::from)?;
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn stop_cart(state: State<'_, AppState>, asset_id: i64) -> IpcResult<PlayoutSnapshotDto> {
    let current = state.engine.current_asset();
    if current.map(|asset| asset.id) == Some(asset_id) {
        state.stop_audio();
        state
            .engine
            .command(application::TransportCommand::Stop)
            .map_err(IpcError::from)?;
    }
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn clear_queue(state: State<'_, AppState>) -> IpcResult<PlayoutSnapshotDto> {
    state.engine.clear().map_err(IpcError::from)?;
    state.stop_audio();
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn cart_slots(state: State<'_, AppState>, page: i64) -> IpcResult<Vec<CartSlotDto>> {
    state
        .database
        .cart_slots(page.clamp(1, 10))
        .map(|slots| slots.into_iter().map(CartSlotDto::from).collect())
        .map_err(IpcError::from)
}

#[tauri::command]
async fn import_media(
    state: State<'_, AppState>,
    path: String,
    category_id: i64,
) -> IpcResult<AssetDto> {
    state
        .library
        .import_path(Path::new(&path), category_id)
        .map(AssetDto::from)
        .map_err(IpcError::from)
}

#[tauri::command]
async fn import_folder(
    state: State<'_, AppState>,
    path: String,
    category_id: i64,
    recursive: bool,
) -> IpcResult<ImportSummaryDto> {
    let summary = state
        .library
        .import_folder(Path::new(&path), category_id, recursive)
        .map_err(IpcError::from)?;
    Ok(ImportSummaryDto {
        imported: summary.imported,
        skipped: summary.skipped,
        failed: summary.failed.len(),
        total: summary.total(),
    })
}

#[tauri::command]
async fn seek_playout(
    state: State<'_, AppState>,
    position_ms: u64,
) -> IpcResult<PlayoutSnapshotDto> {
    state.engine.seek(position_ms).map_err(IpcError::from)?;
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn recovery_status(state: State<'_, AppState>) -> IpcResult<Option<RecoverySnapshotDto>> {
    Ok(state.pending_recovery.lock().clone())
}

#[tauri::command]
async fn restore_recovery(
    state: State<'_, AppState>,
    snapshot: RecoverySnapshotDto,
) -> IpcResult<PlayoutSnapshotDto> {
    if let Some(id) = snapshot.current_asset_id {
        if let Ok(asset) = state.library.get(id) {
            let _ = state.engine.enqueue(asset);
        }
    }
    for id in &snapshot.queue {
        if let Ok(asset) = state.library.get(*id) {
            let _ = state.engine.enqueue(asset);
        }
    }
    if snapshot.current_position_ms > 0 {
        let _ = state.engine.seek(snapshot.current_position_ms);
    }
    state
        .engine
        .command(application::TransportCommand::Play)
        .map_err(IpcError::from)?;
    state.pending_recovery.lock().take();
    let _ = state.database.save_snapshot(1, "{}", true);
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn get_dsp_params(state: State<'_, AppState>) -> IpcResult<infra_audio::DspParams> {
    Ok(state.audio.dsp_params())
}

#[tauri::command]
async fn set_dsp_params(
    state: State<'_, AppState>,
    params: infra_audio::DspParams,
) -> IpcResult<()> {
    state.audio.set_dsp_params(params).map_err(IpcError::from)
}

#[tauri::command]
async fn toggle_talk_over(state: State<'_, AppState>) -> IpcResult<PlayoutSnapshotDto> {
    let current = state.engine.snapshot().map_err(IpcError::from)?.talk_over;
    state
        .engine
        .set_talk_over(!current)
        .map_err(IpcError::from)?;
    state.snapshot().map_err(IpcError::from)
}

#[tauri::command]
async fn start_mic(state: State<'_, AppState>, device: Option<String>) -> IpcResult<()> {
    state
        .audio
        .start_mic(device.as_deref())
        .map_err(IpcError::from)
}

#[tauri::command]
async fn stop_mic(state: State<'_, AppState>) -> IpcResult<()> {
    state.audio.stop_mic();
    Ok(())
}

#[tauri::command]
async fn mic_level(state: State<'_, AppState>) -> IpcResult<f32> {
    Ok(state.audio.mic_level())
}

#[tauri::command]
async fn list_input_devices() -> IpcResult<Vec<String>> {
    Ok(infra_audio::CpalPlayer::list_input_devices())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,audiomaster=debug,infra_db=debug"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

pub fn run() {
    init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let startup = std::time::Instant::now();
            let state = AppState::bootstrap(app.handle())?;
            state.spawn_ticker();

            // Telemetry channel (ADR-011): one aggregated frame at ~25 Hz.
            let telemetry_state = state.clone();
            let telemetry_handle = app.handle().clone();
            let spawned = std::thread::Builder::new()
                .name("audiomaster-telemetry".into())
                .spawn(move || {
                    use tauri::Emitter;
                    loop {
                        std::thread::sleep(Duration::from_millis(40));
                        let snapshot = telemetry_state.engine.snapshot().ok();
                        let (peak_l, peak_r) = telemetry_state.audio.peaks();
                        let payload = TelemetryPayload {
                            position_ms: snapshot.as_ref().map(|s| s.position_ms).unwrap_or(0),
                            peak_l,
                            peak_r,
                            lufs: -23.0,
                            underruns: snapshot.as_ref().map(|s| s.underruns).unwrap_or(0),
                        };
                        let _ = telemetry_handle.emit("telemetry", payload);
                    }
                });
            if let Err(error) = spawned {
                tracing::error!(%error, "could not start the telemetry thread");
            }

            app.manage(state);
            tracing::info!(
                elapsed_ms = startup.elapsed().as_millis(),
                "Audiomaster startup complete"
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            list_categories,
            search_library,
            queue_snapshot,
            playout_snapshot,
            playout_command,
            enqueue_asset,
            trigger_cart,
            instant_play,
            play_over,
            set_loop,
            stop_cart,
            clear_queue,
            cart_slots,
            import_media,
            import_folder,
            seek_playout,
            recovery_status,
            restore_recovery,
            start_mic,
            stop_mic,
            mic_level,
            list_input_devices,
            get_dsp_params,
            set_dsp_params,
            toggle_talk_over,
        ])
        .on_window_event(|app, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = state.database.save_snapshot(1, "{}", true);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running the Audiomaster shell");
}
