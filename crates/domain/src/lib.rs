//! Pure domain model for the Audiomaster broadcast playout system.
//!
//! This crate has no dependencies on Tauri, SQLite, audio backends or the
//! operating system. Everything here must be deterministic and unit-testable.

pub mod crossfade;
pub mod entities;
pub mod errors;
pub mod scheduler;

pub use crossfade::{equal_power_gains, linear_gains, GainCurve};
pub use entities::{
    AssetKind, CuePoints, MediaAsset, Playlist, PlaylistItem, Station, Transition, TransitionKind,
};
pub use errors::DomainError;
pub use scheduler::{
    Daypart, Recurrence, ScheduleActionKind, ScheduleEvent, ScheduleEventKind, ScheduleStatus,
};
