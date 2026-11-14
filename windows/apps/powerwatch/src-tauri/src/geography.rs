//! Geographic records and operator records deliberately use different IDs.
use crate::{AppState, CommandError};
use powerwatch_addresses::{Catalog, CatalogError, CatalogManifest};
use serde::{Deserialize, Serialize};
use std::path::Path;
include!(concat!(env!("OUT_DIR"), "/catalog_packages.rs"));
impl From<CatalogError> for CommandError {
    fn from(error: CatalogError) -> Self {
        Self {
            code: if matches!(error, CatalogError::InvalidSelection) {
                "invalid_selection"
            } else {
                "catalog_unavailable"
            },
        }
    }
}
pub fn open(directory: &Path, oblast: &str) -> Result<Catalog, CommandError> {
    static INSTALL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = INSTALL.lock().map_err(|_| CommandError {
        code: "catalog_unavailable",
    })?;
    let (_, bytes, json) =
        PACKAGES
            .iter()
            .find(|(id, _, _)| *id == oblast)
            .ok_or(CommandError {
                code: "invalid_selection",
            })?;
    let manifest: CatalogManifest = serde_json::from_str(json).map_err(|_| CommandError {
        code: "catalog_unavailable",
    })?;
    let path = powerwatch_addresses::install(directory, *bytes, &manifest)?;
    Ok(Catalog::open(&path, &manifest.sha256)?)
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    District,
    Community,
    Settlement,
    Street,
    House,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryQuery {
    pub oblast_id: String,
    pub district_id: Option<String>,
    pub community_id: Option<String>,
    pub settlement_id: Option<String>,
    pub street_id: Option<String>,
    pub query: String,
    pub kind: CatalogKind,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryResult {
    version: String,
    items: Vec<CountryEntry>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryEntry {
    id: String,
    name: String,
    number: Option<String>,
    building: Option<String>,
    postcode: Option<String>,
}
impl From<powerwatch_core::model::CatalogEntry> for CountryEntry {
    fn from(e: powerwatch_core::model::CatalogEntry) -> Self {
        Self {
            id: e.id,
            name: e.name,
            number: None,
            building: None,
            postcode: None,
        }
    }
}
#[tauri::command]
pub async fn get_country_catalog(
    request: CountryQuery,
    state: tauri::State<'_, AppState>,
) -> Result<CountryResult, CommandError> {
    let directory = state.catalog_directory.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let catalog = open(&directory, &request.oblast_id)?;
        let items = match request.kind {
            CatalogKind::District => catalog
                .districts(&request.oblast_id)?
                .into_iter()
                .map(Into::into)
                .collect(),
            CatalogKind::Community => catalog
                .communities(&request.oblast_id, request.district_id.as_deref())?
                .into_iter()
                .map(Into::into)
                .collect(),
            CatalogKind::Settlement => catalog
                .settlements(
                    &request.oblast_id,
                    request.district_id.as_deref(),
                    request.community_id.as_deref(),
                )?
                .into_iter()
                .map(Into::into)
                .collect(),
            CatalogKind::Street => catalog
                .streets(
                    request.settlement_id.as_deref().ok_or(CommandError {
                        code: "invalid_selection",
                    })?,
                    &request.query,
                )?
                .into_iter()
                .map(Into::into)
                .collect(),
            CatalogKind::House => catalog
                .houses(
                    request.settlement_id.as_deref().ok_or(CommandError {
                        code: "invalid_selection",
                    })?,
                    request.street_id.as_deref().ok_or(CommandError {
                        code: "invalid_selection",
                    })?,
                    &request.query,
                )?
                .into_iter()
                .map(|e| CountryEntry {
                    id: e.id,
                    name: e.name,
                    number: Some(e.number),
                    building: Some(e.building),
                    postcode: e.postcode,
                })
                .collect(),
        };
        Ok(CountryResult {
            version: catalog.version().into(),
            items,
        })
    })
    .await
    .map_err(|_| CommandError {
        code: "internal_error",
    })?
}
