use crate::{Result, StorageError, Store};
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
impl Store {
    pub fn http_pause(&self, host: &str) -> Result<Option<DateTime<Utc>>> {
        self.setting::<i64>(&format!("http_pause:{host}"))?
            .map(|ms| DateTime::from_timestamp_millis(ms).ok_or(StorageError::InvalidTime))
            .transpose()
    }
    /// One SQLite statement is atomic across connections/processes. A shorter
    /// concurrent response never clears or shortens a longer host pause.
    pub fn postpone_http(&self, host: &str, until: DateTime<Utc>) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=max(CAST(settings.value AS INTEGER),CAST(excluded.value AS INTEGER))",
                params![
                    format!("http_pause:{host}"), until.timestamp_millis().to_string()
                ],
            )?;
        Ok(())
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct HttpSlot {
    owner: Option<String>,
    until: DateTime<Utc>,
}
impl Store {
    /// Atomically reserve one host across independent connections/processes.
    /// A completed request leaves a cooldown; a killed process leaves a bounded lease.
    pub fn claim_http(
        &mut self,
        host: &str,
        owner: &str,
        now: DateTime<Utc>,
        expires: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let key = format!("http_slot:{host}");
        let previous: Option<String> = tx
            .query_row("SELECT value FROM settings WHERE key=?1", [&key], |row| {
                row.get(0)
            })
            .optional()?;
        if let Some(previous) = previous {
            let slot: HttpSlot = crate::decode(&previous)?;
            if slot.until > now {
                return Ok(Some(slot.until));
            }
        }
        let slot = HttpSlot {
            owner: Some(owner.into()),
            until: expires,
        };
        tx.execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, crate ::encode(& slot) ?],
        )?;
        tx.commit()?;
        Ok(None)
    }
    /// An expired/stale owner must never release a newer request's reservation.
    pub fn finish_http(&mut self, host: &str, owner: &str, next: DateTime<Utc>) -> Result<bool> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let key = format!("http_slot:{host}");
        let previous: Option<String> = tx
            .query_row("SELECT value FROM settings WHERE key=?1", [&key], |row| {
                row.get(0)
            })
            .optional()?;
        let Some(previous) = previous else {
            return Ok(false);
        };
        let slot: HttpSlot = crate::decode(&previous)?;
        if slot.owner.as_deref() != Some(owner) {
            return Ok(false);
        }
        let slot = HttpSlot {
            owner: None,
            until: next,
        };
        tx.execute(
            "UPDATE settings SET value=?2 WHERE key=?1",
            params![key, crate::encode(&slot)?],
        )?;
        tx.commit()?;
        Ok(true)
    }
}
