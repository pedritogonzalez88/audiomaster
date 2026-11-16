//! Measures the cart-wall trigger latency of the playout engine.
//!
//! Run with: cargo run --release -p playout-engine --example cart_latency

#[allow(clippy::print_stdout)]
fn main() {
    use application::{Clock, PlayoutPort, TransportCommand};
    use playout_engine::PlayoutEngine;
    use std::sync::Arc;
    use std::time::Instant;

    struct ManualClock(std::sync::atomic::AtomicI64);
    impl ManualClock {
        fn new(start: i64) -> Arc<Self> {
            Arc::new(Self(std::sync::atomic::AtomicI64::new(start)))
        }
    }
    impl Clock for ManualClock {
        fn now_utc_ms(&self) -> i64 {
            self.0.load(std::sync::atomic::Ordering::SeqCst)
        }
    }
    impl ManualClock {
        fn advance(&self, ms: i64) {
            self.0.fetch_add(ms, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn asset(id: i64, title: &str, duration_ms: u32) -> domain::MediaAsset {
        domain::MediaAsset {
            id,
            category_id: 1,
            title: title.into(),
            artist: "A".into(),
            album: None,
            release_year: None,
            bpm: 120,
            kind: domain::AssetKind::Audio,
            file_path: format!("x/{id}.mp3"),
            checksum_sha256: format!("{id}"),
            duration_ms,
            gain_db: 0.0,
            loudness_lufs: -23.0,
            cue_points: domain::CuePoints::whole_file(duration_ms),
            play_count: 0,
            last_played_at: None,
        }
    }

    let clock = ManualClock::new(0);
    let engine = PlayoutEngine::new(clock.clone());

    // Seed the queue with a bed track.
    engine.enqueue(asset(1, "Bed", 60_000)).unwrap();
    engine.command(TransportCommand::Play).unwrap();

    let mut latencies: Vec<f64> = Vec::with_capacity(1000);
    for i in 0..1000 {
        let cart = asset(1000 + i as i64, "Cart", 5_000);
        let started = Instant::now();
        engine.enqueue_next(cart).unwrap();
        engine.command(TransportCommand::Play).unwrap();
        let _ = engine.snapshot().unwrap();
        latencies.push(started.elapsed().as_secs_f64() * 1000.0);
        // Advance time a bit so the engine keeps playing.
        clock.advance(10);
        engine.tick();
    }

    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];
    let max = latencies.last().copied().unwrap_or(0.0);
    println!(
        "cart latency  n={}  p50 {:.3} ms  p95 {:.3} ms  p99 {:.3} ms  max {:.3} ms",
        latencies.len(),
        p50,
        p95,
        p99,
        max
    );
    if p95 <= 30.0 {
        println!("PASS: p95 <= 30 ms");
    } else {
        println!("FAIL: p95 > 30 ms");
    }
}
