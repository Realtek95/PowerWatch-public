//! In-memory trusted snapshots for local clock projections; persistence is stage 3.
use crate::{
    CoreError,
    dashboard::{Dashboard, build_dashboard},
    model::ScheduleSnapshot,
    time::LOCAL_ZONE,
};
use chrono::{DateTime, Days, Utc};
use serde::Serialize;
use std::collections::HashMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessStatus {
    Current,
    Stale,
    ClockMismatch,
}
/// Two polling periods tolerate one missed check; future receipts are untrusted.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Freshness {
    pub status: FreshnessStatus,
    pub age_seconds: i64,
    pub stale_after_seconds: i64,
}
impl Freshness {
    pub fn for_forecast(
        snapshot: &ScheduleSnapshot,
        now: DateTime<Utc>,
        interval_minutes: u16,
    ) -> Self {
        let mut value = Self::assess(snapshot.received_at, now, interval_minutes);
        if snapshot
            .queue_provenance
            .as_ref()
            .or(snapshot.provenance.as_ref())
            .is_some_and(|parts| {
                parts
                    .iter()
                    .flat_map(|part| &part.evidence)
                    .any(|proof| !proof.usable_at(now, chrono::TimeDelta::days(2)))
            })
            && value.status == FreshnessStatus::Current
        {
            value.status = FreshnessStatus::Stale;
        }
        value
    }
    pub fn for_snapshot(
        snapshot: &ScheduleSnapshot,
        now: DateTime<Utc>,
        interval_minutes: u16,
    ) -> Self {
        let mut value = Self::for_forecast(snapshot, now, interval_minutes);
        if value.status == FreshnessStatus::Current
            && snapshot.provenance.as_ref().is_some_and(|parts| {
                parts
                    .iter()
                    .flat_map(|part| &part.evidence)
                    .any(|proof| !proof.usable_at(now, chrono::TimeDelta::days(2)))
            })
        {
            value.status = FreshnessStatus::Stale;
        }
        value
    }
    pub fn assess(received: DateTime<Utc>, now: DateTime<Utc>, interval_minutes: u16) -> Self {
        let age = (now - received).num_seconds();
        let threshold = i64::from(interval_minutes.max(1)) * 120;
        Self {
            status: if age < 0 {
                FreshnessStatus::ClockMismatch
            } else if age >= threshold {
                FreshnessStatus::Stale
            } else {
                FreshnessStatus::Current
            },
            age_seconds: age.max(0),
            stale_after_seconds: threshold,
        }
    }
}
/// Only successful, validated fetches may replace a snapshot. Projections never
/// contact the provider or change receipt/publication timestamps.
#[derive(Default)]
pub struct DashboardCache {
    snapshots: HashMap<String, ScheduleSnapshot>,
}
impl DashboardCache {
    pub fn record(&mut self, local_id: &str, snapshot: ScheduleSnapshot) {
        self.snapshots.insert(local_id.into(), snapshot);
    }
    pub fn retain_addresses(&mut self, ids: &[&str]) {
        self.snapshots.retain(|id, _| ids.contains(&id.as_str()));
    }
    pub fn project(
        &self,
        local_id: &str,
        day_offset: u64,
        now: DateTime<Utc>,
        interval_minutes: u16,
    ) -> Result<Dashboard, CoreError> {
        if day_offset > 1 {
            return Err(CoreError::MissingDay);
        }
        let snapshot = self.snapshots.get(local_id).ok_or(CoreError::MissingDay)?;
        let date = now
            .with_timezone(&LOCAL_ZONE)
            .date_naive()
            .checked_add_days(Days::new(day_offset))
            .ok_or(CoreError::InvalidTime)?;
        let mut dashboard = build_dashboard(snapshot.clone(), date, now)?;
        dashboard.freshness = Freshness::for_snapshot(snapshot, now, interval_minutes);
        Ok(dashboard)
    }
}
