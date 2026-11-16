//! Update transport runs off the UI thread. Only a recently verified release
//! retained in Rust may be installed; the frontend never supplies a URL/path.
use crate::{AppState, CommandError};
use chrono::{DateTime, TimeDelta, Utc};
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
            .and_then(|inner| inner.downloaded.clone());
        let _ = powerwatch_updates::cache::prune_installers(
            &directory.join("updates"),
            env!("CARGO_PKG_VERSION"),
            keep.as_deref(),
            std::time::SystemTime::now(),
        );
    }
}
#[derive(Default)]
struct Inner {
    busy: bool,
    pending: Option<Pending>,
    downloaded: Option<std::path::PathBuf>,
}
#[derive(Clone)]
struct Pending {
    manifest: ReleaseManifest,
    platform: String,
    checked_at: DateTime<Utc>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    status: &'static str,
    version: Option<String>,
    notes: Option<String>,
    size: Option<u64>,
}
impl UpdateResult {
    fn empty(status: &'static str) -> Self {
        Self {
            status,
            version: None,
            notes: None,
            size: None,
        }
    }
}
fn begin(app: &tauri::AppHandle) -> Result<(), CommandError> {
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
    Ok(())
}
fn finish(app: &tauri::AppHandle) {
    if let Ok(mut inner) = app.state::<UpdateState>().0.lock() {
        inner.busy = false;
    }
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
    begin(&app)?;
    let worker = app.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || check(&worker, configuration, automatic))
            .await;
    finish(&app);
    result.map_err(|_| CommandError {
        code: "internal_error",
    })?
}
fn check(
    app: &tauri::AppHandle,
    configuration: UpdateConfig,
    automatic: bool,
) -> Result<UpdateResult, CommandError> {
    maintain_cache(app);
    let now = Utc::now();
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
    app.state::<UpdateState>()
        .0
        .lock()
        .map_err(|_| CommandError {
            code: "internal_error",
        })?
        .pending = None;
    let manifest = match ReleaseClient::new()?.check(&configuration, now) {
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
    let platform = "windows-x86_64".to_string();
    let asset = manifest.platforms.get(&platform).ok_or(CommandError {
        code: "update_unavailable",
    })?;
    let result = UpdateResult {
        status: "available",
        version: Some(manifest.version.clone()),
        notes: Some(manifest.notes.clone()),
        size: Some(asset.size),
    };
    {
        let state = app.state::<UpdateState>();
        let mut inner = state.0.lock().map_err(|_| CommandError {
            code: "internal_error",
        })?;
        inner.pending = Some(Pending {
            manifest,
            platform,
            checked_at: now,
        });
        inner.downloaded = None;
    }
    Ok(result)
}
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<&'static str, CommandError> {
    begin(&app)?;
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
        finish(&app);
        return Err(CommandError {
            code: "update_expired",
        });
    };
    let worker = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || download(&worker, &pending)).await;
    let result = match result {
        Ok(result) => result,
        Err(_) => Err(CommandError {
            code: "internal_error",
        }),
    };
    let result = match result {
        Ok((pending, path)) => install_windows(&app, pending, path).await,
        Err(error) => Err(error),
    };
    finish(&app);
    result
}
/// Cached installers remain usable offline; each new download attempt is gated.
fn reserve_download(
    store: &powerwatch_storage::Store,
    now: DateTime<Utc>,
) -> Result<(), CommandError> {
    if store
        .setting::<DateTime<Utc>>("update_pause_until")?
        .is_some_and(|until| until > now)
        || store
            .setting::<DateTime<Utc>>("last_update_download_attempt")?
            .is_some_and(|last| now - last < TimeDelta::seconds(60))
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
    reserve_download(&app.state::<AppState>().repo()?.store, Utc::now())?;
    maintain_cache(app);
    let extension = if pending.platform.starts_with("android") {
        "apk"
    } else {
        "exe"
    };
    let path = directory.join(format!(
        "{}-{}.{}",
        pending.manifest.version,
        Utc::now().timestamp_micros(),
        extension
    ));
    let _ = powerwatch_updates::cache::prune_installers(
        &directory,
        env!("CARGO_PKG_VERSION"),
        Some(&path),
        std::time::SystemTime::now(),
    );
    let mut percent = 0;
    let downloaded =
        ReleaseClient::new()?.download(asset, &config()?.repository, &path, |received, total| {
            let current = received * 100 / total;
            if current != percent {
                percent = current;
                let _ = app.emit("update-progress", current);
            }
        });
    if let Err(UpdateError::RateLimited(until)) = &downloaded {
        app.state::<AppState>()
            .repo()?
            .store
            .set_setting("update_pause_until", until)?;
    }
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
async fn install_windows(
    app: &tauri::AppHandle,
    pending: Pending,
    path: std::path::PathBuf,
) -> Result<&'static str, CommandError> {
    use tauri_plugin_updater::UpdaterExt;
    let configuration = config()?;
    let updater = app
        .updater_builder()
        .pubkey(configuration.public_key.unwrap_or_default())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| CommandError {
            code: "update_invalid",
        })?;
    let raw = serde_json::to_value(&pending.manifest).map_err(|_| CommandError {
        code: "update_invalid",
    })?;
    let update = updater
        .restore_update(raw)
        .map_err(|_| CommandError {
            code: "update_invalid",
        })?
        .ok_or(CommandError {
            code: "update_expired",
        })?;
    let asset = &pending.manifest.platforms[&pending.platform];
    if update.download_url.as_str() != asset.url {
        return Err(UpdateError::Invalid.into());
    }
    let bytes = std::fs::read(&path).map_err(|_| CommandError {
        code: "storage_error",
    })?;
    powerwatch_updates::verify_package(&bytes, asset)?;
    let result = update.install(bytes).map_err(|_| CommandError {
        code: "update_signature",
    });
    let _ = std::fs::remove_file(path);
    result.map(|()| "installer_opened")
}
