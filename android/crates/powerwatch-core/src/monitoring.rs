//! Automatic polling policy only. A platform runner (stage 4) must execute one
//! request at a time and pass its actual completion time before starting another.
use crate::{CoreError, time::LOCAL_ZONE};
use chrono::{DateTime, TimeDelta, Timelike, Utc};
use serde::{Deserialize, Serialize};
pub const ADDRESS_PAUSE_SECONDS: i64 = 5;
/// Local wall-clock minutes, start inclusive and end exclusive. Wrapping ranges
/// such as 23:00–07:00 are valid; equal bounds require explicit all-day mode.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkWindow {
    pub start_minute: u16,
    pub end_minute: u16,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringSettings {
    #[serde(default)]
    pub enabled: bool,
    pub work_window: Option<WorkWindow>,
}
impl Default for MonitoringSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            work_window: Some(WorkWindow {
                start_minute: 420,
                end_minute: 0,
            }),
        }
    }
}
impl MonitoringSettings {
    pub fn is_valid(&self) -> bool {
        self.work_window.as_ref().is_none_or(|window| {
            window.start_minute < 1440
                && window.end_minute < 1440
                && window.start_minute != window.end_minute
        })
    }
    fn is_open(&self, at: DateTime<Utc>) -> bool {
        let Some(window) = &self.work_window else {
            return true;
        };
        let local = at.with_timezone(&LOCAL_ZONE);
        let minute = (local.hour() * 60 + local.minute()) as u16;
        if window.start_minute < window.end_minute {
            window.start_minute <= minute && minute < window.end_minute
        } else {
            minute >= window.start_minute || minute < window.end_minute
        }
    }
    /// Earliest start after the previous request COMPLETES and the pause elapses.
    /// Missed windows are skipped, never replayed as a burst. Walking UTC minutes
    /// avoids constructing ambiguous/nonexistent local instants on DST days.
    /// Manual refresh is separate and remains available outside the work window.
    pub fn next_allowed_at(
        &self,
        now: DateTime<Utc>,
        previous_finished_at: Option<DateTime<Utc>>,
    ) -> Result<DateTime<Utc>, CoreError> {
        if !self.is_valid() {
            return Err(CoreError::InvalidMonitoring);
        }
        let mut candidate = match previous_finished_at {
            Some(previous) => now.max(
                previous
                    .checked_add_signed(TimeDelta::seconds(ADDRESS_PAUSE_SECONDS))
                    .ok_or(CoreError::InvalidTime)?,
            ),
            None => now,
        };
        if self.is_open(candidate) {
            return Ok(candidate);
        }
        candidate = candidate
            .with_second(0)
            .and_then(|at| at.with_nanosecond(0))
            .ok_or(CoreError::InvalidTime)?;
        for _ in 0..=48 * 60 {
            candidate = candidate
                .checked_add_signed(TimeDelta::minutes(1))
                .ok_or(CoreError::InvalidTime)?;
            if self.is_open(candidate) {
                return Ok(candidate);
            }
        }
        Err(CoreError::InvalidTime)
    }
}
