use crate::*;
use reqwest::{StatusCode, blocking::Client, redirect::Policy};
use std::{io::Read, time::Duration};
pub struct ReleaseClient(pub(crate) Client);
impl ReleaseClient {
    pub fn new() -> Result<Self, UpdateError> {
        let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| UpdateError::Network)?
        .with_root_certificates(rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        })
        .with_no_client_auth();
        let client = Client::builder()
            .https_only(true)
            .retry(reqwest::retry::never())
            .tls_backend_preconfigured(tls)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(300))
            .user_agent(concat!("PowerWatch/", env!("CARGO_PKG_VERSION")))
            .redirect(Policy::custom(|attempt| {
                let url = attempt.url();
                if attempt.previous().len() >= 5
                    || url.scheme() != "https"
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || !matches!(
                        url.host_str(),
                        Some(
                            "github.com"
                                | "release-assets.githubusercontent.com"
                                | "objects.githubusercontent.com"
                        )
                    )
                {
                    attempt.error("untrusted release redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| UpdateError::Network)?;
        Ok(Self(client))
    }
    fn response(
        &self,
        url: &str,
        timeout: Duration,
    ) -> Result<reqwest::blocking::Response, UpdateError> {
        let response = self
            .0
            .get(url)
            .timeout(timeout)
            .send()
            .map_err(|_| UpdateError::Network)?;
        Self::checked_response(response, false)
    }
    pub(crate) fn checked_response(
        response: reqwest::blocking::Response,
        partial: bool,
    ) -> Result<reqwest::blocking::Response, UpdateError> {
        match response.status() {
            StatusCode::OK => Ok(response),
            StatusCode::PARTIAL_CONTENT if partial => Ok(response),
            StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => {
                let now = Utc::now();
                let retry = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok());
                let until = retry
                    .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
                    .map(|v| {
                        v.parse::<i64>()
                            .ok()
                            .and_then(TimeDelta::try_seconds)
                            .and_then(|d| now.checked_add_signed(d))
                            .unwrap_or(DateTime::<Utc>::MAX_UTC)
                    })
                    .or_else(|| {
                        retry
                            .and_then(|v| DateTime::parse_from_rfc2822(v).ok())
                            .map(|d| d.with_timezone(&Utc))
                    })
                    .filter(|d| *d > now)
                    .unwrap_or(now + TimeDelta::minutes(5));
                Err(UpdateError::RateLimited(until))
            }
            _ => Err(UpdateError::Unavailable),
        }
    }
    pub fn check(
        &self,
        config: &UpdateConfig,
        now: DateTime<Utc>,
    ) -> Result<ReleaseManifest, UpdateError> {
        let mut body = Vec::new();
        self.response(&config.endpoint()?, Duration::from_secs(30))?
            .take(MAX_MANIFEST + 1)
            .read_to_end(&mut body)
            .map_err(|_| UpdateError::Network)?;
        verify_manifest(&body, config, now)
    }
}
