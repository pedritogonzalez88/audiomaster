use application::{AppResult, ProbedMedia};
use domain::{AssetKind, CuePoints};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey, Value};
use symphonia::core::probe::Hint;

/// Media probe based on Symphonia + SHA-256.
pub struct SymphoniaProbe;

impl Default for SymphoniaProbe {
    fn default() -> Self {
        Self
    }
}

impl SymphoniaProbe {
    pub fn new() -> Self {
        Self
    }
}

impl application::MediaProbe for SymphoniaProbe {
    fn probe(&self, path: &Path) -> AppResult<ProbedMedia> {
        let checksum = sha256_file(path)?;
        let file = std::fs::File::open(path).map_err(|error| {
            application::AppError::Probe(format!("open {}: {error}", path.display()))
        })?;
        let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            hint.with_extension(extension);
        }

        let mut probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|error| application::AppError::Probe(format!("probe: {error}")))?;

        let mut format = probed.format;

        // Metadata may live in the container (format reader) or in a sidecar
        // that the probe read before the first frame (e.g. ID3). Check both and
        // let the container win on conflicts.
        let mut tags = Tags::default();
        {
            let mut metadata = format.metadata();
            merge_tags(&mut tags, metadata.skip_to_latest());
        }
        if let Some(mut metadata) = probed.metadata.get() {
            merge_tags(&mut tags, metadata.skip_to_latest());
        }

        let codec_params = format
            .default_track()
            .ok_or_else(|| application::AppError::Probe("no default track".into()))?
            .codec_params
            .clone();

        if codec_params.codec == CODEC_TYPE_NULL {
            return Err(application::AppError::Probe(
                "unsupported or missing audio codec".into(),
            ));
        }

        let title = tags
            .title
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("Sin título")
                    .to_string()
            });
        let artist = tags.artist.unwrap_or_default();
        let duration_ms = duration_ms(&codec_params).or_else(|| {
            count_frames_from_path(path).map(|(frames, rate)| frames_to_ms(frames, Some(rate)))
        });

        let duration_ms = duration_ms
            .filter(|value| *value > 0)
            .ok_or_else(|| application::AppError::Probe("could not determine duration".into()))?;

        let (bpm, mix_point_ms) = analyze_audio(path, duration_ms);

        let mut cue_points = CuePoints::whole_file(duration_ms);
        cue_points.mix_point_ms = mix_point_ms;
        cue_points.fade_out_ms = mix_point_ms;

        Ok(ProbedMedia {
            title,
            artist,
            album: tags.album,
            release_year: tags.year,
            bpm,
            kind: kind_for(path),
            duration_ms,
            checksum_sha256: checksum,
            loudness_lufs: -23.0,
            cue_points,
        })
    }
}

#[derive(Default)]
struct Tags {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    year: Option<i32>,
}

fn merge_tags(tags: &mut Tags, revision: Option<&MetadataRevision>) {
    let Some(revision) = revision else {
        return;
    };

    for tag in revision.tags() {
        let value = match &tag.value {
            Value::String(value) => Some(value.trim().to_string()),
            Value::UnsignedInt(value) => Some(value.to_string()),
            Value::SignedInt(value) => Some(value.to_string()),
            _ => None,
        };
        let Some(value) = value.filter(|value| !value.is_empty()) else {
            continue;
        };

        // First writer wins: the container is merged before the probe sidecar.
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) if tags.title.is_none() => tags.title = Some(value),
            Some(StandardTagKey::Artist) if tags.artist.is_none() => tags.artist = Some(value),
            Some(StandardTagKey::Album) if tags.album.is_none() => tags.album = Some(value),
            Some(StandardTagKey::Date) | Some(StandardTagKey::OriginalDate)
                if tags.year.is_none() =>
            {
                tags.year = value.get(0..4).and_then(|year| year.parse::<i32>().ok())
            }
            _ => {}
        }
    }
}

fn duration_ms(params: &symphonia::core::codecs::CodecParameters) -> Option<u32> {
    let frames = params.n_frames?;
    if let Some(time_base) = params.time_base {
        let time = time_base.calc_time(frames);
        let millis = time.seconds.saturating_mul(1000) + (time.frac * 1000.0).round() as u64;
        return u32::try_from(millis).ok();
    }
    params
        .sample_rate
        .map(|rate| frames_to_ms(frames, Some(rate)))
}

fn frames_to_ms(frames: u64, sample_rate: Option<u32>) -> u32 {
    let rate = sample_rate.unwrap_or(48_000).max(1);
    u32::try_from(frames.saturating_mul(1000) / u64::from(rate)).unwrap_or(u32::MAX)
}

fn count_frames_from_path(path: &Path) -> Option<(u64, u32)> {
    // Containers such as some MP3 streams do not expose n_frames. Decode the
    // file once to count frames; this is only hit when metadata is incomplete.
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let mut format = probed.format;
    let track = format.default_track()?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(48_000);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    let mut frames = 0u64;
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => frames += decoded.frames() as u64,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(_) => break,
        }
    }

    (frames > 0).then_some((frames, sample_rate))
}

fn analyze_audio(path: &Path, duration_ms: u32) -> (u32, u32) {
    let Some((samples, sample_rate)) = decode_full(path) else {
        return (0, duration_ms);
    };
    let bpm = estimate_bpm_from_samples(&samples, sample_rate);
    let mix = detect_mix_point(&samples, sample_rate, duration_ms).unwrap_or(duration_ms);
    (bpm, mix)
}

fn decode_full(path: &Path) -> Option<(Vec<f32>, u32)> {
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let mut format = probed.format;
    let track = format.default_track()?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(48_000);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    let mut samples = Vec::new();
    let max_frames = sample_rate as usize * 600; // cap at 10 min
    let mut frames_decoded = 0usize;
    while frames_decoded < max_frames {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(_) => break,
        };
        let spec = *decoded.spec();
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buffer.copy_interleaved_ref(decoded);
        let channels = spec.channels.count().max(1);
        for frame in buffer.samples().chunks(channels) {
            let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
            samples.push(mono);
        }
        frames_decoded += buffer.samples().len() / channels;
    }

    if samples.is_empty() {
        None
    } else {
        Some((samples, sample_rate))
    }
}

/// Public helper used by tests and the scheduler to detect a mix point.
pub fn detect_mix_point(samples: &[f32], sample_rate: u32, duration_ms: u32) -> Option<u32> {
    if samples.is_empty() {
        return None;
    }
    let window = (sample_rate as usize / 20).max(1); // 50 ms
    let mut rms_db: Vec<f32> = Vec::new();
    let mut i = 0;
    while i + window <= samples.len() {
        let rms =
            (samples[i..i + window].iter().map(|s| s * s).sum::<f32>() / window as f32).sqrt();
        let db = if rms > 1e-9 {
            20.0 * rms.log10()
        } else {
            -120.0
        };
        rms_db.push(db);
        i += window;
    }
    if rms_db.len() < 40 {
        return None;
    }

    let threshold = -14.0_f32;
    let min_run = 4usize;
    let start_search = rms_db.len() * 3 / 5;
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut run_start: Option<usize> = None;
    for (idx, &db) in rms_db.iter().enumerate().skip(start_search) {
        if db < threshold {
            if run_start.is_none() {
                run_start = Some(idx);
            }
        } else if let Some(s) = run_start.take() {
            if idx - s >= min_run {
                runs.push((s, idx));
            }
        }
    }
    if let Some(s) = run_start {
        if rms_db.len() - s >= min_run {
            runs.push((s, rms_db.len()));
        }
    }
    let (start, _) = runs.last()?;
    let ms = (start * window) as u64 * 1000 / sample_rate as u64;
    let ms = ms as u32;
    if ms < 30_000 || ms > duration_ms.saturating_sub(5_000) {
        return None;
    }
    Some(ms)
}

fn estimate_bpm_from_samples(samples: &[f32], sample_rate: u32) -> u32 {
    if samples.len() < sample_rate as usize * 5 {
        return 0;
    }
    let window = (sample_rate as usize / 100).max(1);
    let mut envelope: Vec<f32> = Vec::new();
    let mut index = 0;
    while index + window <= samples.len() {
        let energy: f32 = samples[index..index + window]
            .iter()
            .map(|s| s * s)
            .sum::<f32>()
            / window as f32;
        envelope.push(energy.sqrt());
        index += window;
    }
    if envelope.len() < 200 {
        return 0;
    }
    let mut onset = vec![0.0_f32; envelope.len()];
    for k in 1..envelope.len() {
        let diff = envelope[k] - envelope[k - 1];
        if diff > 0.0 {
            onset[k] = diff;
        }
    }
    let env_rate = 100.0_f32;
    let min_lag = (env_rate * 60.0 / 200.0) as usize;
    let max_lag = (env_rate * 60.0 / 60.0) as usize;
    let mut best_lag = 0usize;
    let mut best_corr = 0.0_f32;
    for lag in min_lag..=max_lag {
        let n = onset.len() - lag;
        let mut corr = 0.0_f32;
        for k in 0..n {
            corr += onset[k] * onset[k + lag];
        }
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag;
        }
    }
    if best_lag == 0 {
        return 0;
    }
    let bpm = (env_rate * 60.0 / best_lag as f32).round() as u32;
    if (60..=200).contains(&bpm) {
        bpm
    } else {
        0
    }
}

fn kind_for(path: &Path) -> AssetKind {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "m4v" => AssetKind::Video,
        _ => AssetKind::Audio,
    }
}

fn sha256_file(path: &Path) -> AppResult<String> {
    let mut file = std::fs::File::open(path).map_err(|error| {
        application::AppError::Probe(format!("open {}: {error}", path.display()))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| application::AppError::Probe(format!("read: {error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mix_point_on_silence() {
        // 40 s of noise, then 1 s of silence, then 5 s of noise.
        let sample_rate = 1000u32;
        let mut samples = Vec::new();
        for i in 0..40000 {
            samples.push((i as f32 * 0.1).sin() * 0.5);
        }
        samples.resize(samples.len() + 1000, 0.0);
        for i in 0..5000 {
            samples.push((i as f32 * 0.1).sin() * 0.5);
        }
        let duration_ms = (samples.len() as u64 * 1000 / sample_rate as u64) as u32;
        let result = detect_mix_point(&samples, sample_rate, duration_ms);
        // Should find a mix point in the silence region (~5000 ms).
        assert!(result.is_some(), "expected a mix point");
        let mp = result.unwrap();
        assert!(
            (39500..=40600).contains(&mp),
            "mix point {mp} out of expected range"
        );
    }

    #[test]
    fn detects_video_containers() {
        assert_eq!(kind_for(Path::new("clip.MP4")), AssetKind::Video);
        assert_eq!(kind_for(Path::new("song.mp3")), AssetKind::Audio);
        assert_eq!(kind_for(Path::new("no_extension")), AssetKind::Audio);
    }

    #[test]
    fn converts_frames_to_ms() {
        assert_eq!(frames_to_ms(48_000, Some(48_000)), 1_000);
        assert_eq!(frames_to_ms(44_100, Some(44_100)), 1_000);
        assert_eq!(frames_to_ms(0, None), 0);
    }

    #[test]
    fn decodes_wav_metadata() {
        // The corpus WAV is generated by tests/generate_corpus.py in CI.
        let path = Path::new("../../tests/corpus/tone_1s.wav");
        if !path.exists() {
            return;
        }
        let probe = SymphoniaProbe::new();
        let probed = application::MediaProbe::probe(&probe, path).unwrap();
        assert!(probed.duration_ms >= 900);
        assert!(probed.duration_ms <= 1_100);
        assert!(!probed.checksum_sha256.is_empty());
    }
}
