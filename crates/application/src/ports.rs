use crate::errors::AppResult;
use domain::MediaAsset;
use std::path::Path;

/// Persistence port for the media library.
pub trait MediaRepository: Send + Sync {
    fn insert(
        &self,
        category_id: i64,
        probed: &crate::library::ProbedMedia,
        file_path: &Path,
    ) -> AppResult<MediaAsset>;
    fn find_by_checksum(&self, checksum: &str) -> AppResult<Option<MediaAsset>>;
    fn get(&self, id: i64) -> AppResult<Option<MediaAsset>>;
    fn search(&self, query: &crate::library::LibraryQuery) -> AppResult<Vec<MediaAsset>>;
    fn categories(&self) -> AppResult<Vec<crate::library::Category>>;
}

/// Extracts metadata, duration, checksum and basic audio analysis.
pub trait MediaProbe: Send + Sync {
    fn probe(&self, path: &Path) -> AppResult<crate::library::ProbedMedia>;
}

/// Abstract clock so scheduler/playout can be tested with a virtual time.
pub trait Clock: Send + Sync {
    /// UTC milliseconds since the Unix epoch.
    fn now_utc_ms(&self) -> i64;
}

/// Commands accepted by the playout engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportCommand {
    Play,
    Pause,
    Stop,
    Next,
    StopAfterCurrent,
}

/// Immutable view of the engine for the UI/IPC layer.
#[derive(Debug, Clone, Default)]
pub struct PlayoutSnapshot {
    pub is_playing: bool,
    pub is_paused: bool,
    pub stop_after_current: bool,
    pub current_title: Option<String>,
    pub current_artist: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub next_title: Option<String>,
    pub queue_len: usize,
    pub talk_over: bool,
    pub underruns: u64,
    pub loop_current: bool,
    pub voice_over: bool,
    pub crossfade_progress: Option<f64>,
    pub incoming_title: Option<String>,
    pub outgoing_gain: f64,
    pub incoming_gain: f64,
}

/// Playback port implemented by the real engine and by the null engine in tests.
pub trait PlayoutPort: Send + Sync {
    fn command(&self, command: TransportCommand) -> AppResult<()>;
    fn snapshot(&self) -> AppResult<PlayoutSnapshot>;
    fn enqueue(&self, asset: MediaAsset) -> AppResult<()>;
    fn clear(&self) -> AppResult<()>;
    fn set_loop(&self, enabled: bool) -> AppResult<()>;
    fn instant_play(&self, asset: MediaAsset, duration_ms: u64) -> AppResult<()>;
    fn play_over(&self, asset: MediaAsset, duration_ms: u64) -> AppResult<()>;
    fn seek(&self, position_ms: u64) -> AppResult<()>;
    fn set_talk_over(&self, enabled: bool) -> AppResult<()>;
}

/// One row written to `playout_logs` when a track starts airing. Filled by the
/// shell when the playout engine moves to a new current track.
#[derive(Debug, Clone)]
pub struct PlayLogStart {
    pub station_id: i64,
    pub asset_id: Option<i64>,
    pub raw_title: String,
    pub raw_artist: String,
    pub category_code: String,
    pub origin: &'static str,
    pub started_at: i64,
    pub loudness_lufs: Option<f64>,
}

/// Persists playout audit entries. One row per track aired; `end_playback`
/// closes the row when the track stops or is replaced.
pub trait AuditRepository: Send + Sync {
    fn start_playback(&self, entry: &PlayLogStart) -> AppResult<i64>;
    fn end_playback(
        &self,
        id: i64,
        ended_at: i64,
        duration_played_ms: u64,
        ended_reason: Option<&str>,
        interrupted: bool,
    ) -> AppResult<()>;
}
