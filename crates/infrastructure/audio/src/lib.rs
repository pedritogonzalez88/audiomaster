//! Audio infrastructure: metadata probing, decoding and CPAL playback.
//!
//! Phase 1 decodes a track into memory before playing it. This is simple and
//! deterministic, but a 5-minute stereo track uses ~110 MB as f32. Phase 2
//! replaces `CpalPlayer::load` with a streaming decoder feeding a lock-free
//! ring buffer; the public API is designed to make that swap local.

pub mod decoder;
pub mod dsp;
pub mod player;
pub mod probe;

pub use decoder::{decode_to, DecodedAudio};
pub use dsp::{new_chain, DspParams};
pub use player::{AudioError, AudioOutput, CpalPlayer, NullOutput};
pub use probe::SymphoniaProbe;
