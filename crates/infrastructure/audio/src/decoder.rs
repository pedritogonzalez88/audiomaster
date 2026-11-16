use application::{AppError, AppResult};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Fully decoded, interleaved f32 audio ready for the playback sink.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl DecodedAudio {
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }

    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            0
        } else {
            self.frames() as u64 * 1000 / u64::from(self.sample_rate)
        }
    }
}

/// Decodes the default audio track and resamples/remixes it to the output
/// format. Phase 2 moves this into a streaming producer thread.
pub fn decode_to(path: &Path, target_rate: u32, target_channels: u16) -> AppResult<DecodedAudio> {
    if target_rate == 0 || target_channels == 0 {
        return Err(AppError::Audio("invalid target audio format".into()));
    }

    let file = std::fs::File::open(path)
        .map_err(|error| AppError::Audio(format!("open {}: {error}", path.display())))?;
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
        .map_err(|error| AppError::Audio(format!("probe: {error}")))?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| AppError::Audio("no default audio track".into()))?;
    if track.codec_params.codec == CODEC_TYPE_NULL {
        return Err(AppError::Audio("unsupported audio codec".into()));
    }
    let track_id = track.id;
    let source_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| AppError::Audio("unknown sample rate".into()))?;
    let source_channels = track
        .codec_params
        .channels
        .map(|channels| channels.count() as u16)
        .unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| AppError::Audio(format!("decoder: {error}")))?;

    let mut interleaved = Vec::<f32>::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(error) => return Err(AppError::Audio(format!("read packet: {error}"))),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let capacity = decoded.capacity() as u64;
                let mut buffer = SampleBuffer::<f32>::new(capacity, spec);
                buffer.copy_interleaved_ref(decoded);
                interleaved.extend_from_slice(buffer.samples());
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => return Err(AppError::Audio(format!("decode: {error}"))),
        }
    }

    if interleaved.is_empty() {
        return Err(AppError::Audio("decoded track is empty".into()));
    }

    let remixed = remix_channels(&interleaved, source_channels, target_channels);
    let samples = resample_linear(&remixed, source_rate, target_rate, target_channels);

    Ok(DecodedAudio {
        samples,
        sample_rate: target_rate,
        channels: target_channels,
    })
}

fn remix_channels(input: &[f32], source: u16, target: u16) -> Vec<f32> {
    if source == target {
        return input.to_vec();
    }
    let source = source.max(1) as usize;
    let target = target.max(1) as usize;
    let frames = input.len() / source;
    let mut output = vec![0.0f32; frames * target];

    for frame in 0..frames {
        for channel in 0..target {
            let source_channel = channel.min(source - 1);
            output[frame * target + channel] = input[frame * source + source_channel];
        }
    }
    output
}

fn resample_linear(input: &[f32], source_rate: u32, target_rate: u32, channels: u16) -> Vec<f32> {
    if source_rate == target_rate || input.is_empty() {
        return input.to_vec();
    }
    let channels = channels.max(1) as usize;
    let source_frames = input.len() / channels;
    if source_frames == 0 {
        return Vec::new();
    }

    let ratio = f64::from(target_rate) / f64::from(source_rate);
    let target_frames = (source_frames as f64 * ratio).ceil() as usize;
    let mut output = vec![0.0f32; target_frames * channels];

    for frame in 0..target_frames {
        let source_position = frame as f64 / ratio;
        let index = source_position.floor() as usize;
        let fraction = (source_position - index as f64) as f32;
        let next = (index + 1).min(source_frames - 1);

        for channel in 0..channels {
            let a = input[index * channels + channel];
            let b = input[next * channels + channel];
            output[frame * channels + channel] = a + (b - a) * fraction;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remixes_mono_to_stereo() {
        let output = remix_channels(&[0.5, 0.25], 1, 2);
        assert_eq!(output, vec![0.5, 0.5, 0.25, 0.25]);
    }

    #[test]
    fn resamples_length_proportionally() {
        let input: Vec<f32> = (0..100).map(|value| value as f32 / 100.0).collect();
        let output = resample_linear(&input, 48_000, 24_000, 1);
        assert_eq!(output.len(), 50);
    }

    #[test]
    fn same_rate_is_identity() {
        let input = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_linear(&input, 48_000, 48_000, 3), input);
    }
}
