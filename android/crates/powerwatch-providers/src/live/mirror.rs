//! DTEK mirror's date-specific fact data only. Never repeat its weekly preset.
use super::dtek_fact::Fact;
use super::{SourceSchedule, evidence};
use crate::http::FetchError;
use chrono::{DateTime, TimeDelta, Utc};
use powerwatch_core::grid::{ProviderKind, Queue};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    region_id: String,
    last_updated: DateTime<Utc>,
    last_update_status: UpdateStatus,
    meta: Meta,
    fact: Fact,
}
#[derive(Deserialize)]
struct UpdateStatus {
    ok: bool,
    status: String,
    at: DateTime<Utc>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    schema_version: String,
    content_hash: String,
}
pub fn parse(
    data: &Response,
    region: &str,
    queue: &Queue,
    now: DateTime<Utc>,
) -> Result<SourceSchedule, FetchError> {
    if data.region_id != region
        || !data.last_update_status.ok
        || data.last_update_status.status != "parsed"
        || data.meta.schema_version.split('.').next() != Some("1")
        || data.meta.content_hash.len() != 64
        || !data
            .meta
            .content_hash
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
        || data.last_updated > now
        || data.last_update_status.at > now
        || now - data.last_updated > TimeDelta::minutes(45)
        || now - data.last_update_status.at > TimeDelta::minutes(45)
    {
        return Err(FetchError::InvalidData);
    }
    let (days, publication, found) = super::dtek_fact::parse(&data.fact, queue, now)?;
    if !found {
        return Err(FetchError::InvalidData);
    }
    let mut proof = evidence(
        "outage-data-ua",
        "outage-data-ua",
        ProviderKind::Mirror,
        "https://github.com/Baskerville42/outage-data-ua",
        queue,
        now,
        Some(publication),
    );
    proof.content_hash = Some(data.meta.content_hash.clone());
    proof.valid_until = Some(data.last_updated + TimeDelta::minutes(45));
    Ok(SourceSchedule {
        retained: false,
        days,
        evidence: proof,
        emergency_dates: Vec::new(),
    })
}
