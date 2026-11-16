use crate::decoder::DecodedAudio;
use application::{AppError, AppResult};
use arc_swap::ArcSwapOption;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use parking_lot::Mutex;
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no output audio device available")]
    NoOutputDevice,
    #[error("output device configuration: {0}")]
    Config(String),
    #[error("could not build output stream: {0}")]
    BuildStream(String),
    #[error("unsupported sample format: {0:?}")]
    UnsupportedFormat(SampleFormat),
    #[error("audio buffer format mismatch: expected {expected_rate} Hz/{expected_channels} ch, got {actual_rate} Hz/{actual_channels} ch")]
    FormatMismatch {
        expected_rate: u32,
        expected_channels: u16,
        actual_rate: u32,
        actual_channels: u16,
    },
}

impl From<AudioError> for AppError {
    fn from(error: AudioError) -> Self {
        AppError::Audio(error.to_string())
    }
}

/// Playback sink interface. The real implementation is CPAL; tests use
/// `NullOutput` so the domain/application layers never require hardware.
pub trait AudioOutput: Send + Sync {
    fn load(&self, audio: DecodedAudio) -> AppResult<()>;
    fn play(&self) -> AppResult<()>;
    fn pause(&self) -> AppResult<()>;
    fn stop(&self) -> AppResult<()>;
    fn seek_ms(&self, position_ms: u64) -> AppResult<()>;
    fn position_ms(&self) -> u64;
    fn is_finished(&self) -> bool;
    fn is_playing(&self) -> bool;
    fn peaks(&self) -> (f32, f32);
    fn underruns(&self) -> u64;
    fn set_talk_over(&self, enabled: bool) -> AppResult<()>;
    fn start_mic(&self, _device: Option<&str>) -> AppResult<()> {
        Ok(())
    }
    fn stop_mic(&self) {}
    fn mic_level(&self) -> f32 {
        0.0
    }
    // list_input_devices is an inherent associated function on CpalPlayer.
    fn set_dsp_params(&self, params: crate::dsp::DspParams) -> AppResult<()> {
        let _ = params;
        Ok(())
    }
    fn dsp_params(&self) -> crate::dsp::DspParams {
        crate::dsp::DspParams::default()
    }
    fn load_incoming(&self, audio: DecodedAudio) -> AppResult<()> {
        let _ = audio;
        Ok(())
    }
    fn set_crossfade_progress(&self, progress: Option<f64>) {
        let _ = progress;
    }
    fn clear_incoming(&self) {}
    fn set_gain_db(&self, gain_db: f64) -> AppResult<()>;
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;
    fn device_name(&self) -> &str;
}

pub struct PlayerShared {
    buffer: ArcSwapOption<DecodedAudio>,
    position_frames: AtomicU64,
    playing: AtomicBool,
    finished: AtomicBool,
    gain: AtomicU32,
    sample_rate: u32,
    channels: u16,
    underruns: AtomicU64,
    peak_l: AtomicU32,
    peak_r: AtomicU32,
    talk_over: AtomicBool,
    duck_gain: AtomicU32,
    dsp: crate::dsp::DspMutex,
    // Second deck for crossfade.
    incoming_buffer: ArcSwapOption<DecodedAudio>,
    incoming_position_frames: AtomicU64,
    incoming_gain: AtomicU32,
    crossfade_progress: AtomicU32,
    mic_consumer: Mutex<Option<ringbuf::HeapCons<f32>>>,
    mic_enabled: AtomicBool,
    mic_level: AtomicU32,
    mic_active: AtomicBool,
}

impl PlayerShared {
    fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            buffer: ArcSwapOption::empty(),
            position_frames: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            gain: AtomicU32::new(1.0f32.to_bits()),
            sample_rate,
            channels,
            underruns: AtomicU64::new(0),
            peak_l: AtomicU32::new(0.0f32.to_bits()),
            peak_r: AtomicU32::new(0.0f32.to_bits()),
            talk_over: AtomicBool::new(false),
            duck_gain: AtomicU32::new(1.0f32.to_bits()),
            dsp: crate::dsp::new_chain(sample_rate, channels),
            incoming_buffer: ArcSwapOption::empty(),
            incoming_position_frames: AtomicU64::new(0),
            incoming_gain: AtomicU32::new(0.0f32.to_bits()),
            crossfade_progress: AtomicU32::new((-1.0f32).to_bits()),
            mic_consumer: parking_lot::Mutex::new(None),
            mic_enabled: AtomicBool::new(false),
            mic_level: AtomicU32::new(0.0f32.to_bits()),
            mic_active: AtomicBool::new(false),
        }
    }

    fn total_frames(&self) -> u64 {
        self.buffer
            .load()
            .as_ref()
            .map(|audio| audio.frames() as u64)
            .unwrap_or(0)
    }
}

fn write_silence<T>(data: &mut [T])
where
    T: SizedSample + FromSample<f32>,
{
    for sample in data.iter_mut() {
        *sample = T::from_sample(0.0f32);
    }
}

fn write_data<T>(data: &mut [T], shared: &PlayerShared)
where
    T: SizedSample + FromSample<f32>,
{
    if !shared.playing.load(Ordering::Relaxed) {
        write_silence(data);
        return;
    }

    let guard_a = shared.buffer.load();
    let Some(buffer_a) = guard_a.as_ref() else {
        write_silence(data);
        shared.underruns.fetch_add(1, Ordering::Relaxed);
        return;
    };

    let channels = shared.channels.max(1) as usize;
    let total_frames_a = buffer_a.frames() as u64;
    let start_frame_a = shared.position_frames.load(Ordering::Relaxed);
    let gain_a = f32::from_bits(shared.gain.load(Ordering::Relaxed));
    let complete_frames = data.len() / channels;

    let cf_progress = f32::from_bits(shared.crossfade_progress.load(Ordering::Relaxed));
    let crossfading = (0.0..1.0).contains(&cf_progress);
    let guard_b = shared.incoming_buffer.load();
    let buffer_b = if crossfading { guard_b.as_ref() } else { None };

    let talk_over = shared.talk_over.load(Ordering::Relaxed);
    let target_duck = if talk_over {
        domain::crossfade::db_to_linear(-14.0) as f32
    } else {
        1.0
    };
    let current_duck = f32::from_bits(shared.duck_gain.load(Ordering::Relaxed));
    let buffer_ms = if shared.sample_rate == 0 {
        10.0
    } else {
        (complete_frames as f32 / shared.sample_rate as f32) * 1000.0
    };
    let tau = if target_duck < current_duck {
        20.0
    } else {
        300.0
    };
    let alpha = (buffer_ms / tau).clamp(0.0, 1.0);
    let duck = current_duck + (target_duck - current_duck) * alpha;
    shared.duck_gain.store(duck.to_bits(), Ordering::Relaxed);

    let incoming_gain_base = f32::from_bits(shared.incoming_gain.load(Ordering::Relaxed));
    let (out_gain, in_gain) = if crossfading {
        domain::crossfade::equal_power_gains(cf_progress as f64)
    } else {
        (1.0, 0.0)
    };

    let mut f32_data = vec![0.0f32; data.len()];
    let mut peak_l = 0.0_f32;
    let mut peak_r = 0.0_f32;

    let position_b = shared.incoming_position_frames.load(Ordering::Relaxed);

    for frame in 0..complete_frames {
        let source_frame_a = start_frame_a + frame as u64;
        if source_frame_a >= total_frames_a {
            let remainder = &mut data[frame * channels..];
            write_silence(remainder);
            shared.finished.store(true, Ordering::Relaxed);
            shared.playing.store(false, Ordering::Relaxed);
            shared
                .position_frames
                .store(total_frames_a, Ordering::Relaxed);
            shared.peak_l.fetch_max(peak_l.to_bits(), Ordering::Relaxed);
            shared.peak_r.fetch_max(peak_r.to_bits(), Ordering::Relaxed);
            return;
        }

        for channel in 0..channels {
            let idx = source_frame_a as usize * channels + channel;
            let mut value =
                buffer_a.samples.get(idx).copied().unwrap_or(0.0) * gain_a * duck * out_gain as f32;
            if let Some(buf_b) = buffer_b {
                let total_b = buf_b.frames() as u64;
                let sf_b = position_b + frame as u64;
                if sf_b < total_b {
                    let idx_b = sf_b as usize * channels + channel;
                    let v_b = buf_b.samples.get(idx_b).copied().unwrap_or(0.0)
                        * incoming_gain_base
                        * duck
                        * in_gain as f32;
                    value += v_b;
                }
            }
            f32_data[frame * channels + channel] = value;
        }
    }

    let remainder_start = complete_frames * channels;
    if remainder_start < data.len() {
        write_silence(&mut data[remainder_start..]);
    }

    // Mix microphone input (if enabled) before DSP.
    if shared.mic_enabled.load(Ordering::Relaxed) {
        let mut consumer_guard = shared.mic_consumer.lock();
        if let Some(consumer) = consumer_guard.as_mut() {
            let mut mic_peak = 0.0_f32;
            for sample in f32_data.iter_mut() {
                if let Some(mic_sample) = consumer.try_pop() {
                    *sample += mic_sample * 0.8;
                    let abs = mic_sample.abs();
                    if abs > mic_peak {
                        mic_peak = abs;
                    }
                }
            }
            shared
                .mic_level
                .fetch_max(mic_peak.to_bits(), Ordering::Relaxed);
        }
    }

    {
        let mut chain = shared.dsp.lock();
        chain.process(&mut f32_data);
    }

    peak_l = 0.0;
    peak_r = 0.0;
    for (i, sample) in f32_data.iter().enumerate() {
        let ch = i % channels;
        let abs = sample.abs();
        if ch == 0 && abs > peak_l {
            peak_l = abs;
        }
        if ch == 1 && abs > peak_r {
            peak_r = abs;
        }
        if channels == 1 && abs > peak_r {
            peak_r = abs;
        }
    }
    for (d, s) in data.iter_mut().zip(f32_data.iter()) {
        *d = T::from_sample(*s);
    }

    shared
        .position_frames
        .store(start_frame_a + complete_frames as u64, Ordering::Relaxed);
    if crossfading {
        shared
            .incoming_position_frames
            .store(position_b + complete_frames as u64, Ordering::Relaxed);
    }
    shared.peak_l.fetch_max(peak_l.to_bits(), Ordering::Relaxed);
    shared.peak_r.fetch_max(peak_r.to_bits(), Ordering::Relaxed);
}

/// CPAL-backed playback device.
///
/// `cpal::Stream` is `!Send` on Windows (and on some other platforms), so the
/// stream is created and kept alive inside a dedicated thread. The control
/// plane talks to the audio callback only through atomics in `PlayerShared`,
/// and the thread is stopped by dropping the keepalive sender.
pub struct CpalPlayer {
    shared: Arc<PlayerShared>,
    device_name: String,
    shutdown: Mutex<Option<crossbeam_channel::Sender<()>>>,
    audio_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    mic_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl CpalPlayer {
    pub fn new() -> Result<Self, AudioError> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (shutdown_tx, shutdown_rx) = crossbeam_channel::unbounded::<()>();

        let audio_thread = std::thread::Builder::new()
            .name("audiomaster-cpal".into())
            .spawn(move || run_output_thread(ready_tx, shutdown_rx))
            .map_err(|error| AudioError::BuildStream(format!("spawn audio thread: {error}")))?;

        let (shared, device_name) = match ready_rx.recv() {
            Ok(Ok((shared, name))) => (shared, name),
            Ok(Err(error)) => {
                let _ = shutdown_tx.send(());
                let _ = audio_thread.join();
                return Err(AudioError::BuildStream(error));
            }
            Err(error) => {
                let _ = audio_thread.join();
                return Err(AudioError::BuildStream(format!(
                    "audio thread stopped before reporting readiness: {error}"
                )));
            }
        };

        Ok(Self {
            shared,
            device_name,
            shutdown: Mutex::new(Some(shutdown_tx)),
            audio_thread: Mutex::new(Some(audio_thread)),
            mic_thread: Mutex::new(None),
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn shared(&self) -> Arc<PlayerShared> {
        self.shared.clone()
    }
}

impl Drop for CpalPlayer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.lock().take() {
            let _ = shutdown.send(());
        }
        if let Some(handle) = self.audio_thread.lock().take() {
            let _ = handle.join();
        }
    }
}

/// Runs on the dedicated audio thread: builds the CPAL stream, reports the
/// negotiated format back to `new()`, then blocks until shutdown.
fn run_output_thread(
    ready: std::sync::mpsc::Sender<Result<(Arc<PlayerShared>, String), String>>,
    shutdown: crossbeam_channel::Receiver<()>,
) {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        let _ = ready.send(Err(AudioError::NoOutputDevice.to_string()));
        return;
    };

    let supported = match device.default_output_config() {
        Ok(supported) => supported,
        Err(error) => {
            let _ = ready.send(Err(AudioError::Config(error.to_string()).to_string()));
            return;
        }
    };

    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.config();
    let shared = Arc::new(PlayerShared::new(config.sample_rate.0, config.channels));

    let stream = match build_stream(&device, &config, sample_format, shared.clone()) {
        Ok(stream) => stream,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };

    if let Err(error) = stream.play() {
        let _ = ready.send(Err(AudioError::BuildStream(error.to_string()).to_string()));
        return;
    }

    let device_name = device.name().unwrap_or_else(|_| "Default output".into());
    if ready.send(Ok((shared, device_name))).is_err() {
        return;
    }

    // Keep the stream alive until the player is dropped. A disconnected
    // channel also ends the loop, which covers a panic in the control plane.
    let _ = shutdown.recv();
    drop(stream);
}

fn build_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    shared: Arc<PlayerShared>,
) -> Result<cpal::Stream, AudioError> {
    let error_callback = |error| tracing::error!(%error, "audio output stream error");

    macro_rules! build {
        ($sample:ty) => {
            device.build_output_stream(
                config,
                move |data: &mut [$sample], _info: &cpal::OutputCallbackInfo| {
                    write_data(data, &shared);
                },
                error_callback,
                None,
            )
        };
    }

    let stream = match sample_format {
        SampleFormat::F32 => build!(f32),
        SampleFormat::I16 => build!(i16),
        SampleFormat::U16 => build!(u16),
        SampleFormat::I32 => build!(i32),
        other => return Err(AudioError::UnsupportedFormat(other)),
    };

    stream.map_err(|error| AudioError::BuildStream(error.to_string()))
}

impl AudioOutput for CpalPlayer {
    fn load(&self, audio: DecodedAudio) -> AppResult<()> {
        if audio.sample_rate != self.shared.sample_rate || audio.channels != self.shared.channels {
            return Err(AudioError::FormatMismatch {
                expected_rate: self.shared.sample_rate,
                expected_channels: self.shared.channels,
                actual_rate: audio.sample_rate,
                actual_channels: audio.channels,
            }
            .into());
        }
        self.shared.buffer.store(Some(Arc::new(audio)));
        self.shared.position_frames.store(0, Ordering::SeqCst);
        self.shared.finished.store(false, Ordering::SeqCst);
        self.shared.playing.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn play(&self) -> AppResult<()> {
        if self.shared.total_frames() == 0 {
            return Err(AppError::Audio("no audio buffer loaded".into()));
        }
        if self.shared.position_frames.load(Ordering::SeqCst) >= self.shared.total_frames() {
            self.shared.position_frames.store(0, Ordering::SeqCst);
        }
        self.shared.finished.store(false, Ordering::SeqCst);
        self.shared.playing.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn pause(&self) -> AppResult<()> {
        self.shared.playing.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) -> AppResult<()> {
        self.shared.playing.store(false, Ordering::SeqCst);
        self.shared.position_frames.store(0, Ordering::SeqCst);
        self.shared.finished.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn seek_ms(&self, position_ms: u64) -> AppResult<()> {
        let frame = position_ms.saturating_mul(u64::from(self.shared.sample_rate)) / 1000;
        let clamped = frame.min(self.shared.total_frames());
        self.shared.position_frames.store(clamped, Ordering::SeqCst);
        self.shared.finished.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn position_ms(&self) -> u64 {
        let frames = self.shared.position_frames.load(Ordering::Relaxed);
        frames.saturating_mul(1000) / u64::from(self.shared.sample_rate.max(1))
    }

    fn is_finished(&self) -> bool {
        self.shared.finished.load(Ordering::Relaxed)
    }

    fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }

    fn set_talk_over(&self, enabled: bool) -> AppResult<()> {
        self.shared.talk_over.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    fn start_mic(&self, device: Option<&str>) -> AppResult<()> {
        self.stop_mic();
        let host = cpal::default_host();
        let device = if let Some(name) = device {
            host.input_devices()
                .map_err(|e| AudioError::Config(e.to_string()))?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .ok_or(AudioError::NoOutputDevice)?
        } else {
            host.default_input_device()
                .ok_or(AudioError::NoOutputDevice)?
        };
        let config = device
            .default_input_config()
            .map_err(|e| AudioError::Config(e.to_string()))?;
        let channels = config.channels();
        let shared = self.shared.clone();
        shared.mic_enabled.store(true, Ordering::SeqCst);
        shared.mic_active.store(true, Ordering::SeqCst);

        let (mut producer, consumer) = HeapRb::<f32>::new(48_000 * 2).split();
        *shared.mic_consumer.lock() = Some(consumer);

        let thread = std::thread::Builder::new()
            .name("audiomaster-mic".into())
            .spawn(move || {
                let config: StreamConfig = config.into();
                let err_fn = |e| tracing::error!(%e, "mic input stream error");
                let stream = device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let ch = channels.max(1) as usize;
                        for frame in data.chunks(ch) {
                            let mono: f32 = frame.iter().sum::<f32>() / ch as f32;
                            let _ = producer.try_push(mono);
                        }
                    },
                    err_fn,
                    None,
                );
                if let Ok(stream) = stream {
                    let _ = stream.play();
                    while shared.mic_active.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    // stream dropped here
                }
            })
            .map_err(|e| AudioError::BuildStream(e.to_string()))?;
        *self.mic_thread.lock() = Some(thread);
        Ok(())
    }

    fn stop_mic(&self) {
        self.shared.mic_enabled.store(false, Ordering::SeqCst);
        self.shared.mic_active.store(false, Ordering::SeqCst);
        if let Some(thread) = self.mic_thread.lock().take() {
            let _ = thread.join();
        }
        *self.shared.mic_consumer.lock() = None;
        self.shared
            .mic_level
            .store(0.0f32.to_bits(), Ordering::SeqCst);
    }

    fn mic_level(&self) -> f32 {
        f32::from_bits(
            self.shared
                .mic_level
                .swap(0.0f32.to_bits(), Ordering::Relaxed),
        )
    }

    fn set_dsp_params(&self, params: crate::dsp::DspParams) -> AppResult<()> {
        self.shared.dsp.lock().set_params(params);
        Ok(())
    }

    fn dsp_params(&self) -> crate::dsp::DspParams {
        self.shared.dsp.lock().params()
    }

    fn load_incoming(&self, audio: DecodedAudio) -> AppResult<()> {
        if audio.sample_rate != self.shared.sample_rate || audio.channels != self.shared.channels {
            return Err(AudioError::FormatMismatch {
                expected_rate: self.shared.sample_rate,
                expected_channels: self.shared.channels,
                actual_rate: audio.sample_rate,
                actual_channels: audio.channels,
            }
            .into());
        }
        self.shared.incoming_buffer.store(Some(Arc::new(audio)));
        self.shared
            .incoming_position_frames
            .store(0, Ordering::SeqCst);
        Ok(())
    }

    fn set_crossfade_progress(&self, progress: Option<f64>) {
        let value = progress.map(|p| p as f32).unwrap_or(-1.0);
        self.shared
            .crossfade_progress
            .store(value.to_bits(), Ordering::SeqCst);
    }

    fn clear_incoming(&self) {
        self.shared.incoming_buffer.store(None);
    }

    fn peaks(&self) -> (f32, f32) {
        let l = f32::from_bits(self.shared.peak_l.swap(0.0f32.to_bits(), Ordering::Relaxed));
        let r = f32::from_bits(self.shared.peak_r.swap(0.0f32.to_bits(), Ordering::Relaxed));
        (l, r)
    }

    fn set_gain_db(&self, gain_db: f64) -> AppResult<()> {
        let gain = domain::crossfade::db_to_linear(gain_db) as f32;
        self.shared.gain.store(gain.to_bits(), Ordering::SeqCst);
        Ok(())
    }

    fn sample_rate(&self) -> u32 {
        self.shared.sample_rate
    }

    fn channels(&self) -> u16 {
        self.shared.channels
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }
}

/// Hardware-free sink for CI, unit tests and headless builds.
pub struct NullOutput {
    state: Mutex<NullState>,
    sample_rate: u32,
    channels: u16,
    underruns: AtomicU64,
    talk_over: AtomicBool,
}

struct NullState {
    audio: Option<DecodedAudio>,
    position_frames: u64,
    playing: bool,
    finished: bool,
    gain: f32,
}

impl Default for NullOutput {
    fn default() -> Self {
        Self {
            state: Mutex::new(NullState {
                audio: None,
                position_frames: 0,
                playing: false,
                finished: false,
                gain: 1.0,
            }),
            sample_rate: 48_000,
            channels: 2,
            underruns: AtomicU64::new(0),
            talk_over: AtomicBool::new(false),
        }
    }
}

impl NullOutput {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            underruns: AtomicU64::new(0),
            talk_over: AtomicBool::new(false),
            ..Default::default()
        }
    }

    /// Advances the virtual playhead for deterministic tests.
    pub fn advance_ms(&self, ms: u64) {
        let mut state = self.state.lock();
        let Some(total_frames) = state.audio.as_ref().map(|audio| audio.frames() as u64) else {
            return;
        };
        let delta = ms.saturating_mul(u64::from(self.sample_rate)) / 1000;
        state.position_frames = (state.position_frames + delta).min(total_frames);
        if state.position_frames >= total_frames {
            state.finished = true;
            state.playing = false;
        }
    }
}

impl CpalPlayer {
    pub fn list_input_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }
}

impl AudioOutput for NullOutput {
    fn load(&self, audio: DecodedAudio) -> AppResult<()> {
        if audio.sample_rate != self.sample_rate || audio.channels != self.channels {
            return Err(AudioError::FormatMismatch {
                expected_rate: self.sample_rate,
                expected_channels: self.channels,
                actual_rate: audio.sample_rate,
                actual_channels: audio.channels,
            }
            .into());
        }
        let mut state = self.state.lock();
        state.audio = Some(audio);
        state.position_frames = 0;
        state.playing = false;
        state.finished = false;
        Ok(())
    }

    fn play(&self) -> AppResult<()> {
        let mut state = self.state.lock();
        if state.audio.is_none() {
            return Err(AppError::Audio("no audio buffer loaded".into()));
        }
        state.playing = true;
        state.finished = false;
        Ok(())
    }

    fn pause(&self) -> AppResult<()> {
        self.state.lock().playing = false;
        Ok(())
    }

    fn stop(&self) -> AppResult<()> {
        let mut state = self.state.lock();
        state.playing = false;
        state.finished = false;
        state.position_frames = 0;
        Ok(())
    }

    fn seek_ms(&self, position_ms: u64) -> AppResult<()> {
        let mut state = self.state.lock();
        let total = state
            .audio
            .as_ref()
            .map(|audio| audio.frames() as u64)
            .unwrap_or(0);
        let frame = position_ms.saturating_mul(u64::from(self.sample_rate)) / 1000;
        state.position_frames = frame.min(total);
        state.finished = false;
        Ok(())
    }

    fn position_ms(&self) -> u64 {
        let state = self.state.lock();
        state.position_frames.saturating_mul(1000) / u64::from(self.sample_rate.max(1))
    }

    fn is_finished(&self) -> bool {
        self.state.lock().finished
    }

    fn is_playing(&self) -> bool {
        self.state.lock().playing
    }

    fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    fn set_talk_over(&self, enabled: bool) -> AppResult<()> {
        self.talk_over.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    fn peaks(&self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn set_gain_db(&self, gain_db: f64) -> AppResult<()> {
        self.state.lock().gain = domain::crossfade::db_to_linear(gain_db) as f32;
        Ok(())
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn device_name(&self) -> &str {
        "Null output"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, channels: u16, frames: usize) -> DecodedAudio {
        let mut samples = Vec::with_capacity(frames * channels as usize);
        for frame in 0..frames {
            let value = (frame as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.1;
            for _ in 0..channels {
                samples.push(value);
            }
        }
        DecodedAudio {
            samples,
            sample_rate: rate,
            channels,
        }
    }

    #[test]
    fn null_output_tracks_position_and_finish() {
        let output = NullOutput::new(48_000, 2);
        output.load(sine(48_000, 2, 48_000)).unwrap();
        output.play().unwrap();
        output.advance_ms(500);
        assert_eq!(output.position_ms(), 500);
        assert!(output.is_playing());
        output.advance_ms(600);
        assert!(output.is_finished());
        assert!(!output.is_playing());
    }

    #[test]
    fn seek_is_clamped() {
        let output = NullOutput::new(48_000, 2);
        output.load(sine(48_000, 2, 48_000)).unwrap();
        output.seek_ms(10_000).unwrap();
        assert_eq!(output.position_ms(), 1_000);
    }

    #[test]
    fn format_mismatch_is_rejected() {
        let output = NullOutput::new(48_000, 2);
        let error = output.load(sine(44_100, 2, 100)).unwrap_err();
        assert!(matches!(error, AppError::Audio(_)));
    }
}
