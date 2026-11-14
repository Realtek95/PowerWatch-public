//! Source-independent comparisons and completed-day statistics.
use crate::model::{DaySchedule, ScheduleInterval, ScheduleSnapshot, ScheduleState};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Cancelled,
    Moved,
    Extended,
    Shortened,
    IntervalsRevised,
    DataAvailability,
    SourceStatus,
    Queue,
    ExpectedRestoration,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleChange {
    pub kind: ChangeKind,
    pub before: Option<ScheduleInterval>,
    pub after: Option<ScheduleInterval>,
}
/// Receipt/publication timestamps and display labels do not version a schedule.
pub fn signature(snapshot: &ScheduleSnapshot) -> Result<String, serde_json::Error> {
    serde_json::to_string(&(
        &snapshot.provider_id,
        &snapshot.address.id,
        &snapshot.address.queue,
        &snapshot.source_status,
        &snapshot.reason,
        &snapshot.expected_restoration,
        snapshot.queue_days.as_ref().unwrap_or(&snapshot.days),
        snapshot
            .address_check
            .as_ref()
            .and_then(|check| check.event.as_ref())
            .map(|event| &event.id),
    ))
}
pub fn outage_periods(days: &[DaySchedule]) -> Vec<ScheduleInterval> {
    let mut intervals: Vec<_> = days
        .iter()
        .flat_map(|day| day.intervals.iter())
        .filter(|item| item.state == ScheduleState::Outage)
        .cloned()
        .collect();
    intervals.sort_by_key(|item| item.start);
    let mut result: Vec<ScheduleInterval> = Vec::new();
    for item in intervals {
        if let Some(last) = result.last_mut()
            && last.end == item.start
        {
            last.end = item.end;
        } else {
            result.push(item);
        }
    }
    result
}
/// Compare only jointly known coverage. Losing data is not a cancellation;
/// recovering it is not a newly published outage.
fn comparable_days(
    before: &[DaySchedule],
    after: &[DaySchedule],
) -> (Vec<DaySchedule>, Vec<DaySchedule>, bool) {
    let mut old = Vec::new();
    let mut new = Vec::new();
    let mut coverage_changed = false;
    for day in before {
        let Some(current) = after.iter().find(|other| other.date == day.date) else {
            continue;
        };
        let points: std::collections::BTreeSet<_> = day
            .intervals
            .iter()
            .chain(&current.intervals)
            .flat_map(|item| [item.start, item.end])
            .collect();
        let points: Vec<_> = points.into_iter().collect();
        let mut old_parts = Vec::new();
        let mut new_parts = Vec::new();
        for pair in points.windows(2) {
            let state = |day: &DaySchedule| {
                day.intervals
                    .iter()
                    .find(|item| item.start <= pair[0] && item.end >= pair[1])
                    .map_or(ScheduleState::Unknown, |item| item.state)
            };
            let a = state(day);
            let b = state(current);
            let definite =
                |state| matches!(state, ScheduleState::Available | ScheduleState::Outage);
            let comparable = definite(a) && definite(b);
            coverage_changed |= a != b && !comparable;
            old_parts.push(ScheduleInterval {
                start: pair[0],
                end: pair[1],
                state: if comparable {
                    a
                } else {
                    ScheduleState::Unknown
                },
            });
            new_parts.push(ScheduleInterval {
                start: pair[0],
                end: pair[1],
                state: if comparable {
                    b
                } else {
                    ScheduleState::Unknown
                },
            });
        }
        old.push(DaySchedule {
            date: day.date,
            intervals: old_parts,
        });
        new.push(DaySchedule {
            date: day.date,
            intervals: new_parts,
        });
    }
    (old, new, coverage_changed)
}
/// Match exact periods first, then overlapping revisions. Complex changes retain
/// both complete snapshots in storage; an unmatched single pair is a move.
pub fn compare(before: &ScheduleSnapshot, after: &ScheduleSnapshot) -> Vec<ScheduleChange> {
    let mut previous = before.clone();
    let mut current = after.clone();
    previous.days = previous.queue_days.take().unwrap_or(previous.days);
    current.days = current.queue_days.take().unwrap_or(current.days);
    if previous.address_check.is_some()
        && current.address_check.is_none()
        && current.source_status == crate::model::SourceStatus::Unknown
    {
        current.source_status = previous.source_status.clone();
        current.reason = previous.reason.clone();
        current.expected_restoration = previous.expected_restoration;
        current.address_check = previous.address_check.clone();
    }
    previous
        .days
        .retain(|day| after.days.iter().any(|other| other.date == day.date));
    current
        .days
        .retain(|day| before.days.iter().any(|other| other.date == day.date));
    let (old_days, new_days, coverage_changed) = comparable_days(&previous.days, &current.days);
    previous.days = old_days;
    current.days = new_days;
    let before = &previous;
    let after = &current;
    let mut old = outage_periods(&before.days);
    let mut new = outage_periods(&after.days);
    old.retain(|item| !new.contains(item));
    let exact = outage_periods(&before.days);
    new.retain(|item| !exact.contains(item));
    let mut changes = Vec::new();
    if coverage_changed {
        changes.push(ScheduleChange {
            kind: ChangeKind::DataAvailability,
            before: None,
            after: None,
        });
    }
    while let Some(previous) = old.pop() {
        let matched = new
            .iter()
            .enumerate()
            .filter(|(_, item)| item.start < previous.end && previous.start < item.end)
            .max_by_key(|(_, item)| {
                (item.end.min(previous.end) - item.start.max(previous.start)).num_seconds()
            })
            .map(|(index, _)| index)
            .or_else(|| (old.is_empty() && new.len() == 1).then_some(0));
        let Some(index) = matched else {
            changes.push(ScheduleChange {
                kind: ChangeKind::Cancelled,
                before: Some(previous),
                after: None,
            });
            continue;
        };
        let current = new.remove(index);
        let kind = if current.start != previous.start {
            ChangeKind::Moved
        } else if current.end > previous.end {
            ChangeKind::Extended
        } else {
            ChangeKind::Shortened
        };
        changes.push(ScheduleChange {
            kind,
            before: Some(previous),
            after: Some(current),
        });
    }
    changes.extend(new.into_iter().map(|item| ScheduleChange {
        kind: ChangeKind::Added,
        before: None,
        after: Some(item),
    }));
    for (changed, kind) in [
        (
            before.source_status != after.source_status
                || before.reason != after.reason
                || before
                    .address_check
                    .as_ref()
                    .and_then(|check| check.event.as_ref())
                    .map(|event| &event.id)
                    != after
                        .address_check
                        .as_ref()
                        .and_then(|check| check.event.as_ref())
                        .map(|event| &event.id),
            ChangeKind::SourceStatus,
        ),
        (
            before.address.queue != after.address.queue,
            ChangeKind::Queue,
        ),
        (
            before.expected_restoration != after.expected_restoration
                && (after.source_status != crate::model::SourceStatus::Unknown
                    || after.expected_restoration.is_some()),
            ChangeKind::ExpectedRestoration,
        ),
    ] {
        if changed {
            changes.push(ScheduleChange {
                kind,
                before: None,
                after: None,
            });
        }
    }
    if changes.is_empty() && signature(before).ok() != signature(after).ok() {
        changes.push(ScheduleChange {
            kind: ChangeKind::IntervalsRevised,
            before: None,
            after: None,
        });
    }
    changes
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveQuality {
    Complete,
    Partial,
    Missing,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedDay {
    pub provenance: Option<Vec<crate::sourcing::IntervalEvidence>>,
    pub date: NaiveDate,
    pub quality: ArchiveQuality,
    pub day: Option<DaySchedule>,
    pub received_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub frozen_at: DateTime<Utc>,
    pub known_outage_seconds: i64,
    pub uncertain_seconds: i64,
    pub outage_count: usize,
    pub duration_seconds: i64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekStatistics {
    pub known_outage_seconds: i64,
    pub uncertain_seconds: i64,
    pub complete_days: usize,
    pub partial_days: usize,
    pub missing_days: usize,
    pub average_complete_day_seconds: Option<i64>,
    pub outage_count: usize,
}
pub fn statistics(days: &[ArchivedDay]) -> WeekStatistics {
    let complete: Vec<_> = days
        .iter()
        .filter(|day| day.quality == ArchiveQuality::Complete)
        .collect();
    WeekStatistics {
        known_outage_seconds: days.iter().map(|day| day.known_outage_seconds).sum(),
        uncertain_seconds: days.iter().map(|day| day.uncertain_seconds).sum(),
        complete_days: complete.len(),
        partial_days: days
            .iter()
            .filter(|day| day.quality == ArchiveQuality::Partial)
            .count(),
        missing_days: days
            .iter()
            .filter(|day| day.quality == ArchiveQuality::Missing)
            .count(),
        average_complete_day_seconds: (!complete.is_empty()).then(|| {
            complete
                .iter()
                .map(|day| day.known_outage_seconds)
                .sum::<i64>()
                / complete.len() as i64
        }),
        outage_count: outage_periods(
            &days
                .iter()
                .filter_map(|day| day.day.clone())
                .collect::<Vec<_>>(),
        )
        .len(),
    }
}
