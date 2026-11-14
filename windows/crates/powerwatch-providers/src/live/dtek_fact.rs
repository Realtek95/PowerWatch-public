//! Date-specific DTEK fact cells shared by the direct adapter and mirror.
use crate::http::FetchError;
use chrono::{DateTime, Days, NaiveDateTime, TimeDelta, TimeZone, Utc};
use powerwatch_core::{
    grid::Queue,
    model::{DaySchedule, ScheduleInterval, ScheduleState, normalize_day},
    time::{LOCAL_ZONE, day_bounds, local_instant},
};
use serde::Deserialize;
use std::collections::BTreeMap;
type FactData = BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Fact {
    #[serde(deserialize_with = "empty_or_map")]
    data: FactData,
    update: Option<String>,
    update_fact: Option<String>,
}
pub(super) fn parse(
    data: &Fact,
    queue: &Queue,
    now: DateTime<Utc>,
) -> Result<(Vec<DaySchedule>, DateTime<Utc>, bool), FetchError> {
    let publication = data
        .update
        .as_ref()
        .or(data.update_fact.as_ref())
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
    Ok((days, publication, found))
}
fn empty_or_map<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<FactData, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.as_array().is_some_and(Vec::is_empty) {
        return Ok(BTreeMap::new());
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}
