//! Replaceable source boundary, local territory catalog and bounded live reads.
pub mod catalog;
pub mod http;
pub mod live;
pub mod registry;
mod text;
use chrono::{DateTime, Days, NaiveDate, Utc};
use powerwatch_core::{
    dashboard::{Dashboard, build_dashboard},
    model::{Address, CatalogEntry, ScheduleSnapshot},
    time::LOCAL_ZONE,
};
use std::{future::Future, pin::Pin};
use thiserror::Error;
pub type ProviderResult<T> = Result<T, ProviderError>;
/// Shared executor boundary; fixtures implement it only in private test support.
pub trait SnapshotSource: Send + Sync {
    fn snapshot_for(
        &self,
        address: &Address,
        now: DateTime<Utc>,
        previous: Option<&ScheduleSnapshot>,
    ) -> ProviderResult<ScheduleSnapshot>;
    fn refresh_for(
        &self,
        address: &Address,
        now: DateTime<Utc>,
        previous: Option<&ScheduleSnapshot>,
    ) -> ProviderResult<ScheduleSnapshot> {
        self.snapshot_for(address, now, previous)
    }
}
pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = ProviderResult<T>> + Send + 'a>>;
/// Technical failures are mapped to Ukrainian messages at the presentation edge.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("address has no confirmed or explicitly selected queue")]
    UnresolvedAddress,
    #[error("live source: {0}")]
    Live(#[from] http::FetchError),
    #[error("address is not in this provider's catalog")]
    AddressNotFound,
    #[error("source is temporarily unavailable")]
    Unavailable,
    #[error("source response timed out")]
    Timeout,
    #[error("source returned an empty response")]
    EmptyResponse,
    #[error("address has no single schedule due to multiple supply connections")]
    ComplexConnection,
    #[error("invalid data: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("invalid schedule: {0}")]
    InvalidSchedule(#[from] powerwatch_core::CoreError),
}
impl ProviderError {
    /// Stable presentation code shared by IPC and offline preview export.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnresolvedAddress => "unresolved_address",
            Self::Live(error) => error.code(),
            Self::AddressNotFound => "address_not_found",
            Self::Unavailable => "source_unavailable",
            Self::Timeout => "source_timeout",
            Self::EmptyResponse => "empty_response",
            Self::ComplexConnection => "complex_connection",
            Self::InvalidJson(_) | Self::InvalidSchedule(_) => "invalid_data",
        }
    }
}
/// Async, object-safe catalog and schedule contract for both local and future
/// remote sources. Callers supply receipt time, so tests need no wall clock.
pub trait ScheduleProvider: Send + Sync {
    fn id(&self) -> &str;
    fn settlements(&self) -> ProviderFuture<'_, Vec<CatalogEntry>>;
    fn streets<'a>(&'a self, settlement_id: &'a str) -> ProviderFuture<'a, Vec<CatalogEntry>>;
    fn houses<'a>(&'a self, street_id: &'a str) -> ProviderFuture<'a, Vec<Address>>;
    fn fetch<'a>(
        &'a self,
        address_id: &'a str,
        dates: &'a [NaiveDate],
        now: DateTime<Utc>,
    ) -> ProviderFuture<'a, ScheduleSnapshot>;
}
/// Application use case accepts the contract, so changing a provider does not
/// change the Tauri commands, SQLite models or presentation calculations.
pub async fn load_dashboard(
    provider: &dyn ScheduleProvider,
    address_id: &str,
    day_offset: u64,
    now: DateTime<Utc>,
) -> ProviderResult<Dashboard> {
    if day_offset > 1 {
        return Err(powerwatch_core::CoreError::MissingDay.into());
    }
    let today = now.with_timezone(&LOCAL_ZONE).date_naive();
    let tomorrow = today
        .checked_add_days(Days::new(1))
        .ok_or(powerwatch_core::CoreError::InvalidTime)?;
    let snapshot = provider.fetch(address_id, &[today, tomorrow], now).await?;
    Ok(build_dashboard(
        snapshot,
        if day_offset == 0 { today } else { tomorrow },
        now,
    )?)
}
