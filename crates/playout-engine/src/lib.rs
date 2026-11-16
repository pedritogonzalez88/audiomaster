//! Deterministic playout state machine.
//!
//! Phase 1 implements the transport and queue semantics. Phase 2 adds the
//! two-deck crossfade and real audio rendering; keeping the state machine
//! independent from the audio backend lets us test all transitions with a
//! virtual clock and a null sink.

use application::{AppError, AppResult, PlayoutPort, PlayoutSnapshot, TransportCommand};
use domain::MediaAsset;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
struct CurrentTrack {
    asset: MediaAsset,
    position_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct Crossfade {
    started_at_ms: i64,
    duration_ms: u64,
}

#[derive(Debug)]
struct Inner {
    state: EngineState,
    queue: VecDeque<MediaAsset>,
    current: Option<CurrentTrack>,
    incoming: Option<CurrentTrack>,
    bed: Option<CurrentTrack>,
    crossfade: Option<Crossfade>,
    last_tick_ms: i64,
    stop_after_current: bool,
    talk_over: bool,
    underruns: u64,
    loop_current: bool,
    crossfade_duration_ms: u64,
    over: bool,
    over_started_at: i64,
    bed_position_at_over: u64,
}

impl Inner {
    fn new() -> Self {
        Self {
            state: EngineState::Stopped,
            queue: VecDeque::new(),
            current: None,
            incoming: None,
            crossfade: None,
            last_tick_ms: 0,
            stop_after_current: false,
            talk_over: false,
            underruns: 0,
            loop_current: false,
            crossfade_duration_ms: 3000,
            over: false,
            over_started_at: 0,
            bed_position_at_over: 0,
            bed: None,
        }
    }

    fn pull_next(&mut self, now_ms: i64) -> bool {
        if self.stop_after_current {
            self.current = None;
            self.incoming = None;
            self.bed = None;
            self.crossfade = None;
            self.state = EngineState::Stopped;
            self.stop_after_current = false;
            self.over = false;
            return false;
        }

        match self.queue.pop_front() {
            Some(asset) => {
                self.current = Some(CurrentTrack {
                    asset,
                    position_ms: 0,
                });
                self.incoming = None;
                self.crossfade = None;
                self.bed = None;
                self.over = false;
                self.state = EngineState::Playing;
                self.last_tick_ms = now_ms;
                true
            }
            None => {
                self.current = None;
                self.incoming = None;
                self.bed = None;
                self.crossfade = None;
                self.over = false;
                self.state = EngineState::Stopped;
                false
            }
        }
    }

    fn advance(&mut self, now_ms: i64) {
        // Finish the current track and pull the next one (cut).
        self.incoming = None;
        self.bed = None;
        self.crossfade = None;
        self.over = false;
        self.pull_next(now_ms);
    }

    fn tick(&mut self, now_ms: i64) {
        if self.state != EngineState::Playing {
            self.last_tick_ms = now_ms;
            return;
        }

        let delta = (now_ms - self.last_tick_ms).max(0) as u64;
        self.last_tick_ms = now_ms;

        // Advance positions.
        if let Some(current) = self.current.as_mut() {
            current.position_ms = current.position_ms.saturating_add(delta);
        }
        if self.crossfade.is_some() {
            if let Some(incoming) = self.incoming.as_mut() {
                incoming.position_ms = incoming.position_ms.saturating_add(delta);
            }
        }

        // Start a crossfade when the current track reaches its mix point and a
        // next track is queued. Only when a real mix point is set.
        if self.crossfade.is_none() {
            let should_crossfade = self
                .current
                .as_ref()
                .map(|current| {
                    let mix = current.asset.cue_points.mix_point_ms as u64;
                    mix > 0 && mix < current.asset.duration_ms as u64 && current.position_ms >= mix
                })
                .unwrap_or(false)
                && !self.queue.is_empty();

            if should_crossfade {
                if let Some(next_asset) = self.queue.pop_front() {
                    self.incoming = Some(CurrentTrack {
                        asset: next_asset,
                        position_ms: 0,
                    });
                    self.crossfade = Some(Crossfade {
                        started_at_ms: now_ms,
                        duration_ms: self.crossfade_duration_ms,
                    });
                }
            }
        }

        // Complete an in-progress crossfade.
        if let Some(cf) = self.crossfade {
            let progress =
                ((now_ms - cf.started_at_ms) as f64 / cf.duration_ms as f64).clamp(0.0, 1.0);
            if progress >= 1.0 {
                if let Some(incoming) = self.incoming.take() {
                    self.current = Some(incoming);
                }
                self.crossfade = None;
            }
        }

        // End of current track (only when not crossfading).
        if self.crossfade.is_none() {
            let finished = self
                .current
                .as_ref()
                .map(|current| current.position_ms >= current.asset.duration_ms as u64)
                .unwrap_or(true);
            if finished {
                if self.over {
                    if let Some(bed) = self.bed.take() {
                        let elapsed = now_ms - self.over_started_at;
                        let mut bed = bed;
                        bed.position_ms =
                            (bed.position_ms + elapsed as u64).min(bed.asset.duration_ms as u64);
                        self.current = Some(bed);
                        self.over = false;
                        self.talk_over = false;
                        self.last_tick_ms = now_ms;
                        return;
                    }
                    self.over = false;
                    self.talk_over = false;
                }
                if self.loop_current {
                    if let Some(current) = self.current.as_mut() {
                        current.position_ms = 0;
                    }
                    self.last_tick_ms = now_ms;
                } else {
                    self.advance(now_ms);
                }
            }
        }
    }
}

/// Thread-safe playout engine. The real audio thread reads from a separate
/// lock-free ring buffer; this mutex only protects control-plane state.
pub struct PlayoutEngine {
    inner: Mutex<Inner>,
    clock: Arc<dyn application::Clock>,
}

impl PlayoutEngine {
    pub fn new(clock: Arc<dyn application::Clock>) -> Self {
        Self {
            inner: Mutex::new(Inner::new()),
            clock,
        }
    }

    /// Advances internal time. In production this is called by the control
    /// thread every 5-10 ms; in tests it is called with a virtual clock.
    pub fn tick(&self) {
        let now = self.clock.now_utc_ms();
        self.inner.lock().tick(now);
    }

    pub fn state(&self) -> EngineState {
        self.inner.lock().state
    }

    pub fn queue_len(&self) -> usize {
        self.inner.lock().queue.len()
    }

    /// Copy of the queued assets, in play order. Used by the IPC layer to render
    /// the "Playlist en Aire" panel.
    pub fn queued(&self) -> Vec<MediaAsset> {
        self.inner.lock().queue.iter().cloned().collect()
    }

    /// Asset currently on air, if any.
    pub fn current_asset(&self) -> Option<MediaAsset> {
        self.inner
            .lock()
            .current
            .as_ref()
            .map(|current| current.asset.clone())
    }

    /// Asset that is being crossfaded in, if any.
    pub fn incoming_asset(&self) -> Option<MediaAsset> {
        self.inner
            .lock()
            .incoming
            .as_ref()
            .map(|incoming| incoming.asset.clone())
    }

    /// Records an audio underrun detected by the output device. Exposed in the
    /// snapshot so the UI and audit layer can react.
    pub fn record_underrun(&self) {
        let mut inner = self.inner.lock();
        inner.underruns = inner.underruns.saturating_add(1);
    }

    /// Inserts an asset at the head of the queue. Cart wall pads use this so the
    /// jingle/ad fires as soon as the current track ends.
    pub fn enqueue_next(&self, asset: MediaAsset) -> AppResult<()> {
        asset.validate().map_err(AppError::Domain)?;
        self.inner.lock().queue.push_front(asset);
        Ok(())
    }
}

impl PlayoutPort for PlayoutEngine {
    fn command(&self, command: TransportCommand) -> AppResult<()> {
        let now = self.clock.now_utc_ms();
        let mut inner = self.inner.lock();
        inner.tick(now);

        match command {
            TransportCommand::Play => {
                if inner.current.is_none() {
                    inner.pull_next(now);
                } else if inner.state == EngineState::Paused {
                    inner.state = EngineState::Playing;
                }
                if inner.current.is_some() {
                    inner.state = EngineState::Playing;
                }
                inner.last_tick_ms = now;
            }
            TransportCommand::Pause => {
                if inner.state == EngineState::Playing {
                    inner.state = EngineState::Paused;
                } else if inner.state == EngineState::Paused {
                    inner.state = EngineState::Playing;
                }
                inner.last_tick_ms = now;
            }
            TransportCommand::Stop => {
                inner.current = None;
                inner.incoming = None;
                inner.bed = None;
                inner.crossfade = None;
                inner.over = false;
                inner.talk_over = false;
                inner.state = EngineState::Stopped;
                inner.last_tick_ms = now;
            }
            TransportCommand::Next => {
                inner.advance(now);
            }
            TransportCommand::StopAfterCurrent => {
                inner.stop_after_current = !inner.stop_after_current;
            }
        }

        Ok(())
    }

    fn snapshot(&self) -> AppResult<PlayoutSnapshot> {
        let now = self.clock.now_utc_ms();
        let mut inner = self.inner.lock();
        inner.tick(now);

        let crossfade_progress = inner
            .crossfade
            .map(|cf| ((now - cf.started_at_ms) as f64 / cf.duration_ms as f64).clamp(0.0, 1.0));
        let incoming_title = inner.incoming.as_ref().map(|t| t.asset.title.clone());
        let (outgoing_gain, incoming_gain) = match crossfade_progress {
            Some(p) => domain::crossfade::equal_power_gains(p),
            None => (1.0, 0.0),
        };

        let (current_title, current_artist, position_ms, duration_ms) = inner
            .current
            .as_ref()
            .map(|current| {
                (
                    Some(current.asset.title.clone()),
                    Some(current.asset.artist.clone()),
                    current.position_ms,
                    current.asset.duration_ms as u64,
                )
            })
            .unwrap_or((None, None, 0, 0));

        Ok(PlayoutSnapshot {
            is_playing: inner.state == EngineState::Playing,
            is_paused: inner.state == EngineState::Paused,
            stop_after_current: inner.stop_after_current,
            current_title,
            current_artist,
            position_ms,
            duration_ms,
            next_title: inner
                .incoming
                .as_ref()
                .map(|t| t.asset.title.clone())
                .or_else(|| inner.queue.front().map(|a| a.title.clone())),
            queue_len: inner.queue.len() + if inner.incoming.is_some() { 1 } else { 0 },
            talk_over: inner.talk_over,
            underruns: inner.underruns,
            loop_current: inner.loop_current,
            voice_over: inner.over,
            crossfade_progress,
            incoming_title,
            outgoing_gain,
            incoming_gain,
        })
    }

    fn enqueue(&self, asset: MediaAsset) -> AppResult<()> {
        asset.validate().map_err(AppError::Domain)?;
        self.inner.lock().queue.push_back(asset);
        Ok(())
    }

    fn clear(&self) -> AppResult<()> {
        let mut inner = self.inner.lock();
        inner.queue.clear();
        inner.current = None;
        inner.incoming = None;
        inner.crossfade = None;
        inner.bed = None;
        inner.over = false;
        inner.state = EngineState::Stopped;
        inner.talk_over = false;
        inner.stop_after_current = false;
        inner.loop_current = false;
        Ok(())
    }

    fn seek(&self, position_ms: u64) -> AppResult<()> {
        let mut inner = self.inner.lock();
        if let Some(current) = inner.current.as_mut() {
            current.position_ms = position_ms.min(current.asset.duration_ms as u64);
        }
        Ok(())
    }

    fn set_talk_over(&self, enabled: bool) -> AppResult<()> {
        self.inner.lock().talk_over = enabled;
        Ok(())
    }

    fn set_loop(&self, enabled: bool) -> AppResult<()> {
        self.inner.lock().loop_current = enabled;
        Ok(())
    }

    fn instant_play(&self, asset: MediaAsset, duration_ms: u64) -> AppResult<()> {
        asset.validate().map_err(AppError::Domain)?;
        let now = self.clock.now_utc_ms();
        let mut inner = self.inner.lock();
        inner.tick(now);
        if inner.current.is_none() {
            inner.current = Some(CurrentTrack {
                asset,
                position_ms: 0,
            });
            inner.state = EngineState::Playing;
            inner.last_tick_ms = now;
            return Ok(());
        }
        inner.incoming = Some(CurrentTrack {
            asset,
            position_ms: 0,
        });
        inner.crossfade = Some(Crossfade {
            started_at_ms: now,
            duration_ms: duration_ms.max(1),
        });
        inner.state = EngineState::Playing;
        inner.last_tick_ms = now;
        Ok(())
    }

    fn play_over(&self, asset: MediaAsset, duration_ms: u64) -> AppResult<()> {
        asset.validate().map_err(AppError::Domain)?;
        let now = self.clock.now_utc_ms();
        let mut inner = self.inner.lock();
        inner.tick(now);
        if inner.current.is_none() {
            inner.current = Some(CurrentTrack {
                asset,
                position_ms: 0,
            });
            inner.state = EngineState::Playing;
            inner.last_tick_ms = now;
            return Ok(());
        }
        if let Some(current) = inner.current.clone() {
            inner.bed = Some(current);
            inner.bed_position_at_over = inner.current.as_ref().map(|c| c.position_ms).unwrap_or(0);
            inner.over_started_at = now;
        }
        inner.over = true;
        inner.talk_over = true;
        inner.incoming = Some(CurrentTrack {
            asset,
            position_ms: 0,
        });
        inner.crossfade = Some(Crossfade {
            started_at_ms: now,
            duration_ms: duration_ms.max(1),
        });
        inner.state = EngineState::Playing;
        inner.last_tick_ms = now;
        Ok(())
    }
}

/// Clock backed by the operating system. Injectable for tests.
pub struct SystemClock;

impl application::Clock for SystemClock {
    fn now_utc_ms(&self) -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{AssetKind, CuePoints};
    use std::sync::atomic::{AtomicI64, Ordering};

    struct ManualClock(AtomicI64);

    impl ManualClock {
        fn new(start: i64) -> Arc<Self> {
            Arc::new(Self(AtomicI64::new(start)))
        }
        fn advance(&self, ms: i64) {
            self.0.fetch_add(ms, Ordering::SeqCst);
        }
    }

    impl application::Clock for ManualClock {
        fn now_utc_ms(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn asset(id: i64, title: &str, duration_ms: u32) -> MediaAsset {
        MediaAsset {
            id,
            category_id: 1,
            title: title.into(),
            artist: "Artist".into(),
            album: None,
            release_year: None,
            bpm: 120,
            kind: AssetKind::Audio,
            file_path: format!("C:/media/{title}.mp3"),
            checksum_sha256: format!("checksum-{id}"),
            duration_ms,
            gain_db: 0.0,
            loudness_lufs: -23.0,
            cue_points: CuePoints::whole_file(duration_ms),
            play_count: 0,
            last_played_at: None,
        }
    }

    fn engine() -> (Arc<PlayoutEngine>, Arc<ManualClock>) {
        let clock = ManualClock::new(1_000);
        let engine = Arc::new(PlayoutEngine::new(clock.clone()));
        (engine, clock)
    }

    #[test]
    fn plays_queue_in_order() {
        let (engine, clock) = engine();
        engine.enqueue(asset(1, "One", 10_000)).unwrap();
        engine.enqueue(asset(2, "Two", 10_000)).unwrap();

        engine.command(TransportCommand::Play).unwrap();
        let snapshot = engine.snapshot().unwrap();
        assert_eq!(snapshot.current_title.as_deref(), Some("One"));
        assert!(snapshot.is_playing);

        clock.advance(5_000);
        engine.tick();
        let snapshot = engine.snapshot().unwrap();
        assert_eq!(snapshot.position_ms, 5_000);

        clock.advance(5_000);
        engine.tick();
        let snapshot = engine.snapshot().unwrap();
        assert_eq!(snapshot.current_title.as_deref(), Some("Two"));
        assert_eq!(snapshot.position_ms, 0);
        assert_eq!(snapshot.queue_len, 0);
    }

    #[test]
    fn stop_after_current_pauses_at_end() {
        let (engine, clock) = engine();
        engine.enqueue(asset(1, "One", 1_000)).unwrap();
        engine.enqueue(asset(2, "Two", 1_000)).unwrap();
        engine.command(TransportCommand::Play).unwrap();
        engine.command(TransportCommand::StopAfterCurrent).unwrap();

        clock.advance(1_500);
        engine.tick();

        let snapshot = engine.snapshot().unwrap();
        assert!(!snapshot.is_playing);
        assert!(snapshot.current_title.is_none());
        assert_eq!(snapshot.queue_len, 1);
    }

    #[test]
    fn pause_freezes_position() {
        let (engine, clock) = engine();
        engine.enqueue(asset(1, "One", 10_000)).unwrap();
        engine.command(TransportCommand::Play).unwrap();

        clock.advance(2_000);
        engine.tick();
        engine.command(TransportCommand::Pause).unwrap();

        clock.advance(5_000);
        engine.tick();

        let snapshot = engine.snapshot().unwrap();
        assert!(snapshot.is_paused);
        assert_eq!(snapshot.position_ms, 2_000);
    }

    #[test]
    fn next_skips_current() {
        let (engine, _clock) = engine();
        engine.enqueue(asset(1, "One", 10_000)).unwrap();
        engine.enqueue(asset(2, "Two", 10_000)).unwrap();
        engine.command(TransportCommand::Play).unwrap();
        engine.command(TransportCommand::Next).unwrap();

        let snapshot = engine.snapshot().unwrap();
        assert_eq!(snapshot.current_title.as_deref(), Some("Two"));
        assert_eq!(snapshot.queue_len, 0);
    }

    #[test]
    fn clear_stops_everything() {
        let (engine, _clock) = engine();
        engine.enqueue(asset(1, "One", 10_000)).unwrap();
        engine.command(TransportCommand::Play).unwrap();
        engine.clear().unwrap();

        let snapshot = engine.snapshot().unwrap();
        assert!(!snapshot.is_playing);
        assert!(snapshot.current_title.is_none());
        assert_eq!(snapshot.queue_len, 0);
    }

    #[test]
    fn seek_moves_position() {
        let clock = ManualClock::new(0);
        let engine = PlayoutEngine::new(clock.clone());
        let asset = asset(1, "One", 10_000);
        engine.enqueue(asset).unwrap();
        engine.command(TransportCommand::Play).unwrap();
        clock.advance(1000);
        engine.tick();
        engine.seek(5000).unwrap();
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.position_ms, 5000);
    }

    #[test]
    fn crossfade_starts_at_mix_point() {
        let clock = ManualClock::new(0);
        let engine = PlayoutEngine::new(clock.clone());
        let mut a = asset(1, "A", 10_000);
        a.cue_points.mix_point_ms = 4000;
        a.cue_points.fade_out_ms = 4000;
        let b = asset(2, "B", 10_000);
        engine.enqueue(a).unwrap();
        engine.enqueue(b).unwrap();
        engine.command(TransportCommand::Play).unwrap();
        // advance to mix point
        clock.advance(4000);
        engine.tick();
        let snap = engine.snapshot().unwrap();
        assert!(snap.crossfade_progress.is_some());
        assert_eq!(snap.incoming_title.as_deref(), Some("B"));
        // outgoing + incoming gains follow equal-power
        let p = snap.crossfade_progress.unwrap();
        let (out, inc) = domain::crossfade::equal_power_gains(p);
        assert!((snap.outgoing_gain - out).abs() < 1e-9);
        assert!((snap.incoming_gain - inc).abs() < 1e-9);
    }

    #[test]
    fn crossfade_completes_and_switches_current() {
        let clock = ManualClock::new(0);
        let engine = PlayoutEngine::new(clock.clone());
        let mut a = asset(1, "A", 10_000);
        a.cue_points.mix_point_ms = 4000;
        let b = asset(2, "B", 10_000);
        engine.enqueue(a).unwrap();
        engine.enqueue(b).unwrap();
        engine.command(TransportCommand::Play).unwrap();
        clock.advance(4000);
        engine.tick();
        // complete crossfade (3000 ms)
        clock.advance(3000);
        engine.tick();
        let snap = engine.snapshot().unwrap();
        assert!(snap.crossfade_progress.is_none());
        assert_eq!(snap.current_title.as_deref(), Some("B"));
    }

    #[test]
    fn instant_play_starts_crossfade() {
        let clock = ManualClock::new(0);
        let engine = PlayoutEngine::new(clock.clone());
        let a = asset(1, "A", 10_000);
        let b = asset(2, "B", 10_000);
        engine.enqueue(a).unwrap();
        engine.command(TransportCommand::Play).unwrap();
        engine.instant_play(b, 100).unwrap();
        let snap = engine.snapshot().unwrap();
        assert!(snap.crossfade_progress.is_some());
        assert_eq!(snap.incoming_title.as_deref(), Some("B"));
    }

    #[test]
    fn instant_play_on_stopped_starts_directly() {
        let clock = ManualClock::new(0);
        let engine = PlayoutEngine::new(clock);
        let b = asset(2, "B", 10_000);
        engine.instant_play(b, 100).unwrap();
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.current_title.as_deref(), Some("B"));
        assert!(snap.crossfade_progress.is_none());
    }

    #[test]
    fn play_over_restores_bed_after_cart_ends() {
        let clock = ManualClock::new(0);
        let engine = PlayoutEngine::new(clock.clone());
        let a = asset(1, "A", 10_000);
        let b = asset(2, "B", 1_000);
        engine.enqueue(a).unwrap();
        engine.command(TransportCommand::Play).unwrap();
        clock.advance(2000);
        engine.tick();
        engine.play_over(b, 50).unwrap();
        // complete crossfade to B
        clock.advance(100);
        engine.tick();
        assert_eq!(
            engine.snapshot().unwrap().current_title.as_deref(),
            Some("B")
        );
        assert!(engine.snapshot().unwrap().voice_over);
        // B finishes (1000ms)
        clock.advance(1000);
        engine.tick();
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.current_title.as_deref(), Some("A"));
        assert!(!snap.voice_over);
    }

    #[test]
    fn record_underrun_increments() {
        let clock = ManualClock::new(0);
        let engine = PlayoutEngine::new(clock);
        engine.record_underrun();
        engine.record_underrun();
        assert_eq!(engine.snapshot().unwrap().underruns, 2);
    }

    #[test]
    fn talk_over_flag_is_exposed() {
        let (engine, _clock) = engine();
        engine.set_talk_over(true).unwrap();
        assert!(engine.snapshot().unwrap().talk_over);
        engine.set_talk_over(false).unwrap();
        assert!(!engine.snapshot().unwrap().talk_over);
    }
}
