//! Windows lifetime: closing the window keeps the local worker alive when the
//! tray is enabled; only the explicit Exit action ends the process.
use super::*;
use tauri::{
    Emitter,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_notification::NotificationExt;
fn open(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
pub fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", text::OPEN, true, None::<&str>)?,
            &MenuItem::with_id(app, "refresh", text::REFRESH, true, None::<&str>)?,
            &MenuItem::with_id(app, "settings", text::SETTINGS, true, None::<&str>)?,
            &MenuItem::with_id(app, "exit", text::EXIT, true, None::<&str>)?,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id("powerwatch")
        .menu(&menu)
        .tooltip(text::DEFAULT_TOOLTIP)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => open(app),
            "settings" => {
                open(app);
                let _ = app.emit("open-settings", ());
            }
            "refresh" => {
                open(app);
                let _ = app.emit("refresh-requested", ());
            }
            "exit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                open(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    let state = app.state::<AppState>();
    let settings: background::PlatformSettings = state
        .repo()
        .map_err(|_| "repository lock")?
        .store
        .setting("platform_settings")?
        .unwrap_or_default();
    tray.set_visible(settings.tray)?;
    let database_path = state
        .repo()
        .map_err(|_| "repository lock")?
        .store
        .path()
        .ok_or("database path")?;
    let poll_handle = app.handle().clone();
    std::thread::spawn(move || {
        let mut store = None;
        loop {
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                if store.is_none() {
                    store = Some(powerwatch_storage::Store::open(&database_path)?);
                }
                powerwatch_runtime::tick(store.as_mut().ok_or("worker database")?, Utc::now())?;
                Ok(())
            })();
            if result.is_err()
                && let Ok(repository) = poll_handle.state::<AppState>().repo()
            {
                let _ = repository
                    .store
                    .set_setting("background_error", &"platform_error");
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        loop {
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                let state = handle.state::<AppState>();
                let now = state.now().map_err(|_| "clock")?;
                let permission = matches!(
                    handle.notification().permission_state()?,
                    tauri_plugin_notification::PermissionState::Granted
                );
                let (claimed, tooltip) = {
                    let mut repository = state.repo().map_err(|_| "repository lock")?;
                    let mut claimed = Vec::new();
                    for pending in repository.store.pending_notifications()? {
                        if let Some(record) = repository.store.claim_notification(
                            &pending.plan.key,
                            now,
                            permission,
                        )? {
                            claimed.push(record);
                        }
                    }
                    let session = repository.book.state();
                    let mut tooltip = text::DEFAULT_TOOLTIP.to_string();
                    if let Some(saved) = session
                        .addresses
                        .iter()
                        .find(|saved| Some(&saved.id) == session.active_address_id.as_ref())
                        && let Some(snapshot) = repository.store.cached(&address_key(saved))?
                        && let Ok(dashboard) = powerwatch_core::dashboard::build_dashboard(
                            snapshot,
                            now.with_timezone(&powerwatch_core::time::LOCAL_ZONE)
                                .date_naive(),
                            now,
                        )
                    {
                        let status = match dashboard.current_state {
                            powerwatch_core::model::ScheduleState::Available => {
                                text::FORECAST_AVAILABLE
                            }
                            powerwatch_core::model::ScheduleState::Outage => text::FORECAST_OUTAGE,
                            _ => text::FORECAST_UNKNOWN,
                        };
                        tooltip = text::tooltip(
                            &saved.address.label,
                            status,
                            &dashboard,
                            session.check_interval_minutes,
                        );
                    }
                    (claimed, tooltip)
                };
                for record in claimed {
                    let mut notification = handle
                        .notification()
                        .builder()
                        .title(&record.plan.title)
                        .body(&record.plan.body);
                    if !record.plan.sound {
                        notification = notification.silent();
                    }
                    if notification.show().is_err() {
                        state
                            .repo()
                            .map_err(|_| "repository lock")?
                            .store
                            .set_notification_state(&record.plan.key, "failed", now)?;
                    }
                }
                if let Some(tray) = handle.tray_by_id("powerwatch") {
                    tray.set_tooltip(Some(tooltip))?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("background: {error}");
                if let Ok(repository) = handle.state::<AppState>().repo() {
                    let _ = repository
                        .store
                        .set_setting("background_error", &"platform_error");
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    Ok(())
}
