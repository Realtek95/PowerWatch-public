use crate::{Result, StorageError, Store, decode, encode, instant};
use chrono::{DateTime, Days, NaiveDate, Utc};
use powerwatch_core::{
    dashboard::{Dashboard, build_dashboard},
    history::{ArchiveQuality, ArchivedDay, ScheduleChange, WeekStatistics, statistics},
    model::{Address, DaySchedule, ScheduleSnapshot, ScheduleState, SourceStatus},
    time::{LOCAL_ZONE, day_bounds},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
pub(crate) struct DailyCandidate {
    #[serde(flatten)]
    pub day: DaySchedule,
    pub provenance: Option<Vec<powerwatch_core::sourcing::IntervalEvidence>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryAddress {
    pub key: String,
    pub address: Address,
    pub retired: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRecord {
    pub id: i64,
    pub detected_at: DateTime<Utc>,
    pub detected_date: NaiveDate,
    pub published_at: Option<DateTime<Utc>>,
    pub changes: Vec<ScheduleChange>,
    pub before: ScheduleSnapshot,
    pub after: ScheduleSnapshot,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveView {
    pub addresses: Vec<HistoryAddress>,
    pub selected_key: Option<String>,
    pub days: Vec<ArchivedDay>,
    pub statistics: WeekStatistics,
    pub changes: Vec<ChangeRecord>,
    pub dashboard: Option<Dashboard>,
}
impl Store {
    /// Freeze completed calendar days once. Missing records also remain honest;
    /// a later response cannot retroactively create a history never observed.
    pub fn reconcile(&mut self, now: DateTime<Utc>) -> Result<()> {
        let today = now.with_timezone(&LOCAL_ZONE).date_naive();
        let oldest = today
            .checked_sub_days(Days::new(7))
            .ok_or(StorageError::InvalidTime)?;
        let keys: Vec<String> = self
            .conn
            .prepare("SELECT key FROM addresses")?
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let tx = self.conn.transaction()?;
        for key in keys {
            for offset in 1..=7 {
                let date = today
                    .checked_sub_days(Days::new(offset))
                    .ok_or(StorageError::InvalidTime)?;
                let exists: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM daily_schedule_archive WHERE address_key=?1 AND date=?2)",
                        params![key, date.to_string()],
                        |row| row.get(0),
                    )?;
                if exists {
                    continue;
                }
                let candidate: Option<(String, Option<String>, String)> = tx
                    .query_row(
                        "SELECT received_at,published_at,day_json FROM daily_candidates WHERE address_key=?1 AND date=?2",
                        params![key, date.to_string()],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let (start, end) = day_bounds(date)?;
                let mut archive = ArchivedDay {
                    provenance: None,
                    date,
                    quality: ArchiveQuality::Missing,
                    day: None,
                    received_at: None,
                    published_at: None,
                    frozen_at: now,
                    known_outage_seconds: 0,
                    uncertain_seconds: (end - start).num_seconds(),
                    outage_count: 0,
                    duration_seconds: (end - start).num_seconds(),
                };
                if let Some((received, published, json)) = candidate {
                    let candidate: DailyCandidate = decode(&json)?;
                    let day = candidate.day;
                    archive.provenance = candidate.provenance;
                    let received = instant(&received)?;
                    archive.known_outage_seconds = day
                        .intervals
                        .iter()
                        .filter(|item| item.state == ScheduleState::Outage)
                        .map(|item| (item.end - item.start).num_seconds())
                        .sum();
                    archive.uncertain_seconds = day
                        .intervals
                        .iter()
                        .filter(|item| {
                            matches!(
                                item.state,
                                ScheduleState::Unknown | ScheduleState::Switching
                            )
                        })
                        .map(|item| (item.end - item.start).num_seconds())
                        .sum();
                    archive.outage_count = day
                        .intervals
                        .iter()
                        .filter(|item| item.state == ScheduleState::Outage)
                        .count();
                    archive.quality = if archive.uncertain_seconds == 0 && received >= start {
                        ArchiveQuality::Complete
                    } else {
                        ArchiveQuality::Partial
                    };
                    archive.day = Some(day);
                    archive.received_at = Some(received);
                    archive.published_at = published.map(|value| instant(&value)).transpose()?;
                }
                tx.execute(
                    "INSERT INTO daily_schedule_archive(address_key,date,archive_json) VALUES(?1,?2,?3)",
                    params![key, date.to_string(), encode(& archive) ?],
                )?;
            }
        }
        for table in [
            "daily_schedule_archive",
            "daily_candidates",
            "schedule_intervals",
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE date<?1"),
                [oldest.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn archive_view(
        &mut self,
        preferred: Option<&str>,
        date: Option<NaiveDate>,
        now: DateTime<Utc>,
    ) -> Result<ArchiveView> {
        self.reconcile(now)?;
        let raw: Vec<(String, String, bool)> = self
            .conn
            .prepare(
                "SELECT a.key,a.address_json,a.retired FROM addresses a WHERE a.retired=0 OR NOT EXISTS(SELECT 1 FROM addresses current WHERE current.local_id=a.local_id AND current.retired=0) ORDER BY a.retired,a.key",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;
        let addresses: Vec<_> = raw
            .into_iter()
            .map(|(key, json, retired)| {
                Ok(HistoryAddress {
                    key,
                    address: decode(&json)?,
                    retired,
                })
            })
            .collect::<Result<_>>()?;
        let preferred_current = preferred
            .map(|key| self.current_history_key(key))
            .transpose()?
            .flatten();
        let selected = preferred_current
            .as_deref()
            .or(preferred)
            .and_then(|key| addresses.iter().find(|entry| entry.key == key))
            .or_else(|| addresses.first());
        let selected_key = selected.map(|entry| entry.key.clone());
        let mut days: Vec<ArchivedDay> = Vec::new();
        let mut changes = Vec::new();
        let mut dashboard = None;
        if let Some(selected) = selected {
            let keys = self.history_keys(&selected.key)?;
            let slots = vec!["?"; keys.len()].join(",");
            let today = now.with_timezone(&LOCAL_ZONE).date_naive().to_string();
            let mut values: Vec<&str> = keys.iter().map(String::as_str).collect();
            values.push(&today);
            let rows: Vec<(String, String)> = self
                .conn
                .prepare(
                    &format!(
                        "SELECT address_key,archive_json FROM daily_schedule_archive WHERE address_key IN ({slots}) AND date<? ORDER BY date DESC"
                    ),
                )?
                .query_map(
                    rusqlite::params_from_iter(values),
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?
                .collect::<std::result::Result<_, _>>()?;
            let mut merged: std::collections::BTreeMap<NaiveDate, (bool, ArchivedDay)> =
                std::collections::BTreeMap::new();
            for (key, json) in rows {
                let archive: ArchivedDay = decode(&json)?;
                let current = key == selected.key;
                let rank = |current: bool, day: &ArchivedDay| {
                    (
                        day.received_at,
                        day.published_at,
                        day.day.is_some(),
                        current,
                    )
                };
                if merged.get(&archive.date).is_none_or(|(old_current, old)| {
                    rank(current, &archive) > rank(*old_current, old)
                }) {
                    merged.insert(archive.date, (current, archive));
                }
            }
            days = merged.into_values().rev().map(|(_, day)| day).collect();
            let chosen = date
                .and_then(|date| days.iter().find(|day| day.date == date))
                .or_else(|| days.first());
            if let Some(archive) = chosen
                && let Some(day) = &archive.day
            {
                let snapshot = ScheduleSnapshot {
                    plan_suspensions: Vec::new(),
                    forecast_error: None,
                    queue_provenance: None,
                    address_check: None,
                    queue_days: None,
                    provenance: archive.provenance.clone(),
                    provider_id: selected.address.provider_id.clone(),
                    is_demo: selected.address.provider_id == "mock",
                    address: selected.address.clone(),
                    received_at: archive.received_at.unwrap_or(archive.frozen_at),
                    published_at: archive.published_at,
                    source_status: SourceStatus::Unknown,
                    reason: None,
                    expected_restoration: None,
                    days: vec![day.clone()],
                };
                dashboard = Some(build_dashboard(snapshot, archive.date, now)?);
            }
            let rows: Vec<(i64, String, Option<String>, String, String, String)> = self
                .conn
                .prepare(
                    &format!(
                        "SELECT id,detected_at,published_at,changes_json,before_json,after_json FROM schedule_changes WHERE address_key IN ({slots}) ORDER BY detected_at DESC,id DESC LIMIT 100"
                    ),
                )?
                .query_map(
                    rusqlite::params_from_iter(&keys),
                    |row| Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    )),
                )?
                .collect::<std::result::Result<_, _>>()?;
            for (id, detected, published, json, before, after) in rows {
                changes.push(ChangeRecord {
                    id,
                    detected_at: instant(&detected)?,
                    detected_date: instant(&detected)?.with_timezone(&LOCAL_ZONE).date_naive(),
                    published_at: published.map(|value| instant(&value)).transpose()?,
                    changes: decode(&json)?,
                    before: decode(&before)?,
                    after: decode(&after)?,
                });
            }
        }
        Ok(ArchiveView {
            addresses,
            selected_key,
            statistics: statistics(&days),
            days,
            changes,
            dashboard,
        })
    }
}
