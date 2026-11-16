//! Keep emergency applicability separate from the published plan. A healthy
//! response from another source cannot silently clear an existing warning.
use super::SourceSchedule;
use chrono::{DateTime, Utc};
use powerwatch_core::{
    grid::{Queue, SourceScope},
    model::{Address, PlanSuspension, ScheduleSnapshot, ScheduleState},
    time::LOCAL_ZONE,
};
pub(super) fn observations(
    fresh: &[SourceSchedule],
    previous: Option<&ScheduleSnapshot>,
    address: &Address,
    queue: &Queue,
    now: DateTime<Utc>,
) -> Vec<PlanSuspension> {
    let today = now.with_timezone(&LOCAL_ZONE).date_naive();
    let usable = |proof: &powerwatch_core::grid::SourceEvidence| {
        proof.scope == SourceScope::Queue(queue.clone())
            && proof.usable_at(now, chrono::TimeDelta::days(2))
    };
    let in_range = |date| date >= today && Some(date) <= today.succ_opt();
    let mut result: Vec<_> = previous
        .filter(|old| {
            old.address.id == address.id
                && old.provider_id == address.provider_id
                && old.address.queue.as_deref() == Some(queue.code.as_str())
        })
        .into_iter()
        .flat_map(|old| &old.plan_suspensions)
        .filter(|notice| in_range(notice.date) && usable(&notice.evidence))
        .filter(|notice| {
            !fresh.iter().any(|source| {
                usable(&source.evidence)
                    && source.evidence.provider.id == notice.evidence.provider.id
                    && source.evidence.checked_at >= notice.evidence.checked_at
                    && source
                        .evidence
                        .published_at
                        .zip(notice.evidence.published_at)
                        .is_none_or(|(new, old)| new >= old)
                    && !source.emergency_dates.contains(&notice.date)
                    && source.days.iter().any(|day| {
                        day.date == notice.date
                            && day
                                .intervals
                                .iter()
                                .any(|part| part.state != ScheduleState::Unknown)
                    })
            })
        })
        .cloned()
        .collect();
    for source in fresh.iter().filter(|source| usable(&source.evidence)) {
        for date in source
            .emergency_dates
            .iter()
            .copied()
            .filter(|date| in_range(*date))
        {
            if result.iter().any(|old| {
                old.date == date
                    && old.evidence.provider.id == source.evidence.provider.id
                    && (old.evidence.checked_at > source.evidence.checked_at
                        || old
                            .evidence
                            .published_at
                            .zip(source.evidence.published_at)
                            .is_some_and(|(old, new)| old > new))
            }) {
                continue;
            }
            result.retain(|old| {
                old.date != date || old.evidence.provider.id != source.evidence.provider.id
            });
            result.push(PlanSuspension {
                date,
                evidence: source.evidence.clone(),
            });
        }
    }
    result.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| a.evidence.provider.id.cmp(&b.evidence.provider.id))
    });
    result
}
