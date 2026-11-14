//! Evidence-scoped forecast resolution, independent of HTTP and the UI.
use crate::{
    CoreError,
    grid::{
        EventKind, EvidenceStatus, OutageEvent, ProviderAddressId, ProviderKind, Queue,
        SourceEvidence, SourceScope,
    },
    model::{DaySchedule, ScheduleInterval, ScheduleState, normalize_day},
    time::day_bounds,
};
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceForecast {
    /// Last known data fill only missing coverage; their original proof is kept.
    #[serde(default)]
    pub retained: bool,
    pub days: Vec<DaySchedule>,
    pub evidence: SourceEvidence,
    pub emergency_dates: Vec<NaiveDate>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntervalEvidence {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub evidence: Vec<SourceEvidence>,
}
pub struct ResolvedDay {
    pub day: DaySchedule,
    pub provenance: Vec<IntervalEvidence>,
}
fn rank(kind: &ProviderKind) -> u8 {
    match kind {
        ProviderKind::Operator => 1,
        ProviderKind::Aggregator => 2,
        ProviderKind::Mirror => 3,
        ProviderKind::Synthetic => u8::MAX,
    }
}
/// A fresh address event overrides a queue only in its covered interval. Open
/// events preserve uncertainty after the observation, never invent an end.
pub fn resolve_day(
    queue: &Queue,
    binding: Option<&ProviderAddressId>,
    date: NaiveDate,
    forecasts: &[SourceForecast],
    events: &[OutageEvent],
    now: DateTime<Utc>,
) -> Result<ResolvedDay, CoreError> {
    let (start, end) = day_bounds(date)?;
    let usable = |proof: &SourceEvidence| proof.usable_at(now, TimeDelta::days(2));
    let forecasts: Vec<_> = forecasts
        .iter()
        .filter(|candidate| {
            usable(&candidate.evidence)
                && candidate.evidence.scope == SourceScope::Queue(queue.clone())
        })
        .collect();
    let events: Vec<_> = events
        .iter()
        .filter(|event| {
            usable(&event.evidence)
                && event.kind != EventKind::Forecast
                && event.start < end
                && event.end.is_none_or(|at| at > start)
                && binding.is_some_and(|binding| {
                    binding.checked_at <= now
                        && binding.provider_id == event.evidence.provider.id
                        && event.evidence.scope
                            == SourceScope::Address {
                                operator_id: queue.operator_id.clone(),
                                source_id: binding.source_id.clone(),
                            }
                })
        })
        .collect();
    let mut boundaries = BTreeSet::from([start, end]);
    for forecast in &forecasts {
        if let Some(day) = forecast.days.iter().find(|day| day.date == date) {
            normalize_day(date, &day.intervals)?;
            for interval in &day.intervals {
                boundaries.insert(interval.start.clamp(start, end));
                boundaries.insert(interval.end.clamp(start, end));
            }
        }
    }
    for event in &events {
        if event.end.is_some_and(|at| at <= event.start) {
            return Err(CoreError::InvalidInterval);
        }
        boundaries.insert(event.start.clamp(start, end));
        boundaries.insert(event.end.unwrap_or(end).clamp(start, end));
        if event.end.is_none() {
            boundaries.insert(now.clamp(start, end));
        }
    }
    let points: Vec<_> = boundaries.into_iter().collect();
    let mut intervals = Vec::new();
    let mut provenance = Vec::new();
    for pair in points.windows(2) {
        let mut choices: Vec<(u8, ScheduleState, &SourceEvidence, bool)> = Vec::new();
        for candidate in &forecasts {
            let priority = rank(&candidate.evidence.provider.kind)
                .saturating_add(if candidate.retained { 10 } else { 0 });
            if candidate.emergency_dates.contains(&date) {
                choices.push((
                    priority,
                    ScheduleState::Unknown,
                    &candidate.evidence,
                    candidate.retained,
                ));
            } else if let Some(interval) = candidate
                .days
                .iter()
                .find(|day| day.date == date)
                .and_then(|day| {
                    day.intervals
                        .iter()
                        .find(|item| item.start <= pair[0] && item.end >= pair[1])
                })
                && interval.state != ScheduleState::Unknown
            {
                choices.push((
                    priority,
                    interval.state,
                    &candidate.evidence,
                    candidate.retained,
                ));
            }
        }
        let retained: Vec<_> = choices.iter().filter(|choice| choice.3).copied().collect();
        choices.retain(|choice| {
            choice.3
                || (choice.0 == 1 && choice.1 == ScheduleState::Unknown)
                || !retained.iter().any(|old| {
                    rank(&old.2.provider.kind) <= rank(&choice.2.provider.kind)
                        && old
                            .2
                            .published_at
                            .zip(choice.2.published_at)
                            .is_some_and(|(old, new)| old > new)
                })
        });
        for event in &events {
            if event.start <= pair[0] && event.end.unwrap_or(end) >= pair[1] {
                let state = if event.end.is_none() && pair[1] > now {
                    ScheduleState::Unknown
                } else {
                    event.state
                };
                choices.push((0, state, &event.evidence, false));
            }
        }
        let priority = choices.iter().map(|choice| choice.0).min();
        let selected: Vec<_> = choices
            .into_iter()
            .filter(|choice| Some(choice.0) == priority)
            .collect();
        let conflict = selected.windows(2).any(|pair| pair[0].1 != pair[1].1);
        let state = if conflict {
            ScheduleState::Unknown
        } else {
            selected
                .first()
                .map(|choice| choice.1)
                .unwrap_or(ScheduleState::Unknown)
        };
        intervals.push(ScheduleInterval {
            start: pair[0],
            end: pair[1],
            state,
        });
        provenance.push(IntervalEvidence {
            start: pair[0],
            end: pair[1],
            evidence: selected
                .into_iter()
                .map(|choice| {
                    let mut proof = choice.2.clone();
                    if conflict {
                        proof.status = EvidenceStatus::Conflicting;
                    }
                    proof
                })
                .collect(),
        });
    }
    Ok(ResolvedDay {
        day: normalize_day(date, &intervals)?,
        provenance,
    })
}
