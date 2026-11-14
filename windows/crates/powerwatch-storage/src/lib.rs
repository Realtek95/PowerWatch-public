//! Local, source-partitioned SQLite repository. All writes use transactions;
//! normalized data is validated before replacing a trusted snapshot.
mod archive;
mod http_policy;
mod notifications;
pub use archive::{ArchiveView, ChangeRecord, HistoryAddress};
use chrono::{DateTime, Utc};
pub use notifications::{NotificationRecord, PollState};
use powerwatch_core::{
    model::{Address, ScheduleSnapshot, normalize_day},
    session::{AddressBook, SavedAddress},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
use thiserror::Error;
pub type Result<T> = std::result::Result<T, StorageError>;
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("stored JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid schedule: {0}")]
    Core(#[from] powerwatch_core::CoreError),
    #[error("unsupported database schema")]
    Schema,
    #[error("address is not stored")]
    AddressMissing,
    #[error("invalid stored timestamp")]
    InvalidTime,
}
pub struct Store {
    pub(crate) conn: Connection,
}
pub fn address_key(saved: &SavedAddress) -> String {
    format!(
        "{}:{}{}:{}{}:{}",
        saved.id.len(),
        saved.id,
        saved.address.provider_id.len(),
        saved.address.provider_id,
        saved.address.id.len(),
        saved.address.id
    )
}
pub(crate) fn encode<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
pub(crate) fn decode<T: DeserializeOwned>(value: &str) -> Result<T> {
    Ok(serde_json::from_str(value)?)
}
pub(crate) fn instant(value: &str) -> Result<DateTime<Utc>> {
    value.parse().map_err(|_| StorageError::InvalidTime)
}
impl Store {
    pub fn path(&self) -> Option<std::path::PathBuf> {
        self.conn
            .path()
            .filter(|path| !path.is_empty())
            .map(std::path::PathBuf::from)
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(mut conn: Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if ![0, 1].contains(&version) {
            return Err(StorageError::Schema);
        }
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let tx = conn.transaction()?;
        if version == 0 {
            tx.execute_batch(include_str!("schema.sql"))?;
        }
        tx.commit()?;
        Ok(Self { conn })
    }
    pub fn setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let json: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        json.map(|value| decode(&value)).transpose()
    }
    pub fn settings_with_prefix<T: DeserializeOwned>(&self, prefix: &str) -> Result<Vec<T>> {
        self.conn
            .prepare("SELECT value FROM settings WHERE substr(key,1,length(?1))=?1 ORDER BY key")?
            .query_map([prefix], |row| row.get::<_, String>(0))?
            .map(|value| decode(&value?))
            .collect()
    }
    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, encode(value) ?],
            )?;
        Ok(())
    }
    pub fn load_book(&self) -> Result<AddressBook> {
        Ok(self.setting("address_book")?.unwrap_or_default())
    }
    pub fn save_book(&mut self, book: &AddressBook) -> Result<()> {
        self.save_book_with_purge(book, None)
    }
    pub fn save_book_with_purge(
        &mut self,
        book: &AddressBook,
        purge_key: Option<&str>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO settings(key,value) VALUES('address_book',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [encode(book)?],
        )?;
        tx.execute("UPDATE addresses SET retired=1", [])?;
        for saved in &book.state().addresses {
            tx.execute(
                "INSERT INTO addresses(key,local_id,provider_id,source_id,address_json,retired) VALUES(?1,?2,?3,?4,?5,0) ON CONFLICT(key) DO UPDATE SET address_json=excluded.address_json,retired=0",
                params![
                    address_key(saved), saved.id, saved.address.provider_id, saved
                    .address.id, encode(& saved.address) ?
                ],
            )?;
        }
        if let Some(key) = purge_key {
            tx.execute(
                "DELETE FROM settings WHERE key='manual_refresh_address' AND value IN (SELECT json_quote(local_id) FROM addresses WHERE key=?1)",
                [key],
            )?;
            for table in [
                "snapshots",
                "latest_snapshots",
                "daily_candidates",
                "schedule_intervals",
                "schedule_changes",
                "daily_schedule_archive",
                "notifications",
                "notification_journal",
                "poll_state",
            ] {
                tx.execute(
                    &format!(
                        "DELETE FROM {table} WHERE address_key IN (SELECT key FROM addresses WHERE local_id=(SELECT local_id FROM addresses WHERE key=?1))"
                    ),
                    [key],
                )?;
            }
            tx.execute(
                "DELETE FROM addresses WHERE retired=1 AND local_id=(SELECT local_id FROM addresses WHERE key=?1)",
                [key],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn purge_history(&mut self, key: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        for table in [
            "snapshots",
            "latest_snapshots",
            "daily_candidates",
            "schedule_intervals",
            "schedule_changes",
            "daily_schedule_archive",
            "notifications",
            "notification_journal",
            "poll_state",
        ] {
            tx.execute(&format!("DELETE FROM {table} WHERE address_key=?1"), [key])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn cached(&self, key: &str) -> Result<Option<ScheduleSnapshot>> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT snapshot_json FROM latest_snapshots WHERE address_key=?1",
                [key],
                |row| row.get(0),
            )
            .optional()?;
        value.map(|json| decode(&json)).transpose()
    }
    /// Only source-confirmed automatic queues may change during a check. The
    /// book, current address metadata, snapshot and reminders commit together.
    fn source_queue_update(
        tx: &rusqlite::Transaction<'_>,
        key: &str,
        address: &Address,
        snapshot: &mut ScheduleSnapshot,
        now: DateTime<Utc>,
    ) -> Result<Option<AddressBook>> {
        if address.queue == snapshot.address.queue
            || address
                .details
                .as_ref()
                .is_none_or(|details| details.queue.is_none())
        {
            return Ok(None);
        }
        let details = address.details.as_ref().ok_or(StorageError::Schema)?;
        let check = snapshot
            .address_check
            .as_ref()
            .ok_or(StorageError::Schema)?;
        let operator = details
            .operator
            .as_ref()
            .or_else(|| {
                details
                    .selection
                    .as_ref()
                    .map(|selection| &selection.operator)
            })
            .ok_or(StorageError::Schema)?;
        let queue = check.queue.as_ref().ok_or(StorageError::Schema)?;
        if check.evidence.provider.kind != powerwatch_core::grid::ProviderKind::Operator
            || check.binding.provider_id != check.evidence.provider.id
            || check.catalog.provider_id != check.binding.provider_id
            || check.binding.checked_at > now
            || !check
                .evidence
                .usable_at(now, chrono::TimeDelta::minutes(45))
            || check.evidence.scope
                != (powerwatch_core::grid::SourceScope::Address {
                    operator_id: operator.id.clone(),
                    source_id: check.binding.source_id.clone(),
                })
            || queue.operator_id != operator.id
            || snapshot.address.queue.as_deref() != Some(queue.code.as_str())
        {
            return Err(StorageError::Schema);
        }
        let mut book: AddressBook = decode(&tx.query_row(
            "SELECT value FROM settings WHERE key='address_book'",
            [],
            |row| row.get::<_, String>(0),
        )?)?;
        let id = book
            .state()
            .addresses
            .iter()
            .find(|entry| address_key(entry) == key)
            .ok_or(StorageError::AddressMissing)?
            .id
            .clone();
        book.confirm_address(
            &id,
            powerwatch_core::grid::AddressVerification {
                operator: operator.clone(),
                binding: check.binding.clone(),
                catalog: check.catalog.clone(),
                queue: check.queue.clone(),
            },
        )
        .map_err(|_| StorageError::Schema)?;
        snapshot.address = book
            .get(&id)
            .map_err(|_| StorageError::AddressMissing)?
            .address
            .clone();
        Ok(Some(book))
    }
    pub fn ingest(
        &mut self,
        key: &str,
        snapshot: ScheduleSnapshot,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.ingest_with_plans(key, snapshot, now, None)
    }
    /// Persist a validated forecast and its rebuilt outbox in one transaction.
    pub fn ingest_with_plans(
        &mut self,
        key: &str,
        mut snapshot: ScheduleSnapshot,
        now: DateTime<Utc>,
        plans: Option<&[powerwatch_notifications::PlannedNotification]>,
    ) -> Result<()> {
        for day in &mut snapshot.days {
            *day = normalize_day(day.date, &day.intervals)?;
        }
        snapshot.days.sort_by_key(|day| day.date);
        if let Some(days) = &mut snapshot.queue_days {
            for day in days.iter_mut() {
                *day = normalize_day(day.date, &day.intervals)?;
            }
            days.sort_by_key(|day| day.date);
            if days.windows(2).any(|pair| pair[0].date == pair[1].date) {
                return Err(StorageError::Schema);
            }
        }
        if snapshot
            .days
            .windows(2)
            .any(|pair| pair[0].date == pair[1].date)
        {
            return Err(StorageError::Schema);
        }
        let metadata: Option<String> = self
            .conn
            .query_row(
                "SELECT address_json FROM addresses WHERE key=?1",
                [key],
                |row| row.get(0),
            )
            .optional()?;
        let address: Address = decode(&metadata.ok_or(StorageError::AddressMissing)?)?;
        if address.provider_id != snapshot.provider_id || address.id != snapshot.address.id {
            return Err(StorageError::AddressMissing);
        }
        self.reconcile(now)?;
        let previous = self.cached(key)?;
        let today = now
            .with_timezone(&powerwatch_core::time::LOCAL_ZONE)
            .date_naive();
        let tx = self.conn.transaction()?;
        let updated_book = Self::source_queue_update(&tx, key, &address, &mut snapshot, now)?;
        let changes = previous
            .as_ref()
            .map(|old| powerwatch_core::history::compare(old, &snapshot))
            .unwrap_or_default();
        let json = encode(&snapshot)?;
        if let Some(book) = updated_book {
            tx.execute(
                "UPDATE settings SET value=?1 WHERE key='address_book'",
                [encode(&book)?],
            )?;
            tx.execute(
                "UPDATE addresses SET address_json=?1 WHERE key=?2",
                params![encode(&snapshot.address)?, key],
            )?;
        }
        tx.execute(
            "INSERT INTO snapshots(address_key,signature,received_at,snapshot_json) VALUES(?1,?2,?3,?4) ON CONFLICT(address_key,signature) DO UPDATE SET received_at=excluded.received_at,snapshot_json=excluded.snapshot_json",
            params![
                key, powerwatch_core::history::signature(& snapshot) ?, snapshot
                .received_at.to_rfc3339(), json
            ],
        )?;
        if !changes.is_empty()
            && let Some(old) = previous
        {
            tx.execute(
                "INSERT INTO schedule_changes(address_key,detected_at,published_at,changes_json,before_json,after_json) VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    key, now.to_rfc3339(), snapshot.published_at.map(| at | at
                    .to_rfc3339()), encode(& changes) ?, encode(& old) ?, json
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO latest_snapshots(address_key,snapshot_json) VALUES(?1,?2) ON CONFLICT(address_key) DO UPDATE SET snapshot_json=excluded.snapshot_json",
            params![key, json],
        )?;
        for day in &snapshot.days {
            if day.date < today {
                continue;
            }
            let (start, end) = powerwatch_core::time::day_bounds(day.date)?;
            let candidate = archive::DailyCandidate {
                day: day.clone(),
                provenance: snapshot.provenance.as_ref().map(|parts| {
                    parts
                        .iter()
                        .filter(|part| part.start < end && part.end > start)
                        .map(|part| {
                            let mut value = part.clone();
                            value.start = value.start.max(start);
                            value.end = value.end.min(end);
                            value
                        })
                        .collect()
                }),
            };
            tx.execute(
                "INSERT INTO daily_candidates(address_key,date,received_at,published_at,day_json) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(address_key,date) DO UPDATE SET received_at=excluded.received_at,published_at=excluded.published_at,day_json=excluded.day_json",
                params![
                    key, day.date.to_string(), snapshot.received_at.to_rfc3339(),
                    snapshot.published_at.map(| at | at.to_rfc3339()), encode(&
                    candidate) ?
                ],
            )?;
            tx.execute(
                "DELETE FROM schedule_intervals WHERE address_key=?1 AND date=?2",
                params![key, day.date.to_string()],
            )?;
            for item in &day.intervals {
                tx.execute(
                    "INSERT INTO schedule_intervals(address_key,date,start_utc,end_utc,state) VALUES(?1,?2,?3,?4,?5)",
                    params![
                        key, day.date.to_string(), item.start.to_rfc3339(), item.end
                        .to_rfc3339(), encode(& item.state) ?
                    ],
                )?;
            }
        }
        tx.execute(
            "DELETE FROM schedule_changes WHERE detected_at<?1",
            [(now - chrono::TimeDelta::days(30)).to_rfc3339()],
        )?;
        tx.execute(
            "DELETE FROM snapshots WHERE received_at<?1",
            [(now - chrono::TimeDelta::days(30)).to_rfc3339()],
        )?;
        if let Some(plans) = plans {
            notifications::replace_in_transaction(&tx, key, plans, now)?;
        }
        tx.commit()?;
        Ok(())
    }
}
