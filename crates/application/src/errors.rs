use domain::DomainError;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("repository error: {0}")]
    Repository(String),
    #[error("media probe error: {0}")]
    Probe(String),
    #[error("audio error: {0}")]
    Audio(String),
    #[error("playout error: {0}")]
    Playout(String),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("internal error: {0}")]
    Internal(String),
}
