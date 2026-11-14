//! DTEK mirror's date-specific fact data only. Never repeat its weekly preset.
use super::{SourceSchedule, evidence};
use crate::http::FetchError;
use chrono::{DateTime, Days, NaiveDateTime, TimeDelta, TimeZone, Utc};
use powerwatch_core::{
    grid::{ProviderKind, Queue},
    model::{ScheduleInterval, ScheduleState, normalize_day},
    time::{LOCAL_ZONE, day_bounds, local_instant},
};
use serde::Deserialize;
use std::collections::BTreeMap;
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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fact {
    data: BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>,
    update: Option<String>,
    update_fact: Option<String>,
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
    let publication = data
        .fact
        .update
        .as_ref()
        .or(data.fact.update_fact.as_ref())
        .and_then(|value| NaiveDateTime::parse_from_str(value, "%d.%m.%Y %H:%M").ok())
        .and_then(|value| LOCAL_ZONE.from_local_datetime(&value).single())
        .map(|value| value.with_timezone(&Utc))
        .ok_or(FetchError::InvalidData)?;
    if publication > now || now - publication > TimeDelta::days(2) {
        return Err(FetchError::InvalidData);
    }
    let today = now.with_timezone(&LOCAL_ZONE).date_naive();
    let tomorrow = today
        .checked_add_days(Days::new(1))
        .ok_or(FetchError::InvalidData)?;
    let mut days = Vec::new();
    let mut found = false;
    for date in [today, tomorrow] {
        let (start, _) = day_bounds(date).map_err(|_| FetchError::InvalidData)?;
        let schedule = data
            .fact
            .data
            .get(&start.timestamp().to_string())
            .and_then(|groups| groups.get(&format!("GPV{}", queue.code)));
        let mut intervals = Vec::new();
        if let Some(hours) = schedule {
            found = true;
            for (hour, value) in hours {
                let hour: u16 = hour.parse().map_err(|_| FetchError::InvalidData)?;
                if !(1..=24).contains(&hour) {
                    return Err(FetchError::InvalidData);
                }
                let minute = (hour - 1) * 60;
                use ScheduleState::{Available, Outage, Switching, Unknown};
                let halves = match value.as_str() {
                    "yes" => [Available, Available],
                    "no" => [Outage, Outage],
                    "first" => [Outage, Available],
                    "second" => [Available, Outage],
                    "mfirst" => [Switching, Available],
                    "msecond" => [Available, Switching],
                    "maybe" => [Switching, Switching],
                    _ => [Unknown, Unknown],
                };
                for (offset, state) in halves.into_iter().enumerate() {
                    let minute = minute + offset as u16 * 30;
                    intervals.push(ScheduleInterval {
                        start: local_instant(date, minute).map_err(|_| FetchError::InvalidData)?,
                        end: local_instant(date, minute + 30)
                            .map_err(|_| FetchError::InvalidData)?,
                        state,
                    });
                }
            }
        }
        days.push(normalize_day(date, &intervals).map_err(|_| FetchError::InvalidData)?);
    }
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
