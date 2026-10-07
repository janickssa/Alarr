-- Alerts engine (phase 4): alert history with deduplication, and per-user/device mutes.
-- `alert_settings` (per type: enabled, channels, quiet hours) is in 0001.

CREATE TABLE alerts (
    id          INTEGER PRIMARY KEY,
    alert_type  TEXT    NOT NULL,
    -- Deduplication key, e.g. `service_down:sonarr`. One unresolved alert per key.
    key         TEXT    NOT NULL,
    service_id  TEXT,
    severity    TEXT    NOT NULL CHECK (severity IN ('critical', 'warning', 'info')),
    title       TEXT    NOT NULL,
    message     TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    resolved_at INTEGER,
    -- JSON array of {channel, result} for each delivery attempt.
    delivery    TEXT    NOT NULL DEFAULT '[]'
);
CREATE INDEX alerts_created ON alerts (created_at DESC);
CREATE INDEX alerts_key ON alerts (key);
CREATE UNIQUE INDEX alerts_active_key ON alerts (key) WHERE resolved_at IS NULL;

-- Alert types a user (device_id NULL) or one of their devices does not want to hear about.
-- Applies to in-app alerts and device push; ntfy topics are shared and ignore mutes.
CREATE TABLE alert_mutes (
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    device_id  INTEGER REFERENCES devices (id) ON DELETE CASCADE,
    alert_type TEXT    NOT NULL
);
CREATE UNIQUE INDEX alert_mutes_unique ON alert_mutes (user_id, IFNULL(device_id, 0), alert_type);
