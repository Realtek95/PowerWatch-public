CREATE TABLE IF NOT EXISTS notification_baselines (address_key TEXT PRIMARY KEY REFERENCES addresses(key) ON DELETE CASCADE, baseline_json TEXT NOT NULL);
DROP TRIGGER IF EXISTS notification_journal_insert;
DROP TRIGGER IF EXISTS notification_journal_update;
DELETE FROM notification_journal WHERE state IN ('planned','cancelled');
CREATE TRIGGER notification_journal_insert AFTER INSERT ON notifications
WHEN NEW.state NOT IN ('planned','cancelled') BEGIN
  INSERT INTO notification_journal(key,address_key,payload_json,state,updated_at) VALUES(NEW.key,NEW.address_key,NEW.payload_json,NEW.state,NEW.updated_at)
    ON CONFLICT(key) DO UPDATE SET payload_json=excluded.payload_json,state=excluded.state,updated_at=excluded.updated_at;
  DELETE FROM notification_journal WHERE id NOT IN (SELECT id FROM notification_journal ORDER BY updated_at DESC,id DESC LIMIT 25);
END;
CREATE TRIGGER notification_journal_update AFTER UPDATE ON notifications
WHEN (OLD.state != NEW.state OR OLD.payload_json != NEW.payload_json) AND NEW.state NOT IN ('planned','cancelled') BEGIN
  INSERT INTO notification_journal(key,address_key,payload_json,state,updated_at) VALUES(NEW.key,NEW.address_key,NEW.payload_json,NEW.state,NEW.updated_at)
    ON CONFLICT(key) DO UPDATE SET payload_json=excluded.payload_json,state=excluded.state,updated_at=excluded.updated_at;
  DELETE FROM notification_journal WHERE id NOT IN (SELECT id FROM notification_journal ORDER BY updated_at DESC,id DESC LIMIT 25);
END;
PRAGMA user_version = 2;
