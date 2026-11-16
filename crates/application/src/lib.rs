//! Application layer: use cases and the ports they depend on.
//!
//! This crate knows the domain and defines traits (ports) that infrastructure
//! adapters implement. It must never import Tauri, SQLite, CPAL or libmpv.

pub mod errors;
pub mod library;
pub mod ports;
pub mod scheduler;

pub use errors::{AppError, AppResult};
pub use library::{Category, ImportSummary, LibraryQuery, LibraryService, ProbedMedia};
pub use ports::{
    AuditRepository, Clock, MediaProbe, MediaRepository, PlayLogStart, PlayoutPort,
    PlayoutSnapshot, TransportCommand,
};
pub use scheduler::SchedulerRuntime;
