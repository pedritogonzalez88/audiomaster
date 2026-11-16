//! Spike A: measures Symphonia decode cost and the CPU cost of a four-deck
//! mixer at 48 kHz stereo, using the null sink so the run is reproducible.
//!
//! Run with:
//!
//! ```text
//! cargo run --release -p infra-audio --example spike_a -- <file> [<file>...]
//! ```
//!
//! With no arguments the harness synthesises 60 s of PCM and exercises the
//! mixer path only, which keeps CI able to compile and run it without a corpus.
//!
//! The null sink is a perfectly punctual consumer: it cannot underrun by
//! construction. Underrun and latency on real hardware are a Fase 1 soak item;
//! this harness reports what can be measured deterministically plus the CPAL
//! device start-up cost when a device exists.

#![allow(clippy::print_stdout)]

fn main() {
    let files: Vec<std::path::PathBuf> = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect();

    println!("spike A — decode + four-deck mix (48 kHz, stereo)");
    println!();

    let decoded = if files.is_empty() {
        println!("no corpus given: using 60 s of synthesised PCM");
        synthetic_deck(60)
    } else {
        let mut longest = None;
        for path in &files {
            let started = std::time::Instant::now();
            match infra_audio::decode_to(path, 48_000, 2) {
                Ok(audio) => {
                    let elapsed = started.elapsed();
                    let duration_ms = audio.duration_ms();
                    let decode_ms = elapsed.as_secs_f64() * 1000.0;
                    let rtf = if duration_ms == 0 {
                        f64::NAN
                    } else {
                        elapsed.as_secs_f64() / (duration_ms as f64 / 1000.0)
                    };
                    println!(
                        "decode  {:>6.1} ms  for {:>8} ms of audio  RTF {:.3}  {}",
                        decode_ms,
                        duration_ms,
                        rtf,
                        path.display()
                    );
                    if longest
                        .as_ref()
                        .map(|current: &infra_audio::DecodedAudio| {
                            audio.duration_ms() > current.duration_ms()
                        })
                        .unwrap_or(true)
                    {
                        longest = Some(audio);
                    }
                }
                Err(error) => println!("decode  FAILED  {}: {error}", path.display()),
            }
        }
        longest.unwrap_or_else(|| synthetic_deck(60))
    };

    println!();
    mixer_cost(&decoded);
    println!();
    null_sink_pass(&decoded);
    println!();
    cpal_startup();
}

fn synthetic_deck(seconds: u64) -> infra_audio::DecodedAudio {
    let frames = (seconds as usize) * 48_000;
    let mut samples = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let phase = (frame as f32) * 440.0 * std::f32::consts::TAU / 48_000.0;
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

/// Sums four decks with independent gains in 10 ms blocks for 60 s of audio.
/// This is the shape of the audio callback once the two-deck crossfade and the
/// cart wall share the mixer, so the number is a realistic upper bound of the
/// per-buffer CPU cost of one voice of the engine.
fn mixer_cost(deck: &infra_audio::DecodedAudio) {
    const SECONDS: usize = 60;
    const BLOCK: usize = 480; // 10 ms at 48 kHz
    const DECKS: usize = 4;
    let gains = [1.0_f32, 0.7, 0.5, 0.25];
    let channels = deck.channels as usize;

    let mut output = vec![0.0_f32; BLOCK * channels];
    let mut blocks = 0usize;
    let mut peak = 0.0_f32;

    let started = std::time::Instant::now();
    let total_blocks = SECONDS * 48_000 / BLOCK;
    for block in 0..total_blocks {
        output.fill(0.0);
        for (voice, gain) in gains.iter().enumerate() {
            // Four different read offsets emulate decks at different positions.
            let offset = (block * BLOCK + voice * 1_337) % deck.samples.len().max(1);
            let end = (offset + output.len()).min(deck.samples.len());
            let source = &deck.samples[offset..end];
            for (sample, input) in output.iter_mut().zip(source) {
                *sample += input * gain;
            }
        }
        peak = output
            .iter()
            .fold(peak, |acc, sample| acc.max(sample.abs()));
        blocks += 1;
    }
    let elapsed = started.elapsed();
    let audio_seconds = blocks * BLOCK / 48_000;
    let cpu_percent = elapsed.as_secs_f64() / audio_seconds as f64 * 100.0;

    println!(
        "mixer   {DECKS} decks, {audio_seconds} s of audio in {:.1} ms  CPU {:.2}% of one core  peak {:.3}",
        elapsed.as_secs_f64() * 1000.0,
        cpu_percent,
        std::hint::black_box(peak)
    );
}

/// Loads four independent null sinks and consumes a 10 ms block at a time,
/// mirroring the control ticker. A null sink never underruns: the pass proves
/// the buffer/position bookkeeping under four concurrent voices.
fn null_sink_pass(deck: &infra_audio::DecodedAudio) {
    use infra_audio::{AudioOutput, NullOutput};

    let sinks: Vec<NullOutput> = (0..4).map(|_| NullOutput::new(48_000, 2)).collect();
    let started = std::time::Instant::now();
    for sink in &sinks {
        if let Err(error) = sink.load(deck.clone()) {
            println!("null sink  load failed: {error}");
            return;
        }
        let _ = sink.play();
    }

    let mut underruns = 0_u64;
    for _ in 0..6_000 {
        for sink in &sinks {
            sink.advance_ms(10);
            if sink.is_playing() && sink.position_ms() == 0 {
                underruns += 1;
            }
        }
    }
    let elapsed = started.elapsed();
    println!(
        "sink    4 x null sink, 60 s virtual, {:.1} ms wall, underruns {underruns} (null sink is ideal)",
        elapsed.as_secs_f64() * 1000.0
    );
    for sink in &sinks {
        let _ = sink.stop();
    }
}

fn cpal_startup() {
    use infra_audio::AudioOutput;

    let started = std::time::Instant::now();
    match infra_audio::CpalPlayer::new() {
        Ok(player) => {
            let startup = started.elapsed();
            println!(
                "cpal    device \"{}\" {} Hz / {} ch, start-up {:.1} ms",
                player.device_name(),
                player.sample_rate(),
                player.channels(),
                startup.as_secs_f64() * 1000.0
            );
        }
        Err(error) => println!("cpal    no output device available: {error}"),
    }
}
