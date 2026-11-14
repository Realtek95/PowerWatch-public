use crate::{CoreError, time::day_bounds};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
/// Optional source facts stay separate from any forecast of physical power.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Unknown,
    ReportedAvailable,
    ReportedUnavailable,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleState {
    Available,
    Outage,
    Switching,
    Unknown,
}
/// Intervals are half-open UTC ranges, even when the UI crosses local midnight.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInterval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub state: ScheduleState,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
}
/// A local label must never replace the provider's stable address identifiers.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    pub details: Option<crate::grid::AddressDetails>,
    pub id: String,
    pub provider_id: String,
    pub settlement_id: String,
    pub street_id: String,
    pub settlement: String,
    pub street: String,
    pub house: String,
    pub label: String,
    pub queue: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySchedule {
    pub date: NaiveDate,
    pub intervals: Vec<ScheduleInterval>,
}
/// Receipt and publication times have different meanings and are not conflated.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_provenance: Option<Vec<crate::sourcing::IntervalEvidence>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_check: Option<crate::grid::AddressCheck>,
    /// Original queue plan, before an address observation overlays the view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_days: Option<Vec<DaySchedule>>,
    pub provenance: Option<Vec<crate::sourcing::IntervalEvidence>>,
    pub provider_id: String,
    pub is_demo: bool,
    pub address: Address,
    pub received_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub source_status: SourceStatus,
    pub reason: Option<String>,
    pub expected_restoration: Option<DateTime<Utc>>,
    pub days: Vec<DaySchedule>,
}
/// Fill gaps with Unknown, reject overlaps, then merge equivalent neighbours.
/// Clipping for a day is a view operation; source intervals remain unsplit.
pub fn normalize_day(
    date: NaiveDate,
    intervals: &[ScheduleInterval],
) -> Result<DaySchedule, CoreError> {
    if intervals.iter().any(|item| item.start >= item.end) {
        return Err(CoreError::InvalidInterval);
    }
    let (start, end) = day_bounds(date)?;
    let mut clipped: Vec<_> = intervals
        .iter()
        .filter(|item| item.end > start && item.start < end)
        .map(|item| ScheduleInterval {
            start: item.start.max(start),
            end: item.end.min(end),
            state: item.state,
        })
        .collect();
    clipped.sort_by_key(|item| item.start);
    let mut result = Vec::new();
    let mut cursor = start;
    for item in clipped {
        if item.start < cursor {
            return Err(CoreError::OverlappingIntervals);
        }
        if item.start > cursor {
            push_merged(
                &mut result,
                ScheduleInterval {
                    start: cursor,
                    end: item.start,
                    state: ScheduleState::Unknown,
                },
            );
        }
        cursor = item.end;
        push_merged(&mut result, item);
    }
    if cursor < end {
        push_merged(
            &mut result,
            ScheduleInterval {
                start: cursor,
                end,
                state: ScheduleState::Unknown,
            },
        );
    }
    Ok(DaySchedule {
        date,
        intervals: result,
    })
}
fn push_merged(result: &mut Vec<ScheduleInterval>, item: ScheduleInterval) {
    if let Some(last) = result.last_mut()
        && last.end == item.start
        && last.state == item.state
    {
        last.end = item.end;
        return;
    }
    result.push(item);
}
