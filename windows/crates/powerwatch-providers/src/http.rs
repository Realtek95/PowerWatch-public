//! Bounded, serialized source reads. No implicit transport retries or redirects.
use chrono::{DateTime, TimeDelta, Utc};
use reqwest::{
    StatusCode,
    blocking::{Client, RequestBuilder},
    header::{ETAG, IF_NONE_MATCH, RETRY_AFTER},
};
use serde::de::DeserializeOwned;
use std::{
    collections::HashMap,
    io::Read,
    sync::{Arc, Mutex, MutexGuard, TryLockError},
    time::{Duration, Instant},
};
use thiserror::Error;
const MAX_BODY: u64 = 2_000_000;
const CACHE_SECONDS: i64 = 60;
const MAX_CACHE_BYTES: usize = 8_000_000;
const MAX_WAIT: Duration = Duration::from_secs(15);
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("address has multiple supply connections")]
    ComplexConnection,
    #[error("cannot persist source request policy")]
    Persistence,
    #[error("source network failure")]
    Network,
    #[error("source response timeout")]
    Timeout,
    #[error("source returned HTTP {0}")]
    Http(u16),
    #[error("source requested a pause until {0}")]
    RateLimited(DateTime<Utc>),
    #[error("source returned invalid or oversized data")]
    InvalidData,
}
impl FetchError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ComplexConnection => "complex_connection",
            Self::Persistence => "storage_error",
            Self::Timeout => "source_timeout",
            Self::RateLimited(_) => "source_limited",
            Self::InvalidData => "invalid_data",
            Self::Network => "source_network",
            Self::Http(401 | 403) => "source_access_denied",
            Self::Http(400) => "source_request_rejected",
            Self::Http(_) => "source_unavailable",
        }
    }
}
#[derive(Clone)]
struct Cached {
    revalidate: bool,
    etag: Option<String>,
    body: Vec<u8>,
    checked_at: DateTime<Utc>,
}
/// Host pauses and owned request slots must survive independent native workers.
pub trait PauseRepository: Send + Sync {
    fn until(&self, host: &str) -> Result<Option<DateTime<Utc>>, FetchError>;
    fn postpone(&self, host: &str, until: DateTime<Utc>) -> Result<(), FetchError>;
    fn claim(
        &self,
        host: &str,
        owner: &str,
        now: DateTime<Utc>,
        expires: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, FetchError>;
    fn finish(&self, host: &str, owner: &str, next: DateTime<Utc>) -> Result<(), FetchError>;
}
pub(crate) struct SourceResponse {
    pub status: StatusCode,
    pub body: Vec<u8>,
    pub checked_at: DateTime<Utc>,
    etag: Option<String>,
}
#[derive(Default)]
struct HostGate {
    finished: Option<Instant>,
}
pub struct HttpClient {
    client: Client,
    cache: Mutex<HashMap<String, Cached>>,
    paused: Mutex<HashMap<String, DateTime<Utc>>>,
    pause_repository: Mutex<Option<Arc<dyn PauseRepository>>>,
    hosts: Mutex<HashMap<String, Arc<Mutex<HostGate>>>>,
    gap: Duration,
}
impl HttpClient {
    pub fn new() -> Result<Self, FetchError> {
        Self::build(true)
    }
    fn build(https_only: bool) -> Result<Self, FetchError> {
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| FetchError::Network)?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let client = Client::builder()
            .https_only(https_only)
            .cookie_store(true)
            .tls_backend_preconfigured(tls)
            .user_agent(concat!("PowerWatch/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| FetchError::Network)?;
        Ok(Self {
            client,
            cache: Default::default(),
            paused: Default::default(),
            pause_repository: Default::default(),
            hosts: Default::default(),
            gap: Duration::from_secs(2),
        })
    }
    pub(crate) fn revalidate_forecasts(&self) -> Result<(), FetchError> {
        for (url, entry) in self
            .cache
            .lock()
            .map_err(|_| FetchError::Network)?
            .iter_mut()
        {
            if url.contains("/planned-outages")
                || url.contains("/api/v1/groups/")
                || url.ends_with("/api/v1/summary")
                || url.contains("/outage-data-ua/main/data/")
            {
                entry.revalidate = true;
            }
        }
        Ok(())
    }
    pub fn set_pause_repository(
        &self,
        repository: Arc<dyn PauseRepository>,
    ) -> Result<(), FetchError> {
        *self
            .pause_repository
            .lock()
            .map_err(|_| FetchError::Persistence)? = Some(repository);
        Ok(())
    }
    pub fn json<T: DeserializeOwned>(
        &self,
        url: &str,
        now: DateTime<Utc>,
    ) -> Result<T, FetchError> {
        Ok(self.json_observed(url, now)?.0)
    }
    /// A cache hit retains the actual network receipt instead of freshening it.
    pub fn json_observed<T: DeserializeOwned>(
        &self,
        url: &str,
        now: DateTime<Utc>,
    ) -> Result<(T, DateTime<Utc>), FetchError> {
        let parsed = reqwest::Url::parse(url).map_err(|_| FetchError::InvalidData)?;
        let host = parsed.host_str().ok_or(FetchError::InvalidData)?;
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || !matches!(
                host,
                "app.yasno.ua" | "svitlopulse.andrewboyko.name" | "raw.githubusercontent.com"
            )
        {
            return Err(FetchError::InvalidData);
        }
        self.read_observed(url, host, now)
    }
    fn host(&self, host: &str) -> Result<Arc<Mutex<HostGate>>, FetchError> {
        Ok(self
            .hosts
            .lock()
            .map_err(|_| FetchError::Network)?
            .entry(host.into())
            .or_default()
            .clone())
    }
    fn lock_host(slot: &Mutex<HostGate>) -> Result<MutexGuard<'_, HostGate>, FetchError> {
        let started = Instant::now();
        loop {
            match slot.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(TryLockError::Poisoned(_)) => return Err(FetchError::Network),
                Err(TryLockError::WouldBlock) => {
                    if started.elapsed() >= MAX_WAIT {
                        return Err(FetchError::RateLimited(Utc::now() + TimeDelta::seconds(2)));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }
    }
    fn ensure_unpaused(
        &self,
        host: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<Arc<dyn PauseRepository>>, FetchError> {
        let repository = self
            .pause_repository
            .lock()
            .map_err(|_| FetchError::Persistence)?
            .clone();
        if let Some(repo) = &repository
            && let Some(until) = repo.until(host)?
            && until > now
        {
            return Err(FetchError::RateLimited(until));
        }
        if let Some(until) = self
            .paused
            .lock()
            .map_err(|_| FetchError::Network)?
            .get(host)
            && *until > now
        {
            return Err(FetchError::RateLimited(*until));
        }
        Ok(repository)
    }
    fn postpone(
        &self,
        host: &str,
        until: DateTime<Utc>,
        repository: Option<&Arc<dyn PauseRepository>>,
    ) -> Result<(), FetchError> {
        self.paused
            .lock()
            .map_err(|_| FetchError::Network)?
            .entry(host.into())
            .and_modify(|v| *v = (*v).max(until))
            .or_insert(until);
        if let Some(repo) = repository {
            repo.postpone(host, until)?;
        }
        Ok(())
    }
    pub(crate) fn invalid_response(&self, host: &str) -> Result<(), FetchError> {
        let repository = self
            .pause_repository
            .lock()
            .map_err(|_| FetchError::Persistence)?
            .clone();
        self.postpone(
            host,
            Utc::now() + TimeDelta::minutes(5),
            repository.as_ref(),
        )
    }
    fn perform<T>(
        &self,
        host: &str,
        now: DateTime<Utc>,
        gate: &mut HostGate,
        operation: impl FnOnce(DateTime<Utc>) -> Result<T, FetchError>,
    ) -> Result<T, FetchError> {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = now.max(Utc::now());
        let started = Instant::now();
        let current =
            || (base + TimeDelta::from_std(started.elapsed()).unwrap_or_default()).max(Utc::now());
        let owner = format!(
            "{}:{}:{}",
            std::process::id(),
            base.timestamp_nanos_opt().unwrap_or_default(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let repository = loop {
            let repository = self.ensure_unpaused(host, current())?;
            let cooling = gate
                .finished
                .is_some_and(|finished| finished.elapsed() < self.gap);
            if !cooling {
                if let Some(repo) = &repository {
                    if repo
                        .claim(host, &owner, current(), current() + TimeDelta::seconds(60))?
                        .is_none()
                    {
                        break repository;
                    }
                } else {
                    break repository;
                }
            }
            if started.elapsed() >= MAX_WAIT {
                return Err(FetchError::RateLimited(current() + TimeDelta::seconds(2)));
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let result = operation(current());
        let finished = current();
        let until = match &result {
            Err(FetchError::RateLimited(until)) => Some(*until),
            Err(FetchError::Http(401 | 403)) => Some(finished + TimeDelta::minutes(15)),
            Err(FetchError::InvalidData | FetchError::Http(404)) => {
                Some(finished + TimeDelta::minutes(5))
            }
            Err(FetchError::Network | FetchError::Timeout | FetchError::Http(_)) => {
                Some(finished + TimeDelta::minutes(1))
            }
            _ => None,
        };
        let persisted = until
            .map(|until| self.postpone(host, until, repository.as_ref()))
            .transpose();
        gate.finished = Some(Instant::now());
        let released = repository
            .as_ref()
            .map(|repo| {
                repo.finish(
                    host,
                    &owner,
                    finished + TimeDelta::from_std(self.gap).unwrap_or_default(),
                )
            })
            .transpose();
        persisted?;
        released?;
        result
    }
    fn send_checked(
        &self,
        request: RequestBuilder,
        now: DateTime<Utc>,
        allow_csrf_error: bool,
    ) -> Result<SourceResponse, FetchError> {
        let started = Instant::now();
        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                FetchError::Timeout
            } else {
                FetchError::Network
            }
        })?;
        let current =
            || (now + TimeDelta::from_std(started.elapsed()).unwrap_or_default()).max(Utc::now());
        if matches!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE
        ) || (!response.status().is_success() && response.headers().contains_key(RETRY_AFTER))
        {
            let received = current();
            let until = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| retry_after(v, received))
                .filter(|v| *v > received)
                .unwrap_or(received + TimeDelta::minutes(5));
            return Err(FetchError::RateLimited(until));
        }
        if !response.status().is_success()
            && response.status() != StatusCode::NOT_MODIFIED
            && !(allow_csrf_error && response.status() == StatusCode::BAD_REQUEST)
        {
            return Err(FetchError::Http(response.status().as_u16()));
        }
        if response.content_length().is_some_and(|n| n > MAX_BODY) {
            return Err(FetchError::InvalidData);
        }
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let status = response.status();
        let mut body = Vec::new();
        response
            .take(MAX_BODY + 1)
            .read_to_end(&mut body)
            .map_err(|_| FetchError::Network)?;
        if body.len() as u64 > MAX_BODY {
            return Err(FetchError::InvalidData);
        }
        Ok(SourceResponse {
            status,
            body,
            checked_at: current(),
            etag,
        })
    }
    /// These are the site's two observed same-origin routes, with our own cookie jar.
    pub(crate) fn dtek_request(
        &self,
        form: Option<&[(String, String)]>,
        csrf: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<SourceResponse, FetchError> {
        const HOST: &str = "www.dtek-dnem.com.ua";
        let slot = self.host(HOST)?;
        let mut gate = Self::lock_host(&slot)?;
        let request = if let Some(form) = form {
            let mut request = self
                .client
                .post("https://www.dtek-dnem.com.ua/ua/ajax")
                .header("Origin", "https://www.dtek-dnem.com.ua")
                .header("Referer", "https://www.dtek-dnem.com.ua/ua/shutdowns")
                .header("X-Requested-With", "XMLHttpRequest")
                .header("Accept", "application/json")
                .form(form);
            if let Some(token) = csrf {
                if token.len() > 512 || token.chars().any(char::is_control) {
                    return Err(FetchError::InvalidData);
                }
                request = request.header("X-CSRF-Token", token);
            }
            request
        } else {
            self.client
                .get("https://www.dtek-dnem.com.ua/ua/shutdowns")
                .header("Accept", "text/html")
        };
        self.perform(HOST, now, &mut gate, |now| {
            self.send_checked(request, now, form.is_some() && csrf.is_none())
        })
    }
    fn read_observed<T: DeserializeOwned>(
        &self,
        url: &str,
        host: &str,
        now: DateTime<Utc>,
    ) -> Result<(T, DateTime<Utc>), FetchError> {
        let slot = self.host(host)?;
        let mut gate = Self::lock_host(&slot)?;
        let now = now.max(Utc::now());
        self.ensure_unpaused(host, now)?;
        let cached = self
            .cache
            .lock()
            .map_err(|_| FetchError::Network)?
            .get(url)
            .cloned();
        if let Some(entry) = &cached
            && !entry.revalidate
            && now >= entry.checked_at
            && now - entry.checked_at < TimeDelta::seconds(CACHE_SECONDS)
            && now
                .with_timezone(&powerwatch_core::time::LOCAL_ZONE)
                .date_naive()
                == entry
                    .checked_at
                    .with_timezone(&powerwatch_core::time::LOCAL_ZONE)
                    .date_naive()
        {
            return Ok((
                serde_json::from_slice(&entry.body).map_err(|_| FetchError::InvalidData)?,
                entry.checked_at,
            ));
        }
        let mut request = self.client.get(url);
        if let Some(etag) = cached.as_ref().and_then(|v| v.etag.as_ref()) {
            request = request.header(IF_NONE_MATCH, etag);
        }
        self.perform(host, now, &mut gate, |now| {
            let response = self.send_checked(request, now, false)?;
            let entry = if response.status == StatusCode::NOT_MODIFIED {
                let mut entry = cached.ok_or(FetchError::InvalidData)?;
                entry.checked_at = response.checked_at;
                entry.revalidate = false;
                if response.etag.is_some() {
                    entry.etag = response.etag;
                }
                entry
            } else {
                Cached {
                    revalidate: false,
                    etag: response.etag,
                    body: response.body,
                    checked_at: response.checked_at,
                }
            };
            let decoded =
                serde_json::from_slice(&entry.body).map_err(|_| FetchError::InvalidData)?;
            let checked = entry.checked_at;
            let mut cache = self.cache.lock().map_err(|_| FetchError::Network)?;
            cache.remove(url);
            while cache.len() >= 64
                || cache.values().map(|v| v.body.len()).sum::<usize>() + entry.body.len()
                    > MAX_CACHE_BYTES
            {
                let oldest = cache
                    .iter()
                    .min_by_key(|(_, v)| v.checked_at)
                    .map(|(key, _)| key.clone());
                if let Some(oldest) = oldest {
                    cache.remove(&oldest);
                } else {
                    break;
                }
            }
            cache.insert(url.into(), entry);
            Ok((decoded, checked))
        })
    }
}
fn retry_after(value: &str, received: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|v| v.is_ascii_digit()) {
        return Some(
            value
                .parse::<i64>()
                .ok()
                .and_then(|v| TimeDelta::try_seconds(v.max(5)))
                .and_then(|v| received.checked_add_signed(v))
                .unwrap_or(DateTime::<Utc>::MAX_UTC),
        );
    }
    DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|v| v.with_timezone(&Utc))
        .or_else(|| {
            ["%A, %d-%b-%y %H:%M:%S GMT", "%a %b %e %H:%M:%S %Y"]
                .iter()
                .find_map(|format| {
                    chrono::NaiveDateTime::parse_from_str(value, format)
                        .ok()
                        .map(|v| v.and_utc())
                })
        })
}
