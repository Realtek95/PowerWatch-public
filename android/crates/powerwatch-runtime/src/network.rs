//! HTTP policy adapter opens a short independent SQLite connection. Never
//! retain the UI repository lock across a request or a platform callback.
use chrono::{DateTime, Utc};
use powerwatch_providers::{
    http::{FetchError, PauseRepository},
    live::service::LiveService,
};
use powerwatch_storage::Store;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
struct SqliteNetwork(PathBuf);
impl PauseRepository for SqliteNetwork {
    fn until(&self, host: &str) -> Result<Option<DateTime<Utc>>, FetchError> {
        Store::open(&self.0)
            .and_then(|s| s.http_pause(host))
            .map_err(|_| FetchError::Persistence)
    }
    fn postpone(&self, host: &str, until: DateTime<Utc>) -> Result<(), FetchError> {
        Store::open(&self.0)
            .and_then(|s| s.postpone_http(host, until))
            .map_err(|_| FetchError::Persistence)
    }
    fn claim(
        &self,
        host: &str,
        owner: &str,
        now: DateTime<Utc>,
        expires: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, FetchError> {
        Store::open(&self.0)
            .and_then(|mut s| s.claim_http(host, owner, now, expires))
            .map_err(|_| FetchError::Persistence)
    }
    fn finish(&self, host: &str, owner: &str, next: DateTime<Utc>) -> Result<(), FetchError> {
        if Store::open(&self.0)
            .and_then(|mut s| s.finish_http(host, owner, next))
            .map_err(|_| FetchError::Persistence)?
        {
            Ok(())
        } else {
            Err(FetchError::Persistence)
        }
    }
}
impl powerwatch_providers::live::service::SourceReporter for SqliteNetwork {
    fn record(
        &self,
        check: &powerwatch_providers::live::service::SourceCheck,
    ) -> Result<(), FetchError> {
        Store::open(&self.0)
            .and_then(|s| {
                s.set_setting(
                    &format!("source_check:{}:{}", check.operator_id, check.provider_id),
                    check,
                )
            })
            .map_err(|_| FetchError::Persistence)
    }
}
pub fn configure_http_policy(database: &Path) -> Result<(), FetchError> {
    let repository = Arc::new(SqliteNetwork(database.into()));
    let service = LiveService::shared()?;
    service.set_pause_repository(repository.clone())?;
    service.set_source_reporter(repository)
}
