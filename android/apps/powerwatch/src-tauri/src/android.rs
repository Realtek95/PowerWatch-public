//! Thin Android bridge. Every headless callback opens the same local repository;
//! domain decisions and deduplication stay in Rust, never in the WebView.
use super::*;
use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{jlong, jstring},
};
use serde_json::{Value, json};
use tauri::Emitter;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
struct AndroidHandle(PluginHandle<tauri::Wry>);
pub fn init() -> TauriPlugin<tauri::Wry> {
    Builder::new("powerwatch-background")
        .setup(|app, api| {
            app.manage(AndroidHandle(api.register_android_plugin(
                "ua.powerwatch.app",
                "PowerWatchPlugin",
            )?));
            Ok(())
        })
        .build()
}
pub fn configure(
    app: &tauri::AppHandle,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.clone();
    let on_open = tauri::ipc::Channel::<Value>::new(move |body| {
        let event = match body {
            tauri::ipc::InvokeResponseBody::Json(json) => {
                let value = serde_json::from_str::<Value>(&json).unwrap_or(Value::Null);
                if value["notificationPermissionChanged"].as_bool() == Some(true) {
                    "notification-permission-changed"
                } else if value["settings"].as_bool() == Some(true) {
                    "open-settings"
                } else {
                    "open-dashboard"
                }
            }
            _ => "open-dashboard",
        };
        handle.emit(event, ())?;
        Ok(())
    });
    app.state::<AndroidHandle>().0.run_mobile_plugin::<Value>(
        "configure",
        json!({ "databasePath" : path.to_string_lossy(), "onOpen" : on_open }),
    )?;
    Ok(())
}
pub fn starts_in_settings(app: &tauri::AppHandle) -> Result<bool, CommandError> {
    let value: Value = app
        .state::<AndroidHandle>()
        .0
        .run_mobile_plugin("launchAction", json!({}))
        .map_err(|_| CommandError {
            code: "platform_error",
        })?;
    Ok(value["settings"].as_bool().unwrap_or(false))
}
pub fn notification_state(app: &tauri::AppHandle) -> Result<Option<bool>, CommandError> {
    let value: Value = app
        .state::<AndroidHandle>()
        .0
        .run_mobile_plugin("notificationStatus", json!({}))
        .map_err(|_| CommandError {
            code: "platform_error",
        })?;
    Ok(if value["pending"].as_bool() == Some(true) {
        None
    } else {
        Some(value["allowed"].as_bool().unwrap_or(false))
    })
}
pub fn notification_allowed(app: &tauri::AppHandle) -> Result<bool, CommandError> {
    Ok(notification_state(app)?.unwrap_or(false))
}
pub fn reminder_status(app: &tauri::AppHandle) -> Result<ReminderStatus, CommandError> {
    app.state::<AndroidHandle>()
        .0
        .run_mobile_plugin("reminderStatus", json!({}))
        .map_err(|_| CommandError {
            code: "platform_error",
        })
}
pub fn open_exact_reminder_settings(app: &tauri::AppHandle) -> Result<(), CommandError> {
    app.state::<AndroidHandle>()
        .0
        .run_mobile_plugin::<Value>("requestExactReminders", json!({}))
        .map_err(|_| CommandError {
            code: "platform_error",
        })?;
    Ok(())
}
pub fn open_background_settings(app: &tauri::AppHandle) -> Result<(), CommandError> {
    app.state::<AndroidHandle>()
        .0
        .run_mobile_plugin::<Value>("openBackgroundSettings", json!({}))
        .map_err(|_| CommandError {
            code: "platform_error",
        })?;
    Ok(())
}
pub fn flush_notifications(app: &tauri::AppHandle) -> Result<(), CommandError> {
    app.state::<AndroidHandle>()
        .0
        .run_mobile_plugin::<Value>("flushNotifications", json!({}))
        .map_err(|_| CommandError {
            code: "platform_error",
        })?;
    Ok(())
}
pub fn update_info(app: &tauri::AppHandle) -> Result<Value, CommandError> {
    app.state::<AndroidHandle>()
        .0
        .run_mobile_plugin("updateInfo", json!({}))
        .map_err(|_| CommandError {
            code: "update_invalid",
        })
}
pub fn install_update(
    app: &tauri::AppHandle,
    path: &std::path::Path,
    version_code: u64,
    sha256: &str,
) -> Result<&'static str, CommandError> {
    let result: Value = app
        .state::<AndroidHandle>()
        .0
        .run_mobile_plugin(
            "installUpdate",
            json!(
                { "path" : path.to_string_lossy(), "versionCode" : version_code, "sha256"
                : sha256 }
            ),
        )
        .map_err(|_| CommandError {
            code: "update_signature",
        })?;
    match result["status"].as_str() {
        Some("permission_required") => Ok("permission_required"),
        Some("installer_opened") => Ok("installer_opened"),
        _ => Err(CommandError {
            code: "update_signature",
        }),
    }
}
fn execute(
    path: &str,
    operation: &str,
    payload: &str,
    wall: i64,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut store = Store::open(path)?;
    powerwatch_runtime::configure_http_policy(std::path::Path::new(path))?;
    let wall = DateTime::from_timestamp_millis(wall).ok_or("invalid wall time")?;
    let now = wall;
    let args: Value = serde_json::from_str(payload)?;
    Ok(match operation {
        "maintenance" => {
            let _ = store.maintain(now);
            if let Some(directory) = args["cacheDirectory"].as_str() {
                let _ = powerwatch_updates::cache::prune_installers(
                    &std::path::Path::new(directory).join("updates"),
                    env!("CARGO_PKG_VERSION"),
                    None,
                    std::time::SystemTime::now(),
                );
            }
            Value::Null
        }
        "request-refresh" => {
            let book = store.load_book()?.state();
            if let Some(id) = book.active_address_id {
                store.request_manual_refresh(&id)?;
            }
            json!(true)
        }
        "manual-pending" => {
            json!(
                store
                    .setting::<String>("manual_refresh_address")?
                    .is_some_and(|id| !id.is_empty())
            )
        }
        "widget" => serde_json::to_value(powerwatch_runtime::widget::project(&store, now)?)?,
        "inspect" => {
            json!(
                { "polls" : store.poll_states() ?, "notifications" : store
                .notification_log() ?, "pending" : store.pending_notifications() ? }
            )
        }
        "tick" => serde_json::to_value(powerwatch_runtime::tick(&mut store, wall)?)?,
        "pending" => {
            store.cancel_invalid_notification_plans(now)?;
            json!({ "now" : now, "notifications" : store.pending_notifications() ? })
        }
        "claim" => serde_json::to_value(store.claim_notification(
            args["key"].as_str().ok_or("missing key")?,
            now,
            args["permission"].as_bool().unwrap_or(false),
        )?)?,
        "ack" => {
            store.set_notification_state(
                args["key"].as_str().ok_or("missing key")?,
                "failed",
                now,
            )?;
            Value::Null
        }
        _ => return Err("unknown native operation".into()),
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_ua_powerwatch_app_BackgroundBridge_nativeCall(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    path: JString<'_>,
    operation: JString<'_>,
    payload: JString<'_>,
    wall: jlong,
) -> jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<Value, Box<dyn std::error::Error>> {
            let path: String = env.get_string(&path)?.into();
            let operation: String = env.get_string(&operation)?.into();
            let payload: String = env.get_string(&payload)?.into();
            execute(&path, &operation, &payload, wall)
        },
    ));
    let output = match result {
        Ok(Ok(value)) => json!({ "ok" : true, "value" : value }),
        _ => json!({ "ok" : false, "code" : "storage_error" }),
    };
    env.new_string(output.to_string())
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
