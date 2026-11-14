//! Preserve only the last published queue cells for display, with their original
//! (possibly expired) evidence. Explicit incident/ emergency Unknown always wins.
use powerwatch_core::{
    grid::{EvidenceStatus, Queue, SourceScope},
    model::{Address, ScheduleInterval, ScheduleSnapshot, ScheduleState, normalize_day},
    sourcing::{IntervalEvidence, ResolvedDay},
};
use std::collections::BTreeSet;
pub(super) fn display_gaps(
    result: &mut ResolvedDay,
    previous: Option<&ScheduleSnapshot>,
    address: &Address,
    queue: &Queue,
    checked: chrono::DateTime<chrono::Utc>,
) -> bool {
    let Some(old) = previous.filter(|old| {
        old.received_at <= checked
            && old.address.id == address.id
            && old.provider_id == address.provider_id
            && old.address.queue.as_deref() == Some(queue.code.as_str())
    }) else {
        return false;
    };
    let Some(day) = old
        .queue_days
        .as_ref()
        .unwrap_or(&old.days)
        .iter()
        .find(|day| day.date == result.day.date)
    else {
        return false;
    };
    let Some(proofs) = old.queue_provenance.as_ref().or(old.provenance.as_ref()) else {
        return false;
    };
    let mut bounds = BTreeSet::new();
    for interval in result.day.intervals.iter().chain(&day.intervals) {
        bounds.insert(interval.start);
        bounds.insert(interval.end);
    }
    for part in result.provenance.iter().chain(proofs) {
        bounds.insert(part.start);
        bounds.insert(part.end);
    }
    let points: Vec<_> = bounds.into_iter().collect();
    let mut intervals = Vec::new();
    let mut provenance = Vec::new();
    let mut kept = false;
    for span in points.windows(2) {
        let Some(current) = result
            .day
            .intervals
            .iter()
            .find(|i| i.start <= span[0] && i.end >= span[1])
        else {
            continue;
        };
        let mut evidence: Vec<_> = result
            .provenance
            .iter()
            .filter(|p| p.start <= span[0] && p.end >= span[1])
            .flat_map(|p| p.evidence.iter().cloned())
            .collect();
        let mut state = current.state;
        if state == ScheduleState::Unknown
            && evidence.is_empty()
            && let Some(prior) = day.intervals.iter().find(|i| {
                i.start <= span[0] && i.end >= span[1] && i.state != ScheduleState::Unknown
            })
        {
            let prior_evidence: Vec<_> = proofs
                .iter()
                .filter(|p| p.start <= span[0] && p.end >= span[1])
                .flat_map(|p| &p.evidence)
                .filter(|p| {
                    p.scope == SourceScope::Queue(queue.clone())
                        && p.status == EvidenceStatus::Verified
                        && p.checked_at <= checked
                        && p.published_at.is_none_or(|at| at <= p.checked_at)
                })
                .cloned()
                .collect();
            if !prior_evidence.is_empty() {
                state = prior.state;
                evidence = prior_evidence;
                kept = true;
            }
        }
        intervals.push(ScheduleInterval {
            start: span[0],
            end: span[1],
            state,
        });
        provenance.push(IntervalEvidence {
            start: span[0],
            end: span[1],
            evidence,
        });
    }
    if kept && let Ok(day) = normalize_day(result.day.date, &intervals) {
        result.day = day;
        result.provenance = provenance;
    }
    kept
}
