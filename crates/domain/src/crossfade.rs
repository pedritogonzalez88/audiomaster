//! Crossfade math shared by the playout engine and the tests.
//!
//! Equal-power crossfade keeps the perceived energy constant while two tracks
//! overlap. A linear crossfade loses ~3 dB at the midpoint, which is audible
//! on broadcast.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GainCurve {
    EqualPower,
    Linear,
}

/// Returns `(outgoing_gain, incoming_gain)` for a normalized progress `t`.
///
/// `t = 0` means the outgoing track is at full level; `t = 1` means the
/// incoming track is at full level.
pub fn equal_power_gains(t: f64) -> (f64, f64) {
    let t = t.clamp(0.0, 1.0);
    let angle = t * std::f64::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

pub fn linear_gains(t: f64) -> (f64, f64) {
    let t = t.clamp(0.0, 1.0);
    (1.0 - t, t)
}

pub fn gains_for(curve: GainCurve, t: f64) -> (f64, f64) {
    match curve {
        GainCurve::EqualPower => equal_power_gains(t),
        GainCurve::Linear => linear_gains(t),
    }
}

/// Converts decibels to a linear amplitude factor.
pub fn db_to_linear(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// Converts a linear amplitude factor to decibels, with a floor to avoid -inf.
pub fn linear_to_db(linear: f64) -> f64 {
    20.0 * linear.max(1e-9).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn equal_power_keeps_energy_constant() {
        for step in 0..=100 {
            let t = step as f64 / 100.0;
            let (out, inc) = equal_power_gains(t);
            assert_close(out * out + inc * inc, 1.0, 1e-9);
        }
    }

    #[test]
    fn equal_power_endpoints() {
        let (out, inc) = equal_power_gains(0.0);
        assert_close(out, 1.0, 1e-9);
        assert_close(inc, 0.0, 1e-9);

        let (out, inc) = equal_power_gains(1.0);
        assert_close(out, 0.0, 1e-9);
        assert_close(inc, 1.0, 1e-9);
    }

    #[test]
    fn linear_midpoint_loses_energy() {
        let (out, inc) = linear_gains(0.5);
        assert_close(out, 0.5, 1e-9);
        assert_close(inc, 0.5, 1e-9);
        assert!(out * out + inc * inc < 0.6);
    }

    #[test]
    fn clamps_out_of_range_progress() {
        assert_eq!(equal_power_gains(-1.0), equal_power_gains(0.0));
        assert_eq!(equal_power_gains(2.0), equal_power_gains(1.0));
    }

    #[test]
    fn db_conversions_roundtrip() {
        for db in [-60.0, -14.0, -6.0, 0.0, 6.0] {
            assert_close(linear_to_db(db_to_linear(db)), db, 1e-9);
        }
    }
}
