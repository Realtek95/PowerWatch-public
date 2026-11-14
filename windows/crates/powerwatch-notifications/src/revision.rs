//! Last known queue plan, independent of temporary loss of source coverage.
use chrono::{DateTime, NaiveDate, Utc};
use powerwatch_core::{
    history::{ScheduleChange, compare},
    model::{DaySchedule, ScheduleInterval, ScheduleSnapshot, ScheduleState, normalize_day},
    time::LOCAL_ZONE,
};
use serde::{Deserialize, Serialize};
fn piece_usable(
    snapshot: &ScheduleSnapshot,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    now: DateTime<Utc>,
    interval: u16,
) -> bool {
    if snapshot.received_at > now {
        return false;
    }
    let Some(provenance) = snapshot
        .queue_provenance
        .as_ref()
        .or(snapshot.provenance.as_ref())
    else {
        return super::forecast_usable(snapshot, now, interval);
    };
    let mut pieces: Vec<_> = provenance
        .iter()
        .filter(|part| part.start < end && part.end > start)
        .collect();
    pieces.sort_by_key(|part| part.start);
    let mut cursor = start;
    for part in pieces {
        if part.start > cursor {
            return false;
        }
        if part.evidence.iter().any(|proof| {
            proof.usable_at(now, chrono::TimeDelta::days(2))
                && matches!(
                    & proof.scope, powerwatch_core::grid::SourceScope::Queue(queue)
                    if snapshot.address.queue.as_deref() == Some(queue.code.as_str())
                )
        }) {
            cursor = cursor.max(part.end.min(end));
        }
        if cursor >= end {
            return true;
        }
    }
    false
}
pub(crate) fn updates_usable(
    snapshot: &ScheduleSnapshot,
    now: DateTime<Utc>,
    interval: u16,
) -> bool {
    snapshot
        .queue_days
        .as_ref()
        .unwrap_or(&snapshot.days)
        .iter()
        .flat_map(|day| &day.intervals)
        .any(|part| {
            part.end > now
                && part.state != ScheduleState::Unknown
                && piece_usable(snapshot, part.start, part.end, now, interval)
        })
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ForecastReference {
    queue: Option<String>,
    days: Vec<DaySchedule>,
    pub revision: u64,
}
pub struct ForecastUpdate {
    pub published: Vec<NaiveDate>,
    pub changes: Vec<ScheduleChange>,
}
impl ForecastReference {
    pub fn from_snapshot(snapshot: &ScheduleSnapshot) -> Self {
        Self {
            queue: snapshot.address.queue.clone(),
            days: snapshot
                .queue_days
                .as_ref()
                .unwrap_or(&snapshot.days)
                .clone(),
            revision: 0,
        }
    }
    /// Unknown observations never erase the last known plan. Calendar dates,
    /// rather than the labels today/tomorrow, survive a midnight rollover.
    pub fn advance(
        &self,
        snapshot: &ScheduleSnapshot,
        now: DateTime<Utc>,
        interval: u16,
    ) -> (Self, Option<ForecastUpdate>) {
        if snapshot.received_at > now {
            return (self.clone(), None);
        }
        let today = now.with_timezone(&LOCAL_ZONE).date_naive();
        let mut next = Self {
            queue: snapshot.address.queue.clone(),
            days: Vec::new(),
            revision: self.revision,
        };
        let known = |state| state != ScheduleState::Unknown;
        let mut changed = self.queue != next.queue;
        let mut published = Vec::new();
        for day in snapshot
            .queue_days
            .as_ref()
            .unwrap_or(&snapshot.days)
            .iter()
            .filter(|day| day.date >= today)
        {
            let previous = (self.queue == next.queue)
                .then(|| self.days.iter().find(|old| old.date == day.date))
                .flatten();
            let day_start = day.intervals.first().map(|part| part.start);
            let day_end = day.intervals.last().map(|part| part.end);
            let points: std::collections::BTreeSet<_> = day
                .intervals
                .iter()
                .chain(previous.into_iter().flat_map(|old| &old.intervals))
                .flat_map(|part| [part.start, part.end])
                .chain(
                    snapshot
                        .queue_provenance
                        .as_ref()
                        .or(snapshot.provenance.as_ref())
                        .into_iter()
                        .flatten()
                        .flat_map(|part| [part.start, part.end])
                        .filter(|point| {
                            day_start.is_some_and(|start| *point > start)
                                && day_end.is_some_and(|end| *point < end)
                        }),
                )
                .collect();
            let points: Vec<_> = points.into_iter().collect();
            let mut parts = Vec::new();
            let mut date_changed = false;
            for pair in points.windows(2) {
                let state = |parts: &[ScheduleInterval]| {
                    parts
                        .iter()
                        .find(|part| part.start <= pair[0] && part.end >= pair[1])
                        .map_or(ScheduleState::Unknown, |part| part.state)
                };
                let old = previous.map_or(ScheduleState::Unknown, |old| state(&old.intervals));
                let observed = if piece_usable(snapshot, pair[0], pair[1], now, interval) {
                    state(&day.intervals)
                } else {
                    ScheduleState::Unknown
                };
                let merged = if known(observed) { observed } else { old };
                date_changed |= pair[1] > now && known(observed) && observed != old;
                parts.push(ScheduleInterval {
                    start: pair[0],
                    end: pair[1],
                    state: merged,
                });
            }
            if date_changed
                && previous.is_none_or(|old| !old.intervals.iter().any(|part| known(part.state)))
            {
                published.push(day.date);
            }
            changed |= date_changed;
            let Ok(merged) = normalize_day(day.date, &parts) else {
                return (self.clone(), None);
            };
            next.days.push(merged);
        }
        for previous in self
            .days
            .iter()
            .filter(|day| day.date >= today && Some(day.date) <= today.succ_opt())
        {
            if self.queue == next.queue && !next.days.iter().any(|day| day.date == previous.date) {
                next.days.push(previous.clone());
            }
        }
        next.days.sort_by_key(|day| day.date);
        if !changed {
            return (next, None);
        }
        next.revision += 1;
        let mut before = snapshot.clone();
        before.queue_days = None;
        before.days = self.days.clone();
        before.address.queue = self.queue.clone();
        let mut after = before.clone();
        after.days = next.days.clone();
        after.address.queue = next.queue.clone();
        let changes = compare(&before, &after)
            .into_iter()
            .filter(|change| {
                change
                    .before
                    .as_ref()
                    .into_iter()
                    .chain(change.after.as_ref())
                    .any(|part| part.end > now)
            })
            .collect();
        (next, Some(ForecastUpdate { published, changes }))
    }
}
