//! Durable outbox: claim before OS submission, never equate submission with delivery.
use crate::{Result, Store, address_key, decode, encode, instant};
use chrono::{DateTime, TimeDelta, Utc};
use powerwatch_notifications::{NotificationKind, NotificationPreferences, PlannedNotification};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRecord {
    pub id: i64,
    pub plan: PlannedNotification,
    pub state: String,
    pub updated_at: DateTime<Utc>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollState {
    pub address_key: String,
    pub due_at: DateTime<Utc>,
    pub failures: u32,
    pub last_attempt: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
}
impl Store {
    pub fn notification_preferences(&self) -> Result<NotificationPreferences> {
        Ok(self
            .setting("notification_preferences")?
            .unwrap_or_default())
    }
    pub fn notification_log(&self) -> Result<Vec<NotificationRecord>> {
        let rows: Vec<(i64, String, String, String)> = self
            .conn
            .prepare(
                "SELECT id,payload_json,state,updated_at FROM notification_journal ORDER BY updated_at DESC,id DESC LIMIT 25",
            )?
            .query_map(
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?
            .collect::<std::result::Result<_, _>>()?;
        rows.into_iter()
            .map(|(id, json, state, at)| {
                Ok(NotificationRecord {
                    id,
                    plan: decode(&json)?,
                    state,
                    updated_at: instant(&at)?,
                })
            })
            .collect()
    }
    pub fn pending_notifications(&self) -> Result<Vec<NotificationRecord>> {
        let rows: Vec<(i64, String, String)> = self
            .conn
            .prepare(
                "SELECT id,payload_json,updated_at FROM notifications WHERE state='planned' ORDER BY planned_at",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;
        rows.into_iter()
            .map(|(id, json, at)| {
                Ok(NotificationRecord {
                    id,
                    plan: decode(&json)?,
                    state: "planned".into(),
                    updated_at: instant(&at)?,
                })
            })
            .collect()
    }
    pub fn clear_notification_journal(&self, id: Option<i64>) -> Result<()> {
        match id {
            Some(id) => {
                self.conn
                    .execute("DELETE FROM notification_journal WHERE id=?1", [id])?;
            }
            None => {
                self.conn.execute("DELETE FROM notification_journal", [])?;
            }
        }
        Ok(())
    }
    /// Cross-connection lease. A killed worker cannot block polling indefinitely.
    pub fn claim_poll(&mut self, wall: DateTime<Utc>) -> Result<Option<String>> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT value FROM settings WHERE key='poll_lease'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(value) = previous
            && decode::<DateTime<Utc>>(&value)? > wall
        {
            return Ok(None);
        }
        tx.execute(
            "INSERT INTO settings(key,value) VALUES('poll_lease',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [encode(&(wall + TimeDelta::minutes(10)))?],
        )?;
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let owner = format!(
            "{}:{}:{}",
            std::process::id(),
            wall.timestamp_nanos_opt().unwrap_or_default(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        tx.execute(
            "INSERT INTO settings(key,value) VALUES('poll_owner',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [encode(&owner)?],
        )?;
        tx.commit()?;
        Ok(Some(owner))
    }
    pub fn release_poll(&mut self, owner: &str) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: Option<String> = tx
            .query_row(
                "SELECT value FROM settings WHERE key='poll_owner'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if current.as_deref() == Some(encode(&owner)?.as_str()) {
            tx.execute(
                "DELETE FROM settings WHERE key IN ('poll_lease','poll_owner')",
                [],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn request_manual_refresh(&self, id: &str) -> Result<()> {
        if !self
            .load_book()?
            .state()
            .addresses
            .iter()
            .any(|a| a.id == id)
        {
            return Err(crate::StorageError::AddressMissing);
        }
        self.set_setting("manual_refresh_address", &id)
    }
    pub fn manual_refresh_pending(&self, id: &str) -> Result<bool> {
        Ok(self.setting::<String>("manual_refresh_address")?.as_deref() == Some(id))
    }
    pub fn enqueue_notification(
        &mut self,
        plan: &PlannedNotification,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO notifications(key,address_key,planned_at,payload_json,state,updated_at) VALUES(?1,?2,?3,?4,'planned',?5) ON CONFLICT(key) DO UPDATE SET planned_at=excluded.planned_at,payload_json=excluded.payload_json,state=CASE WHEN notifications.state='cancelled' THEN 'planned' ELSE notifications.state END,updated_at=excluded.updated_at",
            params![
                plan.key, plan.address_key, plan.at.to_rfc3339(), encode(plan) ?, now
                .to_rfc3339()
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn replace_notification_plans(
        &mut self,
        key: &str,
        plans: &[PlannedNotification],
        now: DateTime<Utc>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        replace_in_transaction(&tx, key, plans, now)?;
        tx.commit()?;
        Ok(())
    }
    pub fn set_notification_state(
        &mut self,
        key: &str,
        state: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let old: Option<(i64, String)> = tx
            .query_row(
                "SELECT id,state FROM notifications WHERE key=?1",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((id, previous)) = old
            && previous != state
        {
            tx.execute(
                "UPDATE notifications SET state=?1,updated_at=?2 WHERE key=?3",
                params![state, now.to_rfc3339(), key],
            )?;
            tx.execute(
                "INSERT INTO events(notification_id,state,at) VALUES(?1,?2,?3)",
                params![id, state, now.to_rfc3339()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn cancel_invalid_notification_plans(&mut self, now: DateTime<Utc>) -> Result<()> {
        let book = self.load_book()?.state();
        let prefs = self.notification_preferences()?;
        for record in self.pending_notifications()? {
            if record.plan.kind == NotificationKind::Test {
                continue;
            }
            let address = book.addresses.iter().find(|saved| {
                address_key(saved) == record.plan.address_key && saved.notifications_enabled
            });
            if !prefs.allows_plan(&record.plan) || address.is_none() {
                self.set_notification_state(&record.plan.key, "cancelled", now)?;
            } else if record.plan.expires_at < now {
                self.set_notification_state(&record.plan.key, "expired", now)?;
            }
        }
        Ok(())
    }
    pub fn claim_notification(
        &mut self,
        key: &str,
        now: DateTime<Utc>,
        permission: bool,
    ) -> Result<Option<NotificationRecord>> {
        let Some(record) = self
            .pending_notifications()?
            .into_iter()
            .find(|record| record.plan.key == key)
        else {
            return Ok(None);
        };
        if record.plan.at > now {
            return Ok(None);
        }
        let prefs = self.notification_preferences()?;
        let book = self.load_book()?.state();
        let mut rejection = if record.plan.expires_at < now {
            Some("expired")
        } else if !permission {
            Some("blocked")
        } else {
            None
        };
        if record.plan.kind != NotificationKind::Test {
            let saved = book.addresses.iter().find(|saved| {
                address_key(saved) == record.plan.address_key && saved.notifications_enabled
            });
            if !prefs.allows_plan(&record.plan) || saved.is_none() {
                rejection = Some("cancelled");
            } else if prefs.is_quiet(now) {
                rejection = Some("quiet");
            } else if let Some(snapshot) = self.cached(&record.plan.address_key)? {
                use powerwatch_core::model::SourceStatus;
                if (record.plan.kind == NotificationKind::SourceOutage
                    && snapshot.source_status != SourceStatus::ReportedUnavailable)
                    || (record.plan.kind == NotificationKind::SourceRestored
                        && snapshot.source_status != SourceStatus::ReportedAvailable)
                {
                    rejection = Some("cancelled");
                }
                if matches!(
                    record.plan.kind,
                    NotificationKind::SourceOutage
                        | NotificationKind::SourceRestored
                        | NotificationKind::SourceCleared
                ) && snapshot
                    .address_check
                    .as_ref()
                    .is_some_and(|check| !check.evidence.usable_at(now, TimeDelta::minutes(45)))
                {
                    rejection = Some("stale");
                }
                if record.plan.kind == NotificationKind::SourceCleared
                    && snapshot
                        .address_check
                        .as_ref()
                        .is_none_or(|check| check.event.is_some())
                {
                    rejection = Some("cancelled");
                }
                if !record
                    .plan
                    .fresh_for(&snapshot, now, book.check_interval_minutes)
                {
                    rejection = Some("stale");
                }
                if !record.plan.matches_snapshot(&snapshot, now) {
                    rejection = Some("cancelled");
                }
            } else {
                rejection = Some("cancelled");
            }
        }
        if record.plan.kind == NotificationKind::Warning {
            let newer: Vec<String> = self
                .conn
                .prepare(
                    "SELECT payload_json FROM notifications WHERE address_key=?1 AND planned_at>?2 AND planned_at<=?3 AND state IN ('planned','submitted_unknown')",
                )?
                .query_map(
                    params![
                        record.plan.address_key, record.plan.at.to_rfc3339(), now
                        .to_rfc3339()
                    ],
                    |row| row.get(0),
                )?
                .collect::<std::result::Result<_, _>>()?;
            if newer
                .iter()
                .filter_map(|value| decode::<PlannedNotification>(value).ok())
                .any(|plan| {
                    plan.kind == NotificationKind::Warning
                        && plan.boundary == record.plan.boundary
                        && prefs.allows_plan(&plan)
                })
            {
                rejection = Some("cancelled");
            }
        }
        if let Some(reason) = rejection {
            self.set_notification_state(key, reason, now)?;
            return Ok(None);
        }
        let changed = self
            .conn
            .execute(
                "UPDATE notifications SET state='submitted_unknown',updated_at=?1 WHERE key=?2 AND state='planned'",
                params![now.to_rfc3339(), key],
            )?;
        if changed == 0 {
            return Ok(None);
        }
        self.conn.execute(
            "INSERT INTO events(notification_id,state,at) VALUES(?1,'submitted_unknown',?2)",
            params![record.id, now.to_rfc3339()],
        )?;
        Ok(Some(record))
    }
    pub fn poll_states(&self) -> Result<Vec<PollState>> {
        type PollRow = (
            String,
            String,
            u32,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let rows: Vec<PollRow> = self
            .conn
            .prepare(
                "SELECT address_key,due_at,failures,last_attempt,last_success,error_code FROM poll_state",
            )?
            .query_map(
                [],
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
        rows.into_iter()
            .map(
                |(address_key, due, failures, attempt, success, error_code)| {
                    Ok(PollState {
                        address_key,
                        due_at: instant(&due)?,
                        failures,
                        last_attempt: attempt.map(|value| instant(&value)).transpose()?,
                        last_success: success.map(|value| instant(&value)).transpose()?,
                        error_code,
                    })
                },
            )
            .collect()
    }
    pub fn complete_poll(
        &mut self,
        key: &str,
        finished: DateTime<Utc>,
        due: DateTime<Utc>,
        error: Option<&str>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO poll_state(address_key,due_at,failures,last_attempt,last_success,error_code) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(address_key) DO UPDATE SET due_at=excluded.due_at,failures=CASE WHEN excluded.error_code IS NULL THEN 0 ELSE poll_state.failures+1 END,last_attempt=excluded.last_attempt,last_success=CASE WHEN excluded.error_code IS NULL THEN excluded.last_success ELSE poll_state.last_success END,error_code=excluded.error_code",
            params![
                key, due.to_rfc3339(), u32::from(error.is_some()), finished.to_rfc3339(),
                error.is_none().then(|| finished.to_rfc3339()), error
            ],
        )?;
        tx.execute(
            "INSERT INTO settings(key,value) VALUES('last_poll_finished',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [encode(&finished)?],
        )?;
        tx.execute(
            "DELETE FROM notifications WHERE updated_at<?1 AND state!='planned'",
            [(finished - TimeDelta::days(30)).to_rfc3339()],
        )?;
        tx.commit()?;
        Ok(())
    }
}
/// Called inside the snapshot transaction so a crash cannot retain a new
/// forecast with the old reminder plan (or vice versa).
pub(crate) fn replace_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    key: &str,
    plans: &[PlannedNotification],
    now: DateTime<Utc>,
) -> Result<()> {
    let snapshot: Option<String> = tx
        .query_row(
            "SELECT snapshot_json FROM latest_snapshots WHERE address_key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()?;
    let snapshot = snapshot
        .map(|value| decode::<powerwatch_core::model::ScheduleSnapshot>(&value))
        .transpose()?;
    let prefs: Option<String> = tx
        .query_row(
            "SELECT value FROM settings WHERE key='notification_preferences'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let prefs: NotificationPreferences = prefs
        .map(|value| decode(&value))
        .transpose()?
        .unwrap_or_default();
    let rows: Vec<(i64, String)> = tx
        .prepare(
            "SELECT id,payload_json FROM notifications WHERE state='planned' AND address_key=?1",
        )?
        .query_map([key], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    for (id, json) in rows {
        let old: PlannedNotification = decode(&json)?;
        let boundary = matches!(
            old.kind,
            NotificationKind::Warning
                | NotificationKind::ForecastStart
                | NotificationKind::ForecastEnd
        );
        let superseded = !boundary
            && plans
                .iter()
                .any(|plan| plan.kind == old.kind && plan.key != old.key);
        let due_valid = old.at <= now
            && now <= old.expires_at
            && prefs.allows_plan(&old)
            && snapshot
                .as_ref()
                .is_some_and(|snapshot| old.matches_snapshot(snapshot, now));
        if (boundary && !plans.iter().any(|plan| plan.key == old.key) && !due_valid) || superseded {
            tx.execute(
                "UPDATE notifications SET state='cancelled',updated_at=?1 WHERE id=?2",
                params![now.to_rfc3339(), id],
            )?;
            tx.execute(
                "INSERT INTO events(notification_id,state,at) VALUES(?1,'cancelled',?2)",
                params![id, now.to_rfc3339()],
            )?;
        }
    }
    for plan in plans {
        tx.execute(
            "INSERT INTO notifications(key,address_key,planned_at,payload_json,state,updated_at) VALUES(?1,?2,?3,?4,'planned',?5) ON CONFLICT(key) DO UPDATE SET planned_at=excluded.planned_at,payload_json=excluded.payload_json,state=CASE WHEN notifications.state='cancelled' THEN 'planned' ELSE notifications.state END,updated_at=excluded.updated_at",
            params![
                plan.key, plan.address_key, plan.at.to_rfc3339(), encode(plan) ?, now
                .to_rfc3339()
            ],
        )?;
    }
    Ok(())
}
