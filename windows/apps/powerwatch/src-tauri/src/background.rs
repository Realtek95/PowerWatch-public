//! OS-neutral commands; platform adapters submit the durable Rust outbox.
use super::*;
use chrono::TimeDelta;
use powerwatch_notifications::{NotificationKind, NotificationPreferences, PlannedNotification};
use powerwatch_storage::{NotificationRecord, PollState};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundStatus {
    notifications_allowed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reminder_status: Option<ReminderStatus>,
    preferences: NotificationPreferences,
    notifications: Vec<NotificationRecord>,
    polls: Vec<PollState>,
    platform: &'static str,
    platform_settings: PlatformSettings,
    last_error: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderStatus {
    exact_allowed: bool,
    exact_access_required: bool,
    battery_saver: bool,
    background_restricted: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlatformSettings {
    pub tray: bool,
    pub autostart: bool,
}
impl Default for PlatformSettings {
    fn default() -> Self {
        Self {
            tray: true,
            autostart: false,
        }
    }
}
#[tauri::command]
pub fn clear_notification_journal(
    id: Option<i64>,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    state.repo()?.store.clear_notification_journal(id)?;
    Ok(())
}
#[tauri::command(async)]
pub fn open_background_settings(app: tauri::AppHandle) -> Result<(), CommandError> {
    #[cfg(target_os = "android")]
    return android::open_background_settings(&app);
    {
        let _ = app;
        Err(CommandError {
            code: "platform_error",
        })
    }
}
#[tauri::command(async)]
pub fn open_exact_reminder_settings(app: tauri::AppHandle) -> Result<(), CommandError> {
    #[cfg(target_os = "android")]
    return android::open_exact_reminder_settings(&app);
    {
        let _ = app;
        Err(CommandError {
            code: "platform_error",
        })
    }
}
#[tauri::command(async)]
pub fn get_background_status(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<BackgroundStatus, CommandError> {
    let reminder_status = None;
    let allowed = {
        let _ = app;
        None
    };
    let repository = state.repo()?;
    Ok(BackgroundStatus {
        notifications_allowed: allowed,
        reminder_status,
        preferences: repository.store.notification_preferences()?,
        notifications: repository.store.notification_log()?,
        polls: repository.store.poll_states()?,
        platform: "windows",
        platform_settings: repository
            .store
            .setting("platform_settings")?
            .unwrap_or_default(),
        last_error: repository.store.setting("background_error")?,
    })
}
#[tauri::command]
pub fn set_notification_preferences(
    preferences: NotificationPreferences,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    if !preferences.is_valid() {
        return Err(CommandError {
            code: "invalid_selection",
        });
    }
    let now = state.now()?;
    let mut repository = state.repo()?;
    repository
        .store
        .set_setting("notification_preferences", &preferences)?;
    let session = repository.book.state();
    for saved in &session.addresses {
        let key = address_key(saved);
        let mut prefs = preferences.clone();
        prefs.enabled &= saved.notifications_enabled;
        if let Some(snapshot) = repository.store.cached(&key)? {
            let plans = powerwatch_notifications::plan_snapshot(
                &key,
                &saved.id,
                &snapshot,
                None,
                now,
                session.check_interval_minutes,
                &prefs,
            )
            .map_err(|_| CommandError {
                code: "invalid_data",
            })?;
            repository
                .store
                .replace_notification_plans(&key, &plans, now)?;
        }
    }
    repository.store.cancel_invalid_notification_plans(now)?;
    Ok(())
}
#[tauri::command]
pub fn set_address_options(
    id: String,
    monitoring: bool,
    notifications: bool,
    state: tauri::State<'_, AppState>,
) -> Result<SessionState, CommandError> {
    let now = state.now()?;
    let mut repository = state.repo()?;
    let value = repository.update(
        |book| book.set_address_options(&id, monitoring, notifications),
        None,
    )?;
    repository.store.cancel_invalid_notification_plans(now)?;
    Ok(value)
}
#[tauri::command]
pub fn set_platform_settings(
    settings: PlatformSettings,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    {
        use tauri_plugin_autostart::ManagerExt;
        let manager = app.autolaunch();
        let result = if settings.autostart {
            manager.enable()
        } else {
            manager.disable()
        };
        result.map_err(|_| CommandError {
            code: "platform_error",
        })?;
        if let Some(tray) = app.tray_by_id("powerwatch") {
            tray.set_visible(settings.tray).map_err(|_| CommandError {
                code: "platform_error",
            })?;
        }
    }
    state
        .repo()?
        .store
        .set_setting("platform_settings", &settings)?;
    Ok(())
}
#[tauri::command(async)]
pub fn test_notification(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), CommandError> {
    let now = state.now()?;
    let mut repository = state.repo()?;
    let session = repository.book.state();
    let saved = session.addresses.first().ok_or(CommandError {
        code: "address_not_found",
    })?;
    let prefs = repository.store.notification_preferences()?;
    repository.store.enqueue_notification(
        &PlannedNotification {
            key: format!("test:{}", Utc::now().timestamp_millis()),
            address_key: address_key(saved),
            local_address_id: saved.id.clone(),
            kind: NotificationKind::Test,
            at: now,
            expires_at: now + TimeDelta::minutes(2),
            title: powerwatch_notifications::text::TEST_TITLE.into(),
            body: powerwatch_notifications::text::TEST.into(),
            sound: prefs.sound,
            vibration: prefs.vibration,
            boundary: None,
        },
        now,
    )?;
    drop(repository);
    #[cfg(target_os = "android")]
    android::flush_notifications(&app)?;
    let _ = app;
    Ok(())
}
