//! Thin Tauri IPC boundary; the UI does not implement scheduling rules.
mod about;
mod background;
mod updates;
use about::{get_app_info, open_app_link, set_automatic_updates};
mod geography;
use geography::get_country_catalog;
mod desktop;
mod repository;
mod text;
use background::*;
use chrono::Days;
use chrono::{DateTime, NaiveDate, Utc};
use powerwatch_core::{
    cache::DashboardCache,
    dashboard::Dashboard,
    grid::{AddressInput, OperatorSelection, Queue},
    model::CatalogEntry,
    monitoring::MonitoringSettings,
    session::{SessionError, SessionState},
};
use powerwatch_providers::ProviderError;
use powerwatch_storage::{ArchiveView, StorageError, Store, address_key};
use repository::Repository;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Manager;
/// Expose stable error codes, never technical exception text, to the UI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandError {
    code: &'static str,
}
impl From<powerwatch_providers::http::FetchError> for CommandError {
    fn from(error: powerwatch_providers::http::FetchError) -> Self {
        Self { code: error.code() }
    }
}
impl From<StorageError> for CommandError {
    fn from(_: StorageError) -> Self {
        Self {
            code: "storage_error",
        }
    }
}
impl From<ProviderError> for CommandError {
    fn from(error: ProviderError) -> Self {
        Self { code: error.code() }
    }
}
struct AppState {
    catalog_directory: std::path::PathBuf,
    repository: Mutex<Repository>,
    cache: Mutex<DashboardCache>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeInfo {
    start_in_settings: bool,
    platform: &'static str,
    developer_tools: bool,
    local_projection: bool,
    now: DateTime<Utc>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewPreferences {
    theme: String,
    view: String,
    font_size: String,
}
#[tauri::command]
fn get_view_preferences(
    state: tauri::State<'_, AppState>,
) -> Result<Option<ViewPreferences>, CommandError> {
    Ok(state.repo()?.store.setting("view_preferences")?)
}
#[tauri::command]
fn set_view_preferences(
    preferences: ViewPreferences,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    if !["system", "light", "dark"].contains(&preferences.theme.as_str())
        || !["scale", "list"].contains(&preferences.view.as_str())
        || !["normal", "large"].contains(&preferences.font_size.as_str())
    {
        return Err(CommandError {
            code: "invalid_selection",
        });
    }
    state
        .repo()?
        .store
        .set_setting("view_preferences", &preferences)?;
    Ok(())
}
#[tauri::command(async)]
fn get_runtime_info(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RuntimeInfo, CommandError> {
    let settings = {
        let _ = app;
        false
    };
    Ok(RuntimeInfo {
        start_in_settings: settings,
        platform: "windows",
        developer_tools: false,
        local_projection: true,
        now: state.now()?,
    })
}
#[tauri::command]
fn project_dashboard(
    address_id: String,
    day_offset: u64,
    state: tauri::State<'_, AppState>,
) -> Result<Dashboard, CommandError> {
    let session = get_session(state.clone())?;
    let now = state.now()?;
    let snapshots = {
        let repository = state.repo()?;
        session
            .addresses
            .iter()
            .map(|saved| {
                Ok((
                    saved.id.clone(),
                    repository
                        .store
                        .cached(&address_key(saved))?
                        .filter(|snapshot| !snapshot.is_demo),
                ))
            })
            .collect::<Result<Vec<_>, StorageError>>()?
    };
    let ids: Vec<_> = session
        .addresses
        .iter()
        .map(|saved| saved.id.as_str())
        .collect();
    let refresh_pending = state.repo()?.store.manual_refresh_pending(&address_id)?;
    let mut cache = state.cache.lock().map_err(|_| CommandError {
        code: "internal_error",
    })?;
    cache.retain_addresses(&ids);
    for (id, snapshot) in snapshots {
        if let Some(snapshot) = snapshot {
            cache.record(&id, snapshot);
        }
    }
    let mut dashboard = cache
        .project(&address_id, day_offset, now, session.check_interval_minutes)
        .map_err(|_| CommandError {
            code: if refresh_pending {
                "refresh_queued"
            } else {
                "projection_expired"
            },
        })?;
    dashboard.refresh_pending = refresh_pending;
    drop(cache);
    let saved = session
        .addresses
        .iter()
        .find(|entry| entry.id == address_id)
        .ok_or(CommandError {
            code: "address_not_found",
        })?;
    if dashboard.snapshot.address.id != saved.address.id
        || dashboard.snapshot.provider_id != saved.address.provider_id
    {
        return Err(CommandError {
            code: "projection_expired",
        });
    }
    dashboard.snapshot.address.label = saved.address.label.clone();
    let poll = state
        .repo()?
        .store
        .poll_states()?
        .into_iter()
        .find(|poll| poll.address_key == address_key(saved));
    dashboard.retry_pending = poll
        .as_ref()
        .is_some_and(|poll| poll.error_code.is_some() && poll.failures < 3);
    dashboard.last_check_error = poll.and_then(|poll| poll.error_code);
    Ok(dashboard)
}
impl AppState {
    fn repo(&self) -> Result<std::sync::MutexGuard<'_, Repository>, CommandError> {
        self.repository.lock().map_err(|_| CommandError {
            code: "internal_error",
        })
    }
    fn now(&self) -> Result<DateTime<Utc>, CommandError> {
        Ok(Utc::now())
    }
}
impl From<SessionError> for CommandError {
    fn from(error: SessionError) -> Self {
        Self {
            code: match error {
                SessionError::AddressNotFound => "address_not_found",
                SessionError::DuplicateAddress => "duplicate_address",
                SessionError::InvalidName => "invalid_name",
                SessionError::InvalidMonitoring => "invalid_monitoring",
                SessionError::InvalidSelection | SessionError::InvalidPreferences => {
                    "invalid_selection"
                }
            },
        }
    }
}
#[tauri::command]
fn get_session(state: tauri::State<'_, AppState>) -> Result<SessionState, CommandError> {
    Ok(state.repo()?.book.state())
}
#[tauri::command]
fn set_monitoring(
    interval: u16,
    monitoring: MonitoringSettings,
    state: tauri::State<'_, AppState>,
) -> Result<SessionState, CommandError> {
    state
        .repo()?
        .update(|book| book.set_monitoring(interval, monitoring), None)
}
#[tauri::command]
fn get_oblasts() -> Vec<CatalogEntry> {
    powerwatch_providers::catalog::oblasts()
}
#[tauri::command]
fn get_local_settlements(oblast_id: String) -> Vec<CatalogEntry> {
    powerwatch_providers::catalog::settlements(&oblast_id)
}
#[tauri::command]
fn get_address_lookup_support(operator_id: String, settlement_id: String) -> bool {
    powerwatch_providers::live::service::LiveService::address_lookup_supported(
        &operator_id,
        &settlement_id,
    )
}
#[tauri::command]
fn get_operators(oblast_id: Option<String>) -> Vec<CatalogEntry> {
    powerwatch_providers::registry::all()
        .iter()
        .filter(|item| {
            oblast_id
                .as_ref()
                .is_none_or(|id| item.operator.oblast_ids.contains(id))
        })
        .map(|item| CatalogEntry {
            id: item.operator.id.clone(),
            name: item.operator.name.clone(),
        })
        .collect()
}
#[tauri::command]
async fn get_operator_groups(operator_id: String) -> Result<Vec<CatalogEntry>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let groups = powerwatch_providers::live::service::LiveService::shared()?
            .groups(&operator_id, Utc::now())?;
        Ok(groups
            .into_iter()
            .map(|id| CatalogEntry {
                name: id.clone(),
                id,
            })
            .collect())
    })
    .await
    .map_err(|_| CommandError {
        code: "internal_error",
    })?
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddressGroupRequest {
    operator_id: String,
    settlement_id: String,
    street_id: String,
    house_id: String,
    input: AddressInput,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddressResolveRequest {
    operator_id: String,
    settlement_id: String,
    input: AddressInput,
}
#[tauri::command]
async fn resolve_address_group(
    request: AddressResolveRequest,
) -> Result<serde_json::Value, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let verified = powerwatch_providers::live::service::LiveService::shared()?
            .resolve_address(
                &request.operator_id,
                &request.settlement_id,
                &request.input,
                Utc::now(),
            )?;
        Ok(serde_json::json!(
            { "streetId" : verified.catalog.street_id, "houseId" : verified
            .catalog.house_id, "queue" : verified.queue.map(| queue | queue.code)
            }
        ))
    })
    .await
    .map_err(|_| CommandError {
        code: "internal_error",
    })?
}
#[tauri::command]
async fn preview_address_group(
    request: AddressGroupRequest,
) -> Result<Option<String>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let verified = powerwatch_providers::live::service::LiveService::shared()?
            .confirm_address(
                &request.operator_id,
                &request.settlement_id,
                &request.input,
                (&request.street_id, &request.house_id),
                Utc::now(),
            )?;
        Ok(verified.queue.map(|queue| queue.code))
    })
    .await
    .map_err(|_| CommandError {
        code: "internal_error",
    })?
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddressCatalogQuery {
    operator_id: String,
    settlement_id: String,
    street_id: Option<String>,
    query: String,
}
#[tauri::command]
async fn search_address_catalog(
    request: AddressCatalogQuery,
) -> Result<Vec<CatalogEntry>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let service = powerwatch_providers::live::service::LiveService::shared()?;
        Ok(if let Some(street) = request.street_id {
            service.address_houses(
                &request.operator_id,
                &request.settlement_id,
                &street,
                &request.query,
                Utc::now(),
            )?
        } else {
            service.address_streets(
                &request.operator_id,
                &request.settlement_id,
                &request.query,
                Utc::now(),
            )?
        })
    })
    .await
    .map_err(|_| CommandError {
        code: "internal_error",
    })?
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManualAddressRequest {
    geographic: Option<powerwatch_addresses::GeographicSelection>,
    street_id: Option<String>,
    house_id: Option<String>,
    input: AddressInput,
    oblast_id: Option<String>,
    settlement_id: Option<String>,
    label: String,
    editing_id: Option<String>,
    operator_id: Option<String>,
    queue: Option<String>,
}
#[tauri::command]
async fn save_manual_address(
    selection: ManualAddressRequest,
    state: tauri::State<'_, AppState>,
) -> Result<SessionState, CommandError> {
    let ManualAddressRequest {
        geographic,
        street_id,
        house_id,
        input,
        oblast_id,
        settlement_id,
        label,
        editing_id,
        operator_id,
        queue,
    } = selection;
    if let Some(reference) = &geographic {
        let directory = state.catalog_directory.clone();
        let oblast = oblast_id.clone().ok_or(SessionError::InvalidSelection)?;
        let reference = reference.clone();
        if settlement_id.as_deref() != Some(&reference.settlement_id) {
            return Err(SessionError::InvalidSelection.into());
        }
        let value = input.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(), CommandError> {
            geography::open(&directory, &oblast)?.validate(&oblast, &reference, &value)?;
            Ok(())
        })
        .await
        .map_err(|_| CommandError {
            code: "internal_error",
        })??;
    }
    let territory = match (oblast_id, settlement_id) {
        (Some(oblast), Some(settlement)) => {
            let (oblast, name) = powerwatch_providers::catalog::resolve(&oblast, &settlement)
                .ok_or(SessionError::InvalidSelection)?;
            if input.oblast.trim() != oblast.name || input.settlement.trim() != name {
                return Err(SessionError::InvalidSelection.into());
            }
            Some((oblast, Some(settlement)))
        }
        (Some(id), None) => {
            let oblast =
                powerwatch_providers::catalog::oblast(&id).ok_or(SessionError::InvalidSelection)?;
            if input.oblast.trim() != oblast.name {
                return Err(SessionError::InvalidSelection.into());
            }
            Some((oblast, None))
        }
        (None, None) => None,
        (None, Some(_)) => return Err(SessionError::InvalidSelection.into()),
    };
    let verification_input = input.clone();
    let operator_selection =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, CommandError> {
            match operator_id {
                Some(id) => {
                    let record = powerwatch_providers::registry::find(&id)
                        .ok_or(SessionError::InvalidSelection)?;
                    if territory
                        .as_ref()
                        .is_some_and(|(oblast, _)| !record.operator.oblast_ids.contains(&oblast.id))
                    {
                        return Err(SessionError::InvalidSelection.into());
                    }
                    if let Some(code) = &queue {
                        let groups = powerwatch_providers::live::service::LiveService::shared()?
                            .groups(&id, Utc::now())?;
                        if !groups.contains(code) {
                            return Err(SessionError::InvalidSelection.into());
                        }
                    }
                    let verification = match (street_id, house_id) {
                        (Some(street), Some(house)) => {
                            let city = territory
                                .as_ref()
                                .and_then(|(_, city)| city.as_deref())
                                .ok_or(SessionError::InvalidSelection)?;
                            Some(
                                powerwatch_providers::live::service::LiveService::shared()?
                                    .confirm_address(
                                        &id,
                                        city,
                                        &verification_input,
                                        (&street, &house),
                                        Utc::now(),
                                    )?,
                            )
                        }
                        (None, None) => None,
                        _ => return Err(SessionError::InvalidSelection.into()),
                    };
                    Ok((
                        territory,
                        Some(OperatorSelection {
                            operator: record.operator.clone(),
                            queue: queue.map(|code| Queue {
                                operator_id: id,
                                code,
                            }),
                        }),
                        verification,
                    ))
                }
                None if queue.is_none() && street_id.is_none() && house_id.is_none() => {
                    Ok((territory, None, None))
                }
                None => Err(SessionError::InvalidSelection.into()),
            }
        })
        .await
        .map_err(|_| CommandError {
            code: "internal_error",
        })??;
    let (territory, selection, verification) = operator_selection;
    state.repo()?.update(
        |book| {
            let saved =
                book.save_manual(input, territory, &label, editing_id.as_deref(), selection)?;
            let id = saved
                .active_address_id
                .as_deref()
                .ok_or(SessionError::AddressNotFound)?
                .to_owned();
            if let Some(reference) = geographic {
                book.set_geographic(&id, reference)?;
            }
            if let Some(verification) = verification {
                let id = saved
                    .active_address_id
                    .ok_or(SessionError::AddressNotFound)?;
                book.confirm_address(&id, verification)
            } else {
                Ok(book.state())
            }
        },
        None,
    )
}
#[tauri::command]
fn activate_address(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<SessionState, CommandError> {
    state.repo()?.update(|book| book.activate(&id), None)
}
#[tauri::command]
fn delete_address(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<SessionState, CommandError> {
    let mut repository = state.repo()?;
    let key = address_key(repository.book.get(&id)?);
    let session = repository.update(|book| book.remove(&id), Some(key.as_str()))?;
    drop(repository);
    state
        .cache
        .lock()
        .map_err(|_| CommandError {
            code: "internal_error",
        })?
        .retain_addresses(
            &session
                .addresses
                .iter()
                .map(|saved| saved.id.as_str())
                .collect::<Vec<_>>(),
        );
    Ok(session)
}
#[tauri::command]
fn complete_onboarding(
    interval: u16,
    state: tauri::State<'_, AppState>,
) -> Result<SessionState, CommandError> {
    state
        .repo()?
        .update(|book| book.complete_onboarding(interval), None)
}
#[tauri::command]
async fn get_dashboard(
    address_id: String,
    day_offset: u64,
    state: tauri::State<'_, AppState>,
) -> Result<Dashboard, CommandError> {
    let saved = state.repo()?.book.get(&address_id)?.clone();
    if saved.address.provider_id == "manual"
        && saved.address.details.as_ref().is_none_or(|details| {
            details.queue.is_none() && details.selection.as_ref().is_none_or(|s| s.queue.is_none())
        })
    {
        return Err(CommandError {
            code: "unresolved_address",
        });
    }
    let lease = {
        let mut repository = state.repo()?;
        powerwatch_runtime::claim_manual_poll(&mut repository.store, &saved, Utc::now())?
    };
    let Some(lease) = lease else {
        return project_dashboard(address_id, day_offset, state);
    };
    let fetched = {
        let address = saved.address.clone();
        let previous = state.repo()?.store.cached(&address_key(&saved))?;
        tauri::async_runtime::spawn_blocking(move || -> Result<Dashboard, ProviderError> {
            if day_offset > 1 {
                return Err(powerwatch_core::CoreError::MissingDay.into());
            }
            let snapshot = powerwatch_providers::live::service::LiveService::shared()?
                .refresh_with_previous(&address, Utc::now(), previous.as_ref())?;
            let now = Utc::now();
            let date = now
                .with_timezone(&powerwatch_core::time::LOCAL_ZONE)
                .date_naive()
                .checked_add_days(Days::new(day_offset))
                .ok_or(powerwatch_core::CoreError::InvalidTime)?;
            Ok(powerwatch_core::dashboard::build_dashboard(
                snapshot, date, now,
            )?)
        })
        .await
        .unwrap_or(Err(ProviderError::Unavailable))
    };
    let now = state.now()?;
    let mut repository = state.repo()?;
    let key = address_key(&saved);
    let interval = repository.book.state().check_interval_minutes;
    let result = (|| -> Result<Dashboard, CommandError> {
        let current = repository.book.get(&address_id)?.clone();
        if address_key(&current) != key {
            return Err(SessionError::InvalidSelection.into());
        }
        let failures = repository
            .store
            .poll_states()?
            .iter()
            .find(|poll| poll.address_key == key)
            .map(|poll| poll.failures)
            .unwrap_or(0);
        match fetched {
            Err(error) => {
                let due = powerwatch_runtime::retry_after_source_pauses(
                    &repository.store,
                    &current.address,
                    powerwatch_runtime::next_poll_at(now, interval, failures, true),
                )?;
                repository
                    .store
                    .complete_poll(&key, now, due, Some(error.code()))?;
                Err(error.into())
            }
            Ok(mut dashboard) => {
                dashboard.snapshot.address.label = current.address.label.clone();
                let previous = repository.store.cached(&key)?;
                let mut preferences = repository.store.notification_preferences()?;
                preferences.enabled &= current.notifications_enabled;
                let plans = powerwatch_notifications::plan_snapshot(
                    &key,
                    &saved.id,
                    &dashboard.snapshot,
                    previous.as_ref(),
                    now,
                    interval,
                    &preferences,
                )
                .map_err(|_| CommandError {
                    code: "invalid_data",
                })?;
                repository.store.ingest_with_plans(
                    &key,
                    dashboard.snapshot.clone(),
                    now,
                    Some(&plans),
                )?;
                repository.book = repository.store.load_book()?;
                let failed = dashboard.snapshot.forecast_error.is_some();
                let due = powerwatch_runtime::next_poll_at(now, interval, failures, failed);
                let due = if failed {
                    powerwatch_runtime::retry_after_source_pauses(
                        &repository.store,
                        &current.address,
                        due,
                    )?
                } else {
                    due
                };
                repository.store.complete_poll(
                    &key,
                    now,
                    due,
                    dashboard.snapshot.forecast_error.as_deref(),
                )?;
                Ok(dashboard)
            }
        }
    })();
    repository.store.release_poll(&lease)?;
    drop(repository);
    let dashboard = match result {
        Ok(dashboard) => dashboard,
        Err(error)
            if matches!(
                error.code,
                "source_unavailable"
                    | "source_network"
                    | "source_access_denied"
                    | "source_request_rejected"
                    | "source_timeout"
                    | "source_limited"
                    | "invalid_data"
                    | "empty_response"
            ) =>
        {
            return project_dashboard(address_id, day_offset, state).or(Err(error));
        }
        Err(error) => return Err(error),
    };
    state
        .cache
        .lock()
        .map_err(|_| CommandError {
            code: "internal_error",
        })?
        .record(&address_id, dashboard.snapshot);
    project_dashboard(address_id, day_offset, state)
}
#[tauri::command]
fn get_archive(
    key: Option<String>,
    date: Option<NaiveDate>,
    state: tauri::State<'_, AppState>,
) -> Result<ArchiveView, CommandError> {
    let now = state.now()?;
    let mut repository = state.repo()?;
    let preferred = key.or_else(|| {
        repository
            .book
            .state()
            .addresses
            .iter()
            .find(|entry| Some(&entry.id) == repository.book.state().active_address_id.as_ref())
            .map(address_key)
    });
    Ok(repository
        .store
        .archive_view(preferred.as_deref(), date, now)?)
}
/// Android's entry point returns no Result, so a startup error is handled here
/// rather than discarded by the generated JNI wrapper.
#[cfg(mobile)]
#[tauri::mobile_entry_point]
fn mobile_main() {
    if let Err(error) = run() {
        eprintln!("PowerWatch startup failed: {error}");
    }
}
/// Start the local shell with live sources and the actual UTC clock.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));
    let builder = builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let keep = window
                    .state::<AppState>()
                    .repo()
                    .ok()
                    .and_then(|repository| {
                        repository
                            .store
                            .setting::<background::PlatformSettings>("platform_settings")
                            .ok()
                            .flatten()
                    })
                    .unwrap_or_default()
                    .tray;
                if keep {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        });
    let builder = builder.setup(|app| {
        let directory = app.path().app_data_dir()?;
        std::fs::create_dir_all(&directory)?;
        let store = Store::open(directory.join("powerwatch-development.sqlite3"))?;
        powerwatch_runtime::configure_http_policy(
            &directory.join("powerwatch-development.sqlite3"),
        )?;
        let book = store.load_book()?;
        let mut cache = DashboardCache::default();
        for saved in &book.state().addresses {
            if let Some(mut snapshot) = store
                .cached(&address_key(saved))?
                .filter(|snapshot| !snapshot.is_demo)
            {
                snapshot.address.label = saved.address.label.clone();
                cache.record(&saved.id, snapshot);
            }
        }
        app.manage(AppState {
            catalog_directory: directory.join("address-catalog"),
            repository: Mutex::new(Repository { store, book }),
            cache: Mutex::new(cache),
        });
        app.manage(updates::UpdateState::default());
        #[cfg(windows)]
        desktop::setup(app)?;
        #[cfg(target_os = "android")]
        android::configure(
            app.handle(),
            &directory.join("powerwatch-development.sqlite3"),
        )?;
        Ok(())
    });
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_app_info,
        set_automatic_updates,
        open_app_link,
        updates::check_updates,
        updates::install_update,
        get_background_status,
        open_background_settings,
        open_exact_reminder_settings,
        clear_notification_journal,
        set_notification_preferences,
        set_address_options,
        set_platform_settings,
        test_notification,
        get_session,
        get_view_preferences,
        set_view_preferences,
        get_archive,
        set_monitoring,
        get_runtime_info,
        project_dashboard,
        get_oblasts,
        get_operators,
        get_address_lookup_support,
        get_operator_groups,
        get_local_settlements,
        get_country_catalog,
        save_manual_address,
        search_address_catalog,
        preview_address_group,
        resolve_address_group,
        activate_address,
        delete_address,
        complete_onboarding,
        get_dashboard
    ]);
    builder.run(tauri::generate_context!())?;
    Ok(())
}
