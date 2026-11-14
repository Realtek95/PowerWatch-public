//! Source-independent schedule models, clocks and local-day projections.
pub mod cache;
pub mod clock;
pub mod dashboard;
pub mod grid;
pub mod history;
pub mod model;
pub mod monitoring;
pub mod session;
pub mod sourcing;
pub mod time;
use thiserror::Error;
/// Invalid source data is rejected before it can replace a trusted snapshot.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid manual address")]
    InvalidAddress,
    #[error("invalid monitoring window or address pause")]
    InvalidMonitoring,
    #[error("interval must end after it starts")]
    InvalidInterval,
    #[error("schedule intervals overlap")]
    OverlappingIntervals,
    #[error("date or time cannot be represented unambiguously")]
    InvalidTime,
    #[error("the requested date has no schedule")]
    MissingDay,
}
