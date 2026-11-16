//! Lightweight DSP chain applied in the audio callback: 10-band graphic EQ,
//! compressor and a brickwall limiter. Parameters live behind a mutex so the
//! control plane can change them without touching the audio thread's path.

use parking_lot::Mutex;

const EQ_BANDS: usize = 10;
const EQ_FREQS: [f32; EQ_BANDS] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DspParams {
    /// Per-band gain in dB (0 = flat).
    pub eq_gains_db: [f64; EQ_BANDS],
    /// Compressor threshold in dBFS.
    pub comp_threshold_db: f64,
    /// Compressor ratio (e.g. 4.0 = 4:1).
    pub comp_ratio: f64,
    /// Compressor attack in ms.
    pub comp_attack_ms: f64,
    /// Compressor release in ms.
    pub comp_release_ms: f64,
    /// Limiter ceiling in dBFS.
    pub limiter_ceiling_db: f64,
    /// Master bypass.
    pub enabled: bool,
}

impl Default for DspParams {
    fn default() -> Self {
        Self {
            eq_gains_db: [0.0; EQ_BANDS],
            comp_threshold_db: -18.0,
            comp_ratio: 4.0,
            comp_attack_ms: 10.0,
            comp_release_ms: 100.0,
            limiter_ceiling_db: -1.0,
            enabled: true,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct BiquadState {
    z1: f32,
    z2: f32,
}

#[derive(Clone, Copy, Default)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

pub struct DspChain {
    params: DspParams,
    sample_rate: u32,
    channels: usize,
    eq_state: Vec<Vec<BiquadState>>,
    eq_coeffs: Vec<Vec<BiquadCoeffs>>,
    coeffs_dirty: bool,
    comp_env: Vec<f32>,
}

impl DspChain {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let channels = channels.max(1) as usize;
        Self {
            params: DspParams::default(),
            sample_rate,
            channels,
            eq_state: vec![vec![BiquadState::default(); EQ_BANDS]; channels],
            eq_coeffs: vec![vec![BiquadCoeffs::default(); EQ_BANDS]; channels],
            coeffs_dirty: true,
            comp_env: vec![0.0; channels],
        }
    }

    pub fn params(&self) -> DspParams {
        self.params.clone()
    }

    pub fn set_params(&mut self, params: DspParams) {
        self.params = params;
        self.coeffs_dirty = true;
    }

    fn recompute_coeffs(&mut self) {
        if !self.coeffs_dirty {
            return;
        }
        let fs = self.sample_rate as f32;
        for band in 0..EQ_BANDS {
            let f0 = EQ_FREQS[band].min(fs * 0.49);
            let gain_db = self.params.eq_gains_db[band] as f32;
            let q = 1.4;
            let w0 = 2.0 * std::f32::consts::PI * f0 / fs;
            let alpha = w0.sin() / (2.0 * q);
            let a = 10f32.powf(gain_db / 40.0);
            let (b0, b1, b2, a0, a1, a2) = if gain_db.abs() < 1e-3 {
                (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            } else {
                (
                    1.0 + alpha * a,
                    -2.0 * w0.cos(),
                    1.0 - alpha * a,
                    1.0 + alpha / a,
                    -2.0 * w0.cos(),
                    1.0 - alpha / a,
                )
            };
            let inv_a0 = 1.0 / a0;
            let coeffs = BiquadCoeffs {
                b0: b0 * inv_a0,
                b1: b1 * inv_a0,
                b2: b2 * inv_a0,
                a1: a1 * inv_a0,
                a2: a2 * inv_a0,
            };
            for ch in 0..self.channels {
                self.eq_coeffs[ch][band] = coeffs;
            }
        }
        self.coeffs_dirty = false;
    }

    /// Processes an interleaved buffer in place.
    pub fn process(&mut self, data: &mut [f32]) {
        if !self.params.enabled {
            return;
        }
        let channels = self.channels.max(1);
        let frames = data.len() / channels;
        let limiter = 10f32
            .powf(self.params.limiter_ceiling_db as f32 / 20.0)
            .abs();
        let threshold = self.params.comp_threshold_db as f32;
        let ratio = self.params.comp_ratio as f32;
        let attack =
            (self.params.comp_attack_ms as f32 / 1000.0 * self.sample_rate as f32).max(1.0);
        let release =
            (self.params.comp_release_ms as f32 / 1000.0 * self.sample_rate as f32).max(1.0);
        let attack_coef = (-1.0 / attack).exp();
        let release_coef = (-1.0 / release).exp();

        self.recompute_coeffs();

        for frame in 0..frames {
            for ch in 0..channels {
                let idx = frame * channels + ch;
                let mut x = data[idx];

                // EQ cascade.
                for band in 0..EQ_BANDS {
                    let coeffs = self.eq_coeffs[ch][band];
                    let state = &mut self.eq_state[ch][band];
                    let y = coeffs.b0 * x + state.z1;
                    let new_z1 = coeffs.b1 * x - coeffs.a1 * y + state.z2;
                    let new_z2 = coeffs.b2 * x - coeffs.a2 * y;
                    state.z1 = new_z1;
                    state.z2 = new_z2;
                    x = y;
                }

                // Compressor.
                let level_db = 20.0 * x.abs().max(1e-9).log10();
                let env = &mut self.comp_env[ch];
                let coef = if level_db > *env {
                    attack_coef
                } else {
                    release_coef
                };
                *env = coef * *env + (1.0 - coef) * level_db;
                let gain_db = if *env > threshold {
                    (threshold - *env) * (1.0 - 1.0 / ratio)
                } else {
                    0.0
                };
                let comp_gain = 10f32.powf(gain_db / 20.0);
                x *= comp_gain;

                // Limiter.
                if x > limiter {
                    x = limiter;
                } else if x < -limiter {
                    x = -limiter;
                }

                data[idx] = x;
            }
        }
    }
}

pub type DspMutex = Mutex<DspChain>;

pub fn new_chain(sample_rate: u32, channels: u16) -> DspMutex {
    Mutex::new(DspChain::new(sample_rate, channels))
}
