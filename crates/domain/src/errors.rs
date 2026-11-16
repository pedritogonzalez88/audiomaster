use thiserror::Error;

/// Errors that represent a broken domain invariant.
///
/// Infrastructure failures (I/O, database, audio device) are intentionally
/// absent: those belong to the application/infrastructure layers.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("cue points are inconsistent: {0}")]
    InvalidCuePoints(String),
    #[error("duration must be greater than zero")]
    InvalidDuration,
    #[error("a playlist item of type {0} requires additional data")]
    IncompletePlaylistItem(String),
    #[error("transition duration must be between 0 and {max_ms} ms, got {actual_ms} ms")]
    InvalidTransitionDuration { max_ms: u32, actual_ms: u32 },
    #[error("asset is not playable: {0}")]
    NotPlayable(String),
}
