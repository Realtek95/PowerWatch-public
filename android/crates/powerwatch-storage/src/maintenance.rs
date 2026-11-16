use crate::{Result, Store, encode};
use chrono::{DateTime, TimeDelta, Utc};
impl Store {
    /// Daily retention also runs when all sources are offline. SQLite normally
    /// reuses free pages, but does not return them to Android's storage counter.
    /// Compact only a substantial free fraction, never on each short UI tick.
    pub fn maintain(&mut self, now: DateTime<Utc>) -> Result<bool> {
        if self
            .setting::<DateTime<Utc>>("last_storage_maintenance")?
            .is_some_and(|last| last <= now && now - last < TimeDelta::days(1))
        {
            return Ok(false);
        }
        self.reconcile(now)?;
        let tx = self.conn.transaction()?;
        let cutoff = (now - TimeDelta::days(30)).to_rfc3339();
        tx.execute("DELETE FROM snapshots WHERE received_at<?1", [&cutoff])?;
        tx.execute(
            "DELETE FROM schedule_changes WHERE detected_at<?1",
            [&cutoff],
        )?;
        tx.execute(
            "DELETE FROM notifications WHERE updated_at<?1 AND state!='planned'",
            [&cutoff],
        )?;
        tx.execute(
            "INSERT INTO settings(key,value) VALUES('last_storage_maintenance',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [encode(&now)?],
        )?;
        tx.commit()?;
        let page_size: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        let pages: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let free: i64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        if free * page_size >= 8 * 1024 * 1024 && free >= pages / 4 {
            self.conn.execute_batch("VACUUM;")?;
        }
        Ok(true)
    }
}
