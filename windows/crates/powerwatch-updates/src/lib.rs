//! Signed release metadata shared by desktop and Android. No user database or
//! installation API: callers decide when to check and ask consent to install.
pub mod cache;
mod transport;
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, TimeDelta, Utc};
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;
pub use transport::ReleaseClient;
pub const MAX_MANIFEST: u64 = 262_144;
pub const MAX_PACKAGE: u64 = 524_288_000;
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("update network unavailable")]
    Network,
    #[error("update source unavailable")]
    Unavailable,
    #[error("update requested a pause")]
    RateLimited(DateTime<Utc>),
    #[error("invalid update metadata")]
    Invalid,
    #[error("update authenticity check failed")]
    Signature,
    #[error("update package integrity check failed")]
    Integrity,
    #[error("cannot store update package")]
    Storage,
}
impl UpdateError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Network => "update_network",
            Self::Unavailable => "update_unavailable",
            Self::RateLimited(_) => "update_rate_limited",
            Self::Invalid => "update_invalid",
            Self::Signature => "update_signature",
            Self::Integrity => "update_integrity",
            Self::Storage => "storage_error",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateConfig {
    pub repository: String,
    pub public_key: Option<String>,
    pub published: bool,
}
impl UpdateConfig {
    pub fn configured(&self) -> bool {
        self.published
            && self.public_key.as_ref().is_some_and(|k| !k.is_empty())
            && valid_repository(&self.repository)
    }
    pub fn endpoint(&self) -> Result<String, UpdateError> {
        if !valid_repository(&self.repository) {
            return Err(UpdateError::Invalid);
        }
        Ok(format!(
            "https://github.com/{}/releases/latest/download/powerwatch-update.json",
            self.repository
        ))
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub schema: u32,
    pub version: String,
    pub notes: String,
    pub pub_date: DateTime<Utc>,
    pub platforms: BTreeMap<String, ReleaseAsset>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseAsset {
    pub url: String,
    pub signature: Option<String>,
    pub sha256: String,
    pub size: u64,
    pub version_code: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload: String,
    signature: String,
}
pub fn verify_manifest(
    bytes: &[u8],
    config: &UpdateConfig,
    now: DateTime<Utc>,
) -> Result<ReleaseManifest, UpdateError> {
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err(UpdateError::Invalid);
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| UpdateError::Invalid)?;
    let payload = STANDARD
        .decode(&envelope.payload)
        .map_err(|_| UpdateError::Invalid)?;
    let key = config.public_key.as_deref().ok_or(UpdateError::Signature)?;
    let decode = |s: &str| {
        STANDARD
            .decode(s)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or(UpdateError::Signature)
    };
    let key = PublicKey::decode(&decode(key)?).map_err(|_| UpdateError::Signature)?;
    let signature =
        Signature::decode(&decode(&envelope.signature)?).map_err(|_| UpdateError::Signature)?;
    key.verify(&payload, &signature, false)
        .map_err(|_| UpdateError::Signature)?;
    let manifest: ReleaseManifest =
        serde_json::from_slice(&payload).map_err(|_| UpdateError::Invalid)?;
    validate_manifest(&manifest, &config.repository, now)?;
    Ok(manifest)
}
pub fn validate_manifest(
    manifest: &ReleaseManifest,
    repository: &str,
    now: DateTime<Utc>,
) -> Result<(), UpdateError> {
    if manifest.schema != 1
        || semver::Version::parse(&manifest.version).is_err()
        || manifest.pub_date > now + TimeDelta::minutes(5)
        || manifest.notes.len() > 16_384
        || manifest.platforms.is_empty()
        || manifest.platforms.len() > 8
    {
        return Err(UpdateError::Invalid);
    }
    for (platform, asset) in &manifest.platforms {
        if !matches!(
            platform.as_str(),
            "windows-x86_64" | "android-aarch64" | "android-x86_64"
        ) || !valid_asset_url(&asset.url, repository)
            || asset.size == 0
            || asset.size > MAX_PACKAGE
            || asset.sha256.len() != 64
            || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || (platform.starts_with("android-")
                && asset
                    .version_code
                    .is_none_or(|v| v == 0 || v > i32::MAX as u64))
            || (platform.starts_with("windows-")
                && asset
                    .signature
                    .as_ref()
                    .is_none_or(|s| s.is_empty() || s.len() > 4096))
        {
            return Err(UpdateError::Invalid);
        }
    }
    Ok(())
}
pub fn newer_version(candidate: &str, current: &str) -> Result<bool, UpdateError> {
    let candidate = semver::Version::parse(candidate).map_err(|_| UpdateError::Invalid)?;
    let current = semver::Version::parse(current).map_err(|_| UpdateError::Invalid)?;
    Ok(candidate > current && (!current.pre.is_empty() || candidate.pre.is_empty()))
}
pub fn valid_repository(repository: &str) -> bool {
    let mut parts = repository.split('/');
    let valid = |s: &str| {
        !s.is_empty()
            && s.len() <= 100
            && !s.starts_with('.')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    parts.next().is_some_and(valid) && parts.next().is_some_and(valid) && parts.next().is_none()
}
pub fn valid_asset_url(value: &str, repository: &str) -> bool {
    if !valid_repository(repository) {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url
            .path()
            .starts_with(&format!("/{repository}/releases/download/"))
        && !url.path().contains('%')
        && url.path().split('/').count() == 7
        && url
            .path()
            .split('/')
            .rev()
            .take(2)
            .all(|part| !part.is_empty())
}
pub fn verify_package(bytes: &[u8], asset: &ReleaseAsset) -> Result<(), UpdateError> {
    if bytes.len() as u64 != asset.size
        || format!("{:x}", Sha256::digest(bytes)) != asset.sha256.to_ascii_lowercase()
    {
        return Err(UpdateError::Integrity);
    }
    Ok(())
}
