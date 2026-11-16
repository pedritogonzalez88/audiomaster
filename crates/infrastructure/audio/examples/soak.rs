//! Soak test for the Fase 1 exit criterion: 10 consecutive tracks, 60 min of
//! audio, 0 underruns.
//!
//! Run with:
//!
//! ```text
//! cargo run --release -p infra-audio --example soak -- [file ...]
//! ```
//!
//! With no arguments the harness synthesises ten 6-minute tracks in memory, which
//! exercises the playback path without a corpus. Pass real files to measure decode
//! cost as well.

#[allow(clippy::print_stdout)]
fn main() {
    use infra_audio::AudioOutput;
    let files: Vec<std::path::PathBuf> = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect();

    let mut decode_times: Vec<f64> = Vec::new();
    let tracks: Vec<infra_audio::DecodedAudio> = if files.is_empty() {
        (0..10)
            .map(|index| synthetic_track(360, 440.0 + index as f32 * 110.0))
            .collect()
    } else {
        files
            .iter()
            .filter_map(|path| {
                let started = std::time::Instant::now();
                match infra_audio::decode_to(path, 48_000, 2) {
                    Ok(audio) => {
                        decode_times.push(started.elapsed().as_secs_f64() * 1000.0);
                        Some(audio)
                    }
                    Err(error) => {
                        println!("decode  FAILED  {}: {error}", path.display());
                        None
                    }
                }
            })
            .collect()
    };

    if tracks.is_empty() {
        println!("no tracks to play");
        return;
    }

    println!("soak — {} tracks", tracks.len());

    let started = std::time::Instant::now();
    let mut total_underruns = 0_u64;
    let mut max_decode_ms = 0.0_f64;
    let mut total_audio_ms = 0_u64;

    for (index, track) in tracks.iter().enumerate() {
        let decode_ms = decode_times.get(index).copied().unwrap_or(0.0);
        if decode_ms > max_decode_ms {
            max_decode_ms = decode_ms;
        }

        let sink = infra_audio::NullOutput::new(48_000, 2);
        sink.load(track.clone()).expect("load track");
        sink.play().expect("play track");

        let track_ms = track.duration_ms();
        // Advance in 100 ms blocks, mirroring the control ticker.
        let mut position = 0_u64;
        while position < track_ms {
            let step = 100_u64.min(track_ms - position);
            sink.advance_ms(step);
            position += step;
        }
        let underruns = sink.underruns();
        total_underruns += underruns;
        total_audio_ms += track_ms;

        sink.stop().ok();
        println!(
            "track {:>2}/{}  {:>7} ms  underruns {}",
            index + 1,
            tracks.len(),
            track_ms,
            underruns
        );
    }

    let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
    let rtf = wall_ms / total_audio_ms as f64 * 1000.0;
    println!();
    println!(
        "soak    {} tracks, {} ms of audio in {:.0} ms wall (RTF {:.4}), underruns {}",
        tracks.len(),
        total_audio_ms,
        wall_ms,
        rtf,
        total_underruns
    );
    println!(
        "decode  max {:.2} ms per track (in-memory load is O(1) after decode)",
        max_decode_ms
    );
}

fn synthetic_track(seconds: u64, frequency: f32) -> infra_audio::DecodedAudio {
    let frames = (seconds as usize) * 48_000;
    let mut samples = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let phase = (frame as f32) * frequency * std::f32::consts::TAU / 48_000.0;
        let sample = phase.sin() * 0.1;
        samples.push(sample);
        samples.push(sample);
    }
    infra_audio::DecodedAudio {
        samples,
        sample_rate: 48_000,
        channels: 2,
    }
}
