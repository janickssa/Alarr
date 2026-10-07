-- Quorum initial schema. All timestamps are Unix seconds (UTC).

CREATE TABLE users (
    id               INTEGER PRIMARY KEY,
    username         TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    password_hash    TEXT    NOT NULL,
    role             TEXT    NOT NULL CHECK (role IN ('owner', 'member', 'guest')),
    -- Guests only: JSON array of service ids the guest may see.
    allowed_services TEXT    NOT NULL DEFAULT '[]',
    -- Guests only: may run power and service actions on allowed services.
    can_act          INTEGER NOT NULL DEFAULT 0,
    created_at       INTEGER NOT NULL
);

-- Web sessions. Only the SHA-256 of the cookie value is stored.
CREATE TABLE sessions (
    token_hash   TEXT    PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);

-- Paired devices (PWA, iOS). Only the SHA-256 of the bearer token is stored.
CREATE TABLE devices (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    platform     TEXT    NOT NULL,
    token_hash   TEXT    NOT NULL UNIQUE,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL
);

-- Short-lived pairing codes. Only the SHA-256 of the code is stored.
CREATE TABLE pairing_codes (
    code_hash  TEXT    PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_by INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);

-- Service instances. `secrets` is a JSON object of name -> encrypted value.
CREATE TABLE services (
    id          TEXT    PRIMARY KEY,
    kind        TEXT    NOT NULL,
    preset      TEXT,
    name        TEXT    NOT NULL,
    base_url    TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    category    TEXT,
    settings    TEXT    NOT NULL DEFAULT '{}',
    secrets     TEXT    NOT NULL DEFAULT '{}',
    linked_vmid INTEGER,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE activity_log (
    id       INTEGER PRIMARY KEY,
    ts       INTEGER NOT NULL,
    user_id  INTEGER,
    username TEXT,
    action   TEXT    NOT NULL,
    target   TEXT    NOT NULL,
    result   TEXT    NOT NULL CHECK (result IN ('ok', 'error', 'pending')),
    detail   TEXT
);
CREATE INDEX activity_log_ts ON activity_log (ts DESC);

CREATE TABLE alert_settings (
    alert_type  TEXT    PRIMARY KEY,
    enabled     INTEGER NOT NULL DEFAULT 1,
    channels    TEXT    NOT NULL DEFAULT '[]',
    quiet_hours TEXT
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Uptime at 1 minute resolution, kept 7 days. `state`: 2 healthy, 1 warning, 0 down.
-- `target` is `svc:<service id>` or `guest:<vmid>`.
CREATE TABLE uptime_minutes (
    target      TEXT    NOT NULL,
    minute      INTEGER NOT NULL,
    state       INTEGER NOT NULL,
    response_ms INTEGER,
    PRIMARY KEY (target, minute)
) WITHOUT ROWID;

-- Older samples downsampled to hours, kept 90 days.
CREATE TABLE uptime_hours (
    target          TEXT    NOT NULL,
    hour            INTEGER NOT NULL,
    healthy_minutes INTEGER NOT NULL,
    warning_minutes INTEGER NOT NULL,
    down_minutes    INTEGER NOT NULL,
    avg_response_ms INTEGER,
    PRIMARY KEY (target, hour)
) WITHOUT ROWID;
