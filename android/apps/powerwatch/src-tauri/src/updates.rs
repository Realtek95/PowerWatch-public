//! Update transport runs off the UI thread. Only a recently verified release
//! retained in Rust may be installed; the frontend never supplies a URL/path.
use crate::{AppState, CommandError};
use chrono::{DateTime, TimeDelta, Utc};
use powerwatch_updates::cache::SavedManifest;
use powerwatch_updates::{ReleaseClient, ReleaseManifest, UpdateConfig, UpdateError};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
pub fn config() -> Result<UpdateConfig, CommandError> {
    serde_json::from_str(include_str!("../../../../assets/update-config.json")).map_err(|_| {
        CommandError {
            code: "update_invalid",
        }
    })
}
impl From<UpdateError> for CommandError {
    fn from(error: UpdateError) -> Self {
        Self { code: error.code() }
    }
}
#[derive(Default)]
pub struct UpdateState(Mutex<Inner>);
pub fn maintain_cache(app: &tauri::AppHandle) {
    if let Ok(directory) = app.path().app_cache_dir() {
        let keep = app
            .state::<UpdateState>()
            .0
            .lock()
            .ok()
            .and_then(|inner| inner.busy.then(|| inner.downloaded.clone()).flatten());
        let _ = powerwatch_updates::cache::prune_installers(
            &directory.join("updates"),
            env!("CARGO_PKG_VERSION"),
            keep.as_deref(),
            std::time::SystemTime::now(),
        );
    }
}
struct Inner {
    restored: bool,
    status: &'static str,
    phase: &'static str,
    percent: u64,
    error: Option<&'static str>,
    package_state: &'static str,
    busy: bool,
    failed_download: bool,
    pending: Option<Pending>,
    downloaded: Option<std::path::PathBuf>,
}
impl Default for Inner {
    fn default() -> Self {
        Self {
            restored: false,
            status: "skipped",
            phase: "idle",
            percent: 0,
            error: None,
            package_state: "missing",
            busy: false,
            failed_download: false,
            pending: None,
            downloaded: None,
        }
    }
}
#[derive(Clone)]
struct Pending {
    manifest: ReleaseManifest,
    platform: String,
    checked_at: DateTime<Utc>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    status: &'static str,
    version: Option<String>,
    notes: Option<String>,
    size: Option<u64>,
    phase: &'static str,
    progress: u64,
    package_state: &'static str,
    error_code: Option<&'static str>,
    checked_at: Option<DateTime<Utc>>,
}
impl UpdateResult {
    fn empty(status: &'static str) -> Self {
        Self {
            status,
            version: None,
            notes: None,
            size: None,
            phase: "idle",
            progress: 0,
            package_state: "missing",
            error_code: None,
            checked_at: None,
        }
    }
}
fn snapshot(inner: &Inner) -> UpdateResult {
    let mut value = UpdateResult::empty(inner.status);
    if let Some(pending) = &inner.pending {
        value.checked_at = Some(pending.checked_at);
        value.version = Some(pending.manifest.version.clone());
        value.notes = Some(pending.manifest.notes.clone());
        value.size = Some(pending.manifest.platforms[&pending.platform].size);
    }
    value.phase = inner.phase;
    value.progress = inner.percent;
    value.package_state = inner.package_state;
    value.error_code = inner.error;
    value
}
fn state_result(app: &tauri::AppHandle) -> Result<UpdateResult, CommandError> {
    let state = app.state::<UpdateState>();
    let inner = state.0.lock().map_err(|_| CommandError {
        code: "internal_error",
    })?;
    Ok(snapshot(&inner))
}
fn report(app: &tauri::AppHandle, phase: &'static str, percent: u64) {
    if let Ok(mut inner) = app.state::<UpdateState>().0.lock() {
        inner.phase = phase;
        inner.percent = percent;
    }
    let _ = app.emit("update-phase", phase);
    let _ = app.emit("update-progress", percent);
}
fn begin(app: &tauri::AppHandle, phase: &'static str) -> Result<(), CommandError> {
    let state = app.state::<UpdateState>();
    let mut inner = state.0.lock().map_err(|_| CommandError {
        code: "internal_error",
    })?;
    if inner.busy {
        return Err(CommandError {
            code: "update_busy",
        });
    }
    inner.busy = true;
    inner.phase = phase;
    inner.error = None;
    Ok(())
}
fn finish(app: &tauri::AppHandle, error: Option<&'static str>) {
    if let Ok(mut inner) = app.state::<UpdateState>().0.lock() {
        inner.busy = false;
        inner.error = error;
        if error.is_some() {
            inner.phase = "error";
        }
    }
    if let Ok(value) = state_result(app) {
        let _ = app.emit("update-state", value);
    }
}
fn platform(app: &tauri::AppHandle, manifest: &ReleaseManifest) -> Result<String, CommandError> {
    let value = if cfg!(target_arch = "aarch64") {
        "android-aarch64"
    } else {
        "android-x86_64"
    };
    let asset = manifest.platforms.get(value).ok_or(CommandError {
        code: "update_unavailable",
    })?;
    {
        let info = crate::android::update_info(app)?;
        let code = info["versionCode"].as_u64().ok_or(CommandError {
            code: "update_invalid",
        })?;
        if asset.version_code.is_none_or(|v| v <= code) {
            return Err(UpdateError::Invalid.into());
        }
    }
    let _ = (app, asset);
    Ok(value.into())
}
fn attach(app: &tauri::AppHandle, pending: Pending) -> Result<UpdateResult, CommandError> {
    let directory = app
        .path()
        .app_cache_dir()
        .map_err(|_| CommandError {
            code: "storage_error",
        })?
        .join("updates");
    let asset = &pending.manifest.platforms[&pending.platform];
    let downloaded =
        powerwatch_updates::cache::cached_installer(&directory, &pending.manifest.version, asset)
            .map_err(|_| CommandError {
            code: "storage_error",
        })?;
    let extension = if pending.platform.starts_with("android") {
        "apk"
    } else {
        "exe"
    };
    let partial = directory.join(format!(
        "{}-{}.{}.part",
        pending.manifest.version,
        asset.sha256.to_ascii_lowercase(),
        extension
    ));
    let partial_size = partial
        .metadata()
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .unwrap_or(0);
    let package_state = if downloaded.is_some() {
        "ready"
    } else if partial_size > 0 && partial_size <= asset.size {
        "partial"
    } else {
        "missing"
    };
    let state = app.state::<UpdateState>();
    let mut inner = state.0.lock().map_err(|_| CommandError {
        code: "internal_error",
    })?;
    inner.percent = if package_state == "ready" {
        100
    } else {
        (partial_size.saturating_mul(100) / asset.size).min(99)
    };
    inner.status = "available";
    inner.phase = if package_state == "ready" {
        "ready"
    } else {
        "available"
    };
    inner.package_state = package_state;
    inner.downloaded = downloaded;
    inner.pending = Some(pending);
    inner.failed_download = package_state == "partial";
    Ok(snapshot(&inner))
}
fn restore(app: &tauri::AppHandle) -> Result<(), CommandError> {
    let configuration = config()?;
    if !configuration.configured() {
        return Ok(());
    }
    let saved = app
        .state::<AppState>()
        .repo()?
        .store
        .setting::<SavedManifest>("update_signed_manifest")?;
    let Some(saved) = saved else {
        return Ok(());
    };
    let now = Utc::now();
    let highest = app
        .state::<AppState>()
        .repo()?
        .store
        .setting::<String>("highest_update_version")?;
    let Some(manifest) = saved
        .verify(
            &configuration,
            env!("CARGO_PKG_VERSION"),
            highest.as_deref(),
            now,
        )
        .ok()
        .flatten()
    else {
        return Ok(());
    };
    let platform = platform(app, &manifest)?;
    attach(
        app,
        Pending {
            manifest,
            platform,
            checked_at: saved.checked_at,
        },
    )?;
    Ok(())
}
#[tauri::command]
pub async fn get_update_status(app: tauri::AppHandle) -> Result<UpdateResult, CommandError> {
    let needs_restore = {
        let state = app.state::<UpdateState>();
        let inner = state.0.lock().map_err(|_| CommandError {
            code: "internal_error",
        })?;
        !inner.busy && !inner.restored
    };
    if needs_restore {
        begin(&app, "verifying")?;
        app.state::<UpdateState>()
            .0
            .lock()
            .map_err(|_| CommandError {
                code: "internal_error",
            })?
            .restored = true;
        let worker = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || restore(&worker)).await;
        let result = result
            .map_err(|_| CommandError {
                code: "internal_error",
            })
            .and_then(|r| r);
        if result.is_ok()
            && app
                .state::<UpdateState>()
                .0
                .lock()
                .map_err(|_| CommandError {
                    code: "internal_error",
                })?
                .pending
                .is_none()
        {
            report(&app, "idle", 0);
        }
        finish(&app, result.as_ref().err().map(|e| e.code));
        result?;
    }
    {
        let state = app.state::<UpdateState>();
        let mut inner = state.0.lock().map_err(|_| CommandError {
            code: "internal_error",
        })?;
        if !inner.busy
            && inner
                .downloaded
                .as_ref()
                .is_some_and(|path| !path.is_file())
        {
            inner.downloaded = None;
            inner.package_state = "missing";
            inner.percent = 0;
            inner.phase = "available";
        }
    }
    state_result(&app)
}
#[tauri::command]
pub async fn check_updates(
    app: tauri::AppHandle,
    automatic: bool,
) -> Result<UpdateResult, CommandError> {
    let configuration = config()?;
    if !configuration.configured() {
        return Ok(UpdateResult::empty("disabled"));
    }
    begin(&app, "checking")?;
    let worker = app.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || check(&worker, configuration, automatic))
            .await;
    let result = result
        .map_err(|_| CommandError {
            code: "internal_error",
        })
        .and_then(|value| value);
    let result = if automatic
        && result.is_err()
        && app
            .state::<UpdateState>()
            .0
            .lock()
            .map_err(|_| CommandError {
                code: "internal_error",
            })?
            .package_state
            == "ready"
    {
        report(&app, "ready", 100);
        state_result(&app)
    } else {
        result
    };
    let result = if result.as_ref().is_ok_and(|value| value.status == "skipped") {
        let mut value = state_result(&app)?;
        if value.status == "available" {
            value.phase = if value.package_state == "ready" {
                "ready"
            } else {
                "available"
            };
            Ok(value)
        } else {
            result
        }
    } else {
        result
    };
    if let Ok(value) = &result
        && let Ok(mut inner) = app.state::<UpdateState>().0.lock()
    {
        inner.status = value.status;
        inner.phase = value.phase;
        if value.status == "current" {
            inner.pending = None;
            inner.downloaded = None;
            inner.package_state = "missing";
            inner.percent = 0;
        }
    }
    finish(&app, result.as_ref().err().map(|e| e.code));
    result
}
fn check(
    app: &tauri::AppHandle,
    configuration: UpdateConfig,
    automatic: bool,
) -> Result<UpdateResult, CommandError> {
    maintain_cache(app);
    let now = Utc::now();
    if !automatic {
        let state = app.state::<UpdateState>();
        let inner = state.0.lock().map_err(|_| CommandError {
            code: "internal_error",
        })?;
        if let Some(pending) = &inner.pending
            && now - pending.checked_at < TimeDelta::minutes(30)
        {
            let pending = pending.clone();
            drop(inner);
            return attach(app, pending);
        }
    }
    {
        let state = app.state::<AppState>();
        let repo = state.repo()?;
        if automatic
            && !repo
                .store
                .setting::<bool>("automatic_updates")?
                .unwrap_or(true)
        {
            return Ok(UpdateResult::empty("skipped"));
        }
        if repo
            .store
            .setting::<DateTime<Utc>>("update_pause_until")?
            .is_some_and(|until| until > now)
        {
            return Err(CommandError {
                code: "update_rate_limited",
            });
        }
        if let Some(last) = repo.store.setting::<DateTime<Utc>>("last_update_check")?
            && now - last
                < if automatic {
                    TimeDelta::days(1)
                } else {
                    TimeDelta::seconds(60)
                }
        {
            return Ok(UpdateResult::empty("skipped"));
        }
        repo.store.set_setting("last_update_check", &now)?;
    }
    let (manifest, envelope) = match ReleaseClient::new()?.check_with_envelope(&configuration, now)
    {
        Err(UpdateError::RateLimited(until)) => {
            app.state::<AppState>()
                .repo()?
                .store
                .set_setting("update_pause_until", &until)?;
            return Err(UpdateError::RateLimited(until).into());
        }
        result => result?,
    };
    {
        let state = app.state::<AppState>();
        let repo = state.repo()?;
        if let Some(highest) = repo.store.setting::<String>("highest_update_version")?
            && powerwatch_updates::newer_version(&highest, &manifest.version)?
        {
            return Err(UpdateError::Invalid.into());
        }
        repo.store
            .set_setting("highest_update_version", &manifest.version)?;
    }
    if !powerwatch_updates::newer_version(&manifest.version, env!("CARGO_PKG_VERSION"))? {
        return Ok(UpdateResult::empty("current"));
    }
    let platform = platform(app, &manifest)?;
    let envelope = String::from_utf8(envelope).map_err(|_| CommandError {
        code: "update_invalid",
    })?;
    app.state::<AppState>().repo()?.store.set_setting(
        "update_signed_manifest",
        &SavedManifest {
            envelope,
            checked_at: now,
        },
    )?;
    attach(
        app,
        Pending {
            manifest,
            platform,
            checked_at: now,
        },
    )
}
#[tauri::command]
pub async fn download_update(app: tauri::AppHandle) -> Result<UpdateResult, CommandError> {
    begin(&app, "downloading")?;
    let pending = app
        .state::<UpdateState>()
        .0
        .lock()
        .map_err(|_| CommandError {
            code: "internal_error",
        })?
        .pending
        .clone();
    let Some(pending) = pending.filter(|p| Utc::now() - p.checked_at < TimeDelta::minutes(30))
    else {
        finish(&app, Some("update_expired"));
        return Err(CommandError {
            code: "update_expired",
        });
    };
    let worker = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || download(&worker, &pending))
        .await
        .map_err(|_| CommandError {
            code: "internal_error",
        })
        .and_then(|r| r);
    if result.is_ok() {
        if let Ok(mut inner) = app.state::<UpdateState>().0.lock() {
            inner.package_state = "ready";
        }
        report(&app, "ready", 100);
    }
    if result.is_err() {
        let pending = app
            .state::<UpdateState>()
            .0
            .lock()
            .ok()
            .and_then(|inner| inner.pending.clone());
        if let Some(pending) = pending {
            let _ = attach(&app, pending);
        }
    }
    finish(&app, result.as_ref().err().map(|e| e.code));
    result?;
    state_result(&app)
}
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<&'static str, CommandError> {
    begin(&app, "installing")?;
    let pending = app
        .state::<UpdateState>()
        .0
        .lock()
        .map_err(|_| CommandError {
            code: "internal_error",
        })?
        .pending
        .clone();
    let Some(pending) = pending.filter(|p| Utc::now() - p.checked_at < TimeDelta::days(7)) else {
        finish(&app, Some("update_expired"));
        return Err(CommandError {
            code: "update_expired",
        });
    };
    let worker = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let directory = worker
            .path()
            .app_cache_dir()
            .map_err(|_| CommandError {
                code: "storage_error",
            })?
            .join("updates");
        let path = powerwatch_updates::cache::cached_installer(
            &directory,
            &pending.manifest.version,
            &pending.manifest.platforms[&pending.platform],
        )
        .map_err(|_| CommandError {
            code: "storage_error",
        })?
        .ok_or(CommandError {
            code: "update_not_downloaded",
        })?;
        Ok((pending, path))
    })
    .await
    .map_err(|_| CommandError {
        code: "internal_error",
    })
    .and_then(|r: Result<_, CommandError>| r);
    let result = result.and_then(|(pending, path)| {
        let asset = &pending.manifest.platforms[&pending.platform];
        crate::android::install_update(&app, &path, asset.version_code.unwrap(), &asset.sha256)
    });
    if let Ok(status) = &result {
        report(&app, status, 100);
    }
    finish(&app, result.as_ref().err().map(|e| e.code));
    result
}
/// Cached installers remain usable offline; each new download attempt is gated.
fn reserve_download(
    store: &powerwatch_storage::Store,
    now: DateTime<Utc>,
    continuing: bool,
) -> Result<(), CommandError> {
    if store
        .setting::<DateTime<Utc>>("update_pause_until")?
        .is_some_and(|until| until > now)
        || (!continuing
            && store
                .setting::<DateTime<Utc>>("last_update_download_attempt")?
                .is_some_and(|last| now - last < TimeDelta::seconds(60)))
    {
        return Err(CommandError {
            code: "update_rate_limited",
        });
    }
    store.set_setting("last_update_download_attempt", &now)?;
    Ok(())
}
fn download(
    app: &tauri::AppHandle,
    pending: &Pending,
) -> Result<(Pending, std::path::PathBuf), CommandError> {
    if let Some(path) = app
        .state::<UpdateState>()
        .0
        .lock()
        .map_err(|_| CommandError {
            code: "internal_error",
        })?
        .downloaded
        .clone()
        .filter(|p| p.is_file())
    {
        return Ok((pending.clone(), path));
    }
    let directory = app
        .path()
        .app_cache_dir()
        .map_err(|_| CommandError {
            code: "storage_error",
        })?
        .join("updates");
    std::fs::create_dir_all(&directory).map_err(|_| CommandError {
        code: "storage_error",
    })?;
    let asset = &pending.manifest.platforms[&pending.platform];
    if let Some(path) =
        powerwatch_updates::cache::cached_installer(&directory, &pending.manifest.version, asset)
            .map_err(|_| CommandError {
                code: "storage_error",
            })?
    {
        app.state::<UpdateState>()
            .0
            .lock()
            .map_err(|_| CommandError {
                code: "internal_error",
            })?
            .downloaded = Some(path.clone());
        return Ok((pending.clone(), path));
    }
    let extension = if pending.platform.starts_with("android") {
        "apk"
    } else {
        "exe"
    };
    let path = directory.join(format!(
        "{}-{}.{}",
        pending.manifest.version,
        asset.sha256.to_ascii_lowercase(),
        extension
    ));
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if !metadata.file_type().is_file() {
            return Err(CommandError {
                code: "storage_error",
            });
        }
        std::fs::remove_file(&path).map_err(|_| CommandError {
            code: "storage_error",
        })?;
    }
    let partial = path.with_extension(format!("{extension}.part"));
    reserve_download(
        &app.state::<AppState>().repo()?.store,
        Utc::now(),
        partial.is_file()
            || app
                .state::<UpdateState>()
                .0
                .lock()
                .map_err(|_| CommandError {
                    code: "internal_error",
                })?
                .failed_download,
    )?;
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file()) {
        std::fs::remove_file(&path).map_err(|_| CommandError {
            code: "storage_error",
        })?;
    }
    let _ = powerwatch_updates::cache::prune_installers(
        &directory,
        env!("CARGO_PKG_VERSION"),
        Some(&path),
        std::time::SystemTime::now(),
    );
    let mut percent = 0;
    let downloaded = ReleaseClient::new()?.download_with_status(
        asset,
        &config()?.repository,
        &path,
        |received, total| {
            let current = (received * 100 / total).min(99);
            if current != percent {
                percent = current;
                let phase = app
                    .state::<UpdateState>()
                    .0
                    .lock()
                    .map(|inner| inner.phase)
                    .unwrap_or("downloading");
                report(app, phase, current);
            }
        },
        |stage| {
            let phase = match stage {
                powerwatch_updates::TransferStage::Downloading => "downloading",
                powerwatch_updates::TransferStage::Waiting => "waiting",
                powerwatch_updates::TransferStage::Verifying => "verifying",
            };
            let percent = app
                .state::<UpdateState>()
                .0
                .lock()
                .map(|inner| inner.percent)
                .unwrap_or(0);
            report(app, phase, percent);
        },
    );
    if let Err(UpdateError::RateLimited(until)) = &downloaded {
        app.state::<AppState>()
            .repo()?
            .store
            .set_setting("update_pause_until", until)?;
    }
    app.state::<UpdateState>()
        .0
        .lock()
        .map_err(|_| CommandError {
            code: "internal_error",
        })?
        .failed_download = downloaded.is_err();
    downloaded?;
    app.state::<UpdateState>()
        .0
        .lock()
        .map_err(|_| CommandError {
            code: "internal_error",
        })?
        .downloaded = Some(path.clone());
    maintain_cache(app);
    Ok((pending.clone(), path))
}
