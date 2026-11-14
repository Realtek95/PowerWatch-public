//! Explicit provider-ID mappings; never infer an operator from a queue number.
use powerwatch_core::grid::GridOperator;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
#[derive(Clone, Deserialize, Serialize)]
pub struct YasnoRegion {
    pub region: u16,
    pub dso: u16,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatorRecord {
    pub operator: GridOperator,
    pub svitlopulse_id: String,
    pub oblast_slug: String,
    pub yasno: Option<YasnoRegion>,
    pub mirror_region: Option<String>,
}
pub fn all() -> &'static [OperatorRecord] {
    static DATA: OnceLock<Vec<OperatorRecord>> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/data/operators.json"))
            .expect("validated operator registry")
    })
}
pub fn find(id: &str) -> Option<&'static OperatorRecord> {
    all().iter().find(|item| item.operator.id == id)
}
