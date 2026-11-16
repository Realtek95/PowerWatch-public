use crate::{
    CoreError,
    cache::Freshness,
    model::{DaySchedule, ScheduleInterval, ScheduleSnapshot, ScheduleState},
    time::{LOCAL_ZONE, local_instant},
};
use chrono::{DateTime, NaiveDate, TimeDelta, Timelike, Utc};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleTick {
    pub position: f64,
    pub label: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BandSegment {
    pub interval_index: usize,
    pub top: f64,
    pub height: f64,
    pub state: ScheduleState,
}
/// Splitting a visual band never changes the number of domain intervals.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayBand {
    pub label: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub ticks: Vec<ScaleTick>,
    pub segments: Vec<BandSegment>,
    pub now_position: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    OutageStart,
    OutageEnd,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextEvent {
    pub kind: EventKind,
    pub at: DateTime<Utc>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub period_end_known: bool,
}
/// Presentation data is calculated in Rust, shared by Windows and Android.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check_error: Option<String>,
    pub retry_pending: bool,
    pub refresh_pending: bool,
    pub now: DateTime<Utc>,
    pub freshness: Freshness,
    pub snapshot: ScheduleSnapshot,
    pub selected_day: DaySchedule,
    pub bands: Vec<DayBand>,
    pub current_state: ScheduleState,
    pub next_event: Option<NextEvent>,
    pub next_event_uncertain: bool,
    pub known_outage_seconds: i64,
    pub has_uncertainty: bool,
    pub clock_change: bool,
}
pub fn build_dashboard(
    snapshot: ScheduleSnapshot,
    date: NaiveDate,
    now: DateTime<Utc>,
) -> Result<Dashboard, CoreError> {
    let display_days = if snapshot
        .plan_suspensions
        .iter()
        .any(|notice| notice.date == date)
    {
        snapshot.queue_days.as_ref().unwrap_or(&snapshot.days)
    } else {
        &snapshot.days
    };
    let day = display_days
        .iter()
        .find(|day| day.date == date)
        .ok_or(CoreError::MissingDay)?
        .clone();
    let current_state = snapshot
        .days
        .iter()
        .flat_map(|day| &day.intervals)
        .find(|item| item.start <= now && now < item.end)
        .map_or(ScheduleState::Unknown, |item| item.state);
    let mut outages: Vec<ScheduleInterval> = Vec::new();
    let mut source_intervals: Vec<_> = snapshot
        .days
        .iter()
        .flat_map(|day| &day.intervals)
        .collect();
    source_intervals.sort_by_key(|item| item.start);
    for item in source_intervals
        .iter()
        .copied()
        .filter(|item| item.state == ScheduleState::Outage)
    {
        if let Some(last) = outages.last_mut()
            && last.end == item.start
        {
            last.end = item.end;
        } else {
            outages.push(item.clone());
        }
    }
    let next_event = outages
        .iter()
        .find(|item| item.end > now)
        .filter(|item| {
            let target = if item.start > now {
                item.start
            } else {
                item.end
            };
            let unknown_before = source_intervals.iter().any(|part| {
                part.start < target
                    && part.end > now
                    && matches!(
                        part.state,
                        ScheduleState::Unknown | ScheduleState::Switching
                    )
            });
            let known_end = source_intervals
                .iter()
                .any(|part| part.start == item.end && part.state == ScheduleState::Available);
            !unknown_before && (item.start > now || known_end)
        })
        .map(|item| NextEvent {
            kind: if item.start > now {
                EventKind::OutageStart
            } else {
                EventKind::OutageEnd
            },
            at: if item.start > now {
                item.start
            } else {
                item.end
            },
            period_start: item.start,
            period_end: item.end,
            period_end_known: source_intervals
                .iter()
                .any(|part| part.start == item.end && part.state == ScheduleState::Available),
        });
    let next_event_uncertain = next_event.is_none()
        && (current_state != ScheduleState::Available
            || source_intervals.iter().any(|part| {
                part.end > now
                    && matches!(
                        part.state,
                        ScheduleState::Unknown | ScheduleState::Switching
                    )
            }));
    let bands = vec![project_band(&day, 0, now)?, project_band(&day, 12, now)?];
    let known_outage_seconds = day
        .intervals
        .iter()
        .filter(|item| item.state == ScheduleState::Outage)
        .map(|item| (item.end - item.start).num_seconds())
        .sum();
    let has_uncertainty = day.intervals.iter().any(|item| {
        matches!(
            item.state,
            ScheduleState::Unknown | ScheduleState::Switching
        )
    });
    let clock_change = (local_instant(date, 1440)? - local_instant(date, 0)?).num_hours() != 24;
    Ok(Dashboard {
        last_check_error: None,
        retry_pending: false,
        refresh_pending: false,
        now,
        freshness: Freshness::for_snapshot(&snapshot, now, 30),
        snapshot,
        selected_day: day,
        bands,
        current_state,
        next_event,
        next_event_uncertain,
        known_outage_seconds,
        has_uncertainty,
        clock_change,
    })
}
fn project_band(day: &DaySchedule, hour: u16, now: DateTime<Utc>) -> Result<DayBand, CoreError> {
    let start = local_instant(day.date, hour * 60)?;
    let end = local_instant(day.date, (hour + 12) * 60)?;
    let duration = (end - start).num_seconds() as f64;
    let position = |instant: DateTime<Utc>| (instant - start).num_seconds() as f64 / duration;
    let clock_change = duration != 12.0 * 3600.0;
    let mut ticks = Vec::new();
    let mut tick = start;
    while tick < end {
        let wall = tick.with_timezone(&LOCAL_ZONE);
        if wall.hour().is_multiple_of(2) {
            ticks.push(ScaleTick {
                position: position(tick),
                label: wall
                    .format(if clock_change { "%H:%M %:z" } else { "%H:%M" })
                    .to_string(),
            });
        }
        tick += TimeDelta::hours(1);
    }
    ticks.push(ScaleTick {
        position: 1.0,
        label: format!("{:02}:00", hour + 12),
    });
    let segments = day
        .intervals
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let segment_start = item.start.max(start);
            let segment_end = item.end.min(end);
            (segment_start < segment_end).then(|| BandSegment {
                interval_index: index,
                top: position(segment_start),
                height: position(segment_end) - position(segment_start),
                state: item.state,
            })
        })
        .collect();
    Ok(DayBand {
        label: format!("{hour:02}:00 – {:02}:00", hour + 12),
        start,
        end,
        ticks,
        segments,
        now_position: (start <= now && now < end).then(|| position(now)),
    })
}
