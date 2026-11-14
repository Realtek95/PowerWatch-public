//! Public application facts and fixed external links, with no arbitrary URL IPC.
use crate::{AppState, CommandError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri_plugin_opener::OpenerExt;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppConfig {
    author: String,
    project_url: Option<String>,
}
fn config() -> Result<AppConfig, CommandError> {
    serde_json::from_str(include_str!("../../../../assets/app-config.json")).map_err(|_| {
        CommandError {
            code: "internal_error",
        }
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    author: String,
    version: &'static str,
    build_number: String,
    platform: &'static str,
    project_url: Option<String>,
    updates_configured: bool,
    automatic_updates: bool,
    last_update_check: Option<DateTime<Utc>>,
    source_checks: Vec<powerwatch_providers::live::service::SourceCheck>,
}
#[tauri::command]
pub fn get_app_info(state: tauri::State<'_, AppState>) -> Result<AppInfo, CommandError> {
    let config = config()?;
    let repository = state.repo()?;
    let stamp = env!("POWERWATCH_BUILD_NUMBER")
        .parse()
        .map_err(|_| CommandError {
            code: "internal_error",
        })?;
    let build = DateTime::from_timestamp(stamp, 0).ok_or(CommandError {
        code: "internal_error",
    })?;
    Ok(AppInfo {
        name: "PowerWatch",
        author: config.author,
        version: env!("CARGO_PKG_VERSION"),
        build_number: build.format("%Y%m%d.%H%M").to_string(),
        platform: "android",
        project_url: config.project_url,
        updates_configured: crate::updates::config()?.configured(),
        automatic_updates: repository
            .store
            .setting("automatic_updates")?
            .unwrap_or(true),
        last_update_check: repository.store.setting("last_update_check")?,
        source_checks: repository.store.settings_with_prefix("source_check:")?,
    })
}
#[tauri::command]
pub fn set_automatic_updates(
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    if enabled && !crate::updates::config()?.configured() {
        return Err(CommandError {
            code: "update_unavailable",
        });
    }
    state
        .repo()?
        .store
        .set_setting("automatic_updates", &enabled)?;
    Ok(())
}
fn fixed_url(id: &str, project: Option<&str>) -> Option<String> {
    Some(
        match id {
            "project" => {
                return project
                    .filter(|url| url.starts_with("https://github.com/"))
                    .map(str::to_owned);
            }
            "yasno" => "https://static.yasno.ua/",
            "dtek-dnem" => "https://www.dtek-dnem.com.ua/ua/shutdowns",
            "svitlopulse" => "https://svitlopulse.andrewboyko.name/sources/",
            "outage-data-ua" => "https://github.com/Baskerville42/outage-data-ua",
            "nais" => {
                "https://nais.gov.ua/m/slovnik-administrativno-teritorialnogo-ustroyu-ukraini-slovnik-vulits-naselenih-punktiv-ta-vulits-imenovanih-obektiv"
            }
            "drv" => "https://www.drv.gov.ua/ords/svc/r/personal/cab/opendata",
            "katottg" => {
                "https://mininfra.gov.ua/diialnist/rozvytok-mistsevoho-samovriaduvannia/kodyfikator-administratyvno-terytorialnykh-odynyts-ta-terytorii-terytorialnykh-hromad"
            }
            "osm" => "https://www.openstreetmap.org/copyright",
            _ => return None,
        }
            .to_owned(),
    )
}
#[tauri::command]
pub fn open_app_link(id: String, app: tauri::AppHandle) -> Result<(), CommandError> {
    let settings = config()?;
    let url = fixed_url(&id, settings.project_url.as_deref()).ok_or(CommandError {
        code: "link_unavailable",
    })?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| CommandError {
            code: "link_unavailable",
        })
}
