//! Schema checked against the public YASNO response on 12.11.2026.
use super::{SourceSchedule, evidence};
use crate::http::FetchError;
use chrono::{DateTime, Days, TimeDelta, Utc};
use powerwatch_core::{
    grid::{ProviderKind, Queue},
    model::{ScheduleInterval, ScheduleState, normalize_day},
    time::{LOCAL_ZONE, local_instant},
};
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Deserialize)]
pub struct Day {
    slots: Vec<Slot>,
    date: DateTime<chrono::FixedOffset>,
    status: String,
}
#[derive(Deserialize)]
struct Slot {
    start: u16,
    end: u16,
    #[serde(rename = "type")]
    kind: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    today: Day,
    tomorrow: Day,
    updated_on: DateTime<Utc>,
}
pub type Response = BTreeMap<String, Group>;
pub fn parse(
    data: &Response,
    queue: &Queue,
    now: DateTime<Utc>,
) -> Result<SourceSchedule, FetchError> {
    let group = data.get(&queue.code).ok_or(FetchError::InvalidData)?;
    if group.updated_on > now || now - group.updated_on > TimeDelta::days(2) {
        return Err(FetchError::InvalidData);
    }
    let mut emergency_dates = Vec::new();
    let mut days = Vec::new();
    for day in [&group.today, &group.tomorrow] {
        let date = day.date.with_timezone(&LOCAL_ZONE).date_naive();
        if day.date.date_naive() != date {
            return Err(FetchError::InvalidData);
        }
        let applied = match day.status.as_str() {
            "ScheduleApplies" | "NoOutages" => true,
            "WaitingForSchedule" => false,
            "EmergencyShutdowns" => {
                emergency_dates.push(date);
                day.slots.iter().any(|slot| slot.kind == "Definite")
            }
            _ => return Err(FetchError::InvalidData),
        };
        let mut intervals = Vec::new();
        for slot in &day.slots {
            if day.status == "NoOutages" && slot.kind != "NotPlanned" {
                return Err(FetchError::InvalidData);
            }
            let state = if !applied {
                ScheduleState::Unknown
            } else {
                match slot.kind.as_str() {
                    "Definite" => ScheduleState::Outage,
                    "NotPlanned" => ScheduleState::Available,
                    _ => ScheduleState::Unknown,
                }
            };
            intervals.push(ScheduleInterval {
                start: local_instant(date, slot.start).map_err(|_| FetchError::InvalidData)?,
                end: local_instant(date, slot.end).map_err(|_| FetchError::InvalidData)?,
                state,
            });
        }
        if applied && day.status == "NoOutages" && intervals.is_empty() {
            intervals.push(ScheduleInterval {
                start: local_instant(date, 0).map_err(|_| FetchError::InvalidData)?,
                end: local_instant(date, 1440).map_err(|_| FetchError::InvalidData)?,
                state: ScheduleState::Available,
            });
        }
        days.push(normalize_day(date, &intervals).map_err(|_| FetchError::InvalidData)?);
    }
    let today = now.with_timezone(&LOCAL_ZONE).date_naive();
    if days[0].date != today || Some(days[1].date) != today.checked_add_days(Days::new(1)) {
        return Err(FetchError::InvalidData);
    }
    let mut proof = evidence(
        "yasno",
        "YASNO",
        ProviderKind::Operator,
        "https://yasno.com.ua/",
        queue,
        now,
        Some(group.updated_on),
    );
    proof.valid_until =
        Some(local_instant(days[1].date, 1440).map_err(|_| FetchError::InvalidData)?);
    Ok(SourceSchedule {
        retained: false,
        days,
        evidence: proof,
        emergency_dates,
    })
}
