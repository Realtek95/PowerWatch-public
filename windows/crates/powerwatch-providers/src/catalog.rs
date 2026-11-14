//! Local official territory catalog. A territory ID is never a source binding.
use powerwatch_core::{grid::Oblast, model::CatalogEntry};
use serde::Deserialize;
use std::sync::OnceLock;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalSettlement {
    id: String,
    oblast_id: String,
    name: String,
    context: String,
}
#[derive(Deserialize)]
struct Catalog {
    oblasts: Vec<Oblast>,
    settlements: Vec<LocalSettlement>,
}
fn catalog() -> &'static Catalog {
    static DATA: OnceLock<Catalog> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/data/katottg.json"))
            .expect("validated bundled KATOTTG catalog")
    })
}
pub fn oblasts() -> Vec<CatalogEntry> {
    catalog()
        .oblasts
        .iter()
        .map(|item| CatalogEntry {
            id: item.id.clone(),
            name: item.name.clone(),
        })
        .collect()
}
pub fn oblast(id: &str) -> Option<Oblast> {
    catalog().oblasts.iter().find(|item| item.id == id).cloned()
}
pub fn settlements(oblast_id: &str) -> Vec<CatalogEntry> {
    catalog()
        .settlements
        .iter()
        .filter(|item| item.oblast_id == oblast_id)
        .map(|item| CatalogEntry {
            id: item.id.clone(),
            name: if item.context.is_empty() {
                item.name.clone()
            } else {
                format!("{} · {}", item.name, item.context)
            },
        })
        .collect()
}
/// Only exact parent/child catalog relationships may enrich a saved address.
pub fn resolve(oblast_id: &str, settlement_id: &str) -> Option<(Oblast, String)> {
    let oblast = catalog().oblasts.iter().find(|item| item.id == oblast_id)?;
    let settlement = catalog()
        .settlements
        .iter()
        .find(|item| item.id == settlement_id && item.oblast_id == oblast_id)?;
    Some((oblast.clone(), settlement.name.clone()))
}
/// A source exposing names without community IDs can only match a unique name
/// inside its oblast. Homonymous villages must remain unconfirmed.
pub fn unique_settlement_name(oblast_id: &str, settlement_id: &str) -> Option<String> {
    let (_, name) = resolve(oblast_id, settlement_id)?;
    (catalog()
        .settlements
        .iter()
        .filter(|item| item.oblast_id == oblast_id && item.name == name)
        .count()
        == 1)
        .then_some(name)
}
