//! Live queue forecasts. No adapter here claims to measure a home's power.
pub mod address;
pub mod dtek;
mod dtek_fact;
pub mod mirror;
mod retention;
pub mod service;
pub mod svitlopulse;
pub mod yasno;
use chrono::{DateTime, Utc};
use powerwatch_core::grid::{
    EvidenceStatus, Provider, ProviderKind, Queue, SourceEvidence, SourceScope,
};
pub use powerwatch_core::sourcing::SourceForecast as SourceSchedule;
fn evidence(
    id: &str,
    name: &str,
    kind: ProviderKind,
    website: &str,
    queue: &Queue,
    now: DateTime<Utc>,
    published: Option<DateTime<Utc>>,
) -> SourceEvidence {
    SourceEvidence {
        provider: Provider {
            id: id.into(),
            name: name.into(),
            kind,
            website: website.into(),
        },
        scope: SourceScope::Queue(queue.clone()),
        checked_at: now,
        published_at: published,
        valid_until: None,
        status: EvidenceStatus::Verified,
        content_hash: None,
    }
}
