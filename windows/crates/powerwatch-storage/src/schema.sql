PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS addresses (key TEXT PRIMARY KEY, local_id TEXT NOT NULL, provider_id TEXT NOT NULL, source_id TEXT NOT NULL, address_json TEXT NOT NULL, retired INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS snapshots (id INTEGER PRIMARY KEY, address_key TEXT NOT NULL REFERENCES addresses(key) ON DELETE CASCADE, signature TEXT NOT NULL, received_at TEXT NOT NULL, snapshot_json TEXT NOT NULL, UNIQUE(address_key,signature));
CREATE TABLE IF NOT EXISTS latest_snapshots (address_key TEXT PRIMARY KEY REFERENCES addresses(key) ON DELETE CASCADE, snapshot_json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS daily_candidates (address_key TEXT NOT NULL REFERENCES addresses(key) ON DELETE CASCADE, date TEXT NOT NULL, received_at TEXT NOT NULL, published_at TEXT, day_json TEXT NOT NULL, PRIMARY KEY(address_key,date));
CREATE TABLE IF NOT EXISTS schedule_intervals (address_key TEXT NOT NULL REFERENCES addresses(key) ON DELETE CASCADE, date TEXT NOT NULL, start_utc TEXT NOT NULL, end_utc TEXT NOT NULL, state TEXT NOT NULL, PRIMARY KEY(address_key,date,start_utc));
CREATE TABLE IF NOT EXISTS schedule_changes (id INTEGER PRIMARY KEY, address_key TEXT NOT NULL REFERENCES addresses(key) ON DELETE CASCADE, detected_at TEXT NOT NULL, published_at TEXT, changes_json TEXT NOT NULL, before_json TEXT NOT NULL, after_json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS daily_schedule_archive (address_key TEXT NOT NULL REFERENCES addresses(key) ON DELETE CASCADE, date TEXT NOT NULL, archive_json TEXT NOT NULL, PRIMARY KEY(address_key,date));
CREATE INDEX IF NOT EXISTS changes_address_time ON schedule_changes(address_key,detected_at);
PRAGMA user_version = 2;
CREATE TABLE notifications (id INTEGER PRIMARY KEY, key TEXT NOT NULL UNIQUE, address_key TEXT NOT NULL REFERENCES addresses(key) ON DELETE CASCADE, planned_at TEXT NOT NULL, payload_json TEXT NOT NULL, state TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE events (id INTEGER PRIMARY KEY, notification_id INTEGER NOT NULL REFERENCES notifications(id) ON DELETE CASCADE, state TEXT NOT NULL, at TEXT NOT NULL);
CREATE TABLE poll_state (address_key TEXT PRIMARY KEY REFERENCES addresses(key) ON DELETE CASCADE, due_at TEXT NOT NULL, failures INTEGER NOT NULL DEFAULT 0, last_attempt TEXT, last_success TEXT, error_code TEXT);
CREATE INDEX notifications_due ON notifications(state,planned_at);
-- Visible history is deliberately independent of active plans and dedup keys.
CREATE TABLE notification_journal (id INTEGER PRIMARY KEY AUTOINCREMENT, key TEXT NOT NULL UNIQUE, address_key TEXT NOT NULL, payload_json TEXT NOT NULL, state TEXT NOT NULL, updated_at TEXT NOT NULL);
