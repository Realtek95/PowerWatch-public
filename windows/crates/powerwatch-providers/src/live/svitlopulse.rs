//! Read forecasts only; crowd signals/probabilities never become home status.
use super::{SourceSchedule, evidence};
use crate::http::FetchError;
use chrono::{DateTime, Days, TimeDelta, Utc};
use powerwatch_core::{
    grid::{ProviderKind, Queue},
    model::{ScheduleInterval, ScheduleState, normalize_day},
    time::{LOCAL_ZONE, day_bounds},
};
use serde::Deserialize;
#[derive(Deserialize)]
pub struct Summary {
    pub demo: bool,
    pub rev: u64,
    pub at: DateTime<Utc>,
}
#[derive(Deserialize)]
pub struct Response {
    at: DateTime<Utc>,
    rev: u64,
    group: Group,
    operator: Operator,
}
#[derive(Deserialize)]
struct Group {
    operator_id: String,
    group: String,
    today: Vec<Interval>,
    tomorrow: Vec<Interval>,
    plan_status: String,
    class: String,
}
#[derive(Deserialize)]
struct Interval {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    kind: String,
}
#[derive(Deserialize)]
struct Operator {
    id: String,
    stale: bool,
    updated_at: Option<DateTime<Utc>>,
}
pub fn parse(
    summary: &Summary,
    data: &Response,
    external_operator: &str,
    queue: &Queue,
    now: DateTime<Utc>,
) -> Result<SourceSchedule, FetchError> {
    let source_at = data.operator.updated_at.ok_or(FetchError::InvalidData)?;
    if summary.demo
        || summary.rev != data.rev
        || data.operator.id != external_operator
        || data.group.operator_id != external_operator
        || data.group.group != queue.code
        || data.operator.stale
        || source_at > now
        || now - source_at > TimeDelta::minutes(45)
        || data.at > now
        || summary.at > now
        || now - data.at > TimeDelta::minutes(2)
        || now - summary.at > TimeDelta::minutes(2)
    {
        return Err(FetchError::InvalidData);
    }
    let today = data.at.with_timezone(&LOCAL_ZONE).date_naive();
    let tomorrow = today
        .checked_add_days(Days::new(1))
        .ok_or(FetchError::InvalidData)?;
    let emergency = data.group.class == "emergency" || data.group.plan_status == "emergency";
    let mut days = Vec::new();
    for (date, ranges) in [(today, &data.group.today), (tomorrow, &data.group.tomorrow)] {
        let (start, end) = day_bounds(date).map_err(|_| FetchError::InvalidData)?;
        let applied = !emergency && data.group.plan_status == "applies" && !ranges.is_empty();
        let published = applied
            || (emergency
                && ranges
                    .iter()
                    .any(|range| matches!(range.kind.as_str(), "off" | "maybe")));
        let mut intervals = Vec::new();
        for range in ranges {
            if range.start < start || range.end > end || range.end <= range.start {
                return Err(FetchError::InvalidData);
            }
            let state = if !published {
                ScheduleState::Unknown
            } else {
                match range.kind.as_str() {
                    "off" => ScheduleState::Outage,
                    "maybe" => ScheduleState::Switching,
                    "on" => ScheduleState::Available,
                    _ => ScheduleState::Unknown,
                }
            };
            intervals.push(ScheduleInterval {
                start: range.start,
                end: range.end,
                state,
            });
        }
        intervals.sort_by_key(|item| item.start);
        if applied {
            let mut cursor = start;
            let mut complement = Vec::new();
            for item in &intervals {
                if item.start > cursor {
                    complement.push(ScheduleInterval {
                        start: cursor,
                        end: item.start,
                        state: ScheduleState::Available,
                    });
                }
                cursor = item.end;
            }
            if cursor < end {
                complement.push(ScheduleInterval {
                    start: cursor,
                    end,
                    state: ScheduleState::Available,
                });
            }
            intervals.extend(complement);
        }
        days.push(normalize_day(date, &intervals).map_err(|_| FetchError::InvalidData)?);
    }
    let mut proof = evidence(
        "svitlopulse",
        "SvitloPulse",
        ProviderKind::Aggregator,
        "https://svitlopulse.andrewboyko.name/api/",
        queue,
        now,
        Some(source_at),
    );
    proof.valid_until = Some(source_at + TimeDelta::minutes(45));
    Ok(SourceSchedule {
        retained: false,
        days,
        evidence: proof,
        emergency_dates: if emergency { vec![today] } else { Vec::new() },
    })
}
