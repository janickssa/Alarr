//! SQLite storage (sqlx, migrations in `backend/migrations`).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::str::FromStr;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{FromRow, SqlitePool};
use utoipa::ToSchema;

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Member,
    Guest,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Member => "member",
            Self::Guest => "guest",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "owner" => Self::Owner,
            "member" => Self::Member,
            _ => Self::Guest,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: Role,
    /// Guests only: service ids the guest may see.
    pub allowed_services: Vec<String>,
    /// Guests only: may run power and service actions on allowed services.
    pub can_act: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(FromRow)]
struct UserRow {
    id: i64,
    username: String,
    password_hash: String,
    role: String,
    allowed_services: String,
    can_act: i64,
    created_at: i64,
}

impl UserRow {
    fn into_user(self) -> (User, String) {
        (
            User {
                id: self.id,
                username: self.username,
                role: Role::parse(&self.role),
                allowed_services: serde_json::from_str(&self.allowed_services).unwrap_or_default(),
                can_act: self.can_act != 0,
                created_at: ts(self.created_at),
            },
            self.password_hash,
        )
    }
}

pub fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs, 0).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Device {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub platform: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
}

#[derive(FromRow)]
struct DeviceRow {
    id: i64,
    user_id: i64,
    name: String,
    platform: String,
    created_at: i64,
    last_seen_at: i64,
}

impl From<DeviceRow> for Device {
    fn from(r: DeviceRow) -> Self {
        Self {
            id: r.id,
            user_id: r.user_id,
            name: r.name,
            platform: r.platform,
            created_at: ts(r.created_at),
            last_seen_at: ts(r.last_seen_at),
        }
    }
}

/// A stored service instance. `secrets` holds sealed (encrypted) values.
#[derive(Debug, Clone)]
pub struct ServiceRow {
    pub id: String,
    pub kind: String,
    pub preset: Option<String>,
    pub name: String,
    pub base_url: Option<String>,
    pub enabled: bool,
    pub category: Option<String>,
    pub settings: Value,
    pub secrets: BTreeMap<String, String>,
    pub linked_vmid: Option<u32>,
    pub sort_order: i64,
}

#[derive(FromRow)]
struct ServiceDbRow {
    id: String,
    kind: String,
    preset: Option<String>,
    name: String,
    base_url: Option<String>,
    enabled: i64,
    category: Option<String>,
    settings: String,
    secrets: String,
    linked_vmid: Option<i64>,
    sort_order: i64,
}

impl From<ServiceDbRow> for ServiceRow {
    fn from(r: ServiceDbRow) -> Self {
        Self {
            id: r.id,
            kind: r.kind,
            preset: r.preset,
            name: r.name,
            base_url: r.base_url,
            enabled: r.enabled != 0,
            category: r.category,
            settings: serde_json::from_str(&r.settings)
                .unwrap_or(Value::Object(Default::default())),
            secrets: serde_json::from_str(&r.secrets).unwrap_or_default(),
            linked_vmid: r.linked_vmid.and_then(|v| u32::try_from(v).ok()),
            sort_order: r.sort_order,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ActivityResult {
    Ok,
    Error,
    Pending,
}

impl ActivityResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Pending => "pending",
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActivityEntry {
    pub id: i64,
    pub at: chrono::DateTime<chrono::Utc>,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    /// Machine-readable action, e.g. `guest.power.reboot`, `service.create`, `auth.login`.
    pub action: String,
    /// What it acted on, e.g. `guest:123`, `service:sonarr`.
    pub target: String,
    pub result: ActivityResult,
    pub detail: Option<String>,
}

#[derive(FromRow)]
struct ActivityRow {
    id: i64,
    ts: i64,
    user_id: Option<i64>,
    username: Option<String>,
    action: String,
    target: String,
    result: String,
    detail: Option<String>,
}

impl From<ActivityRow> for ActivityEntry {
    fn from(r: ActivityRow) -> Self {
        Self {
            id: r.id,
            at: ts(r.ts),
            user_id: r.user_id,
            username: r.username,
            action: r.action,
            target: r.target,
            result: match r.result.as_str() {
                "ok" => ActivityResult::Ok,
                "pending" => ActivityResult::Pending,
                _ => ActivityResult::Error,
            },
            detail: r.detail,
        }
    }
}

/// Uptime counts for one target.
#[derive(Debug, Clone, Default)]
pub struct UptimeStats {
    pub up_24h: i64,
    pub total_24h: i64,
    pub up_7d: i64,
    pub total_7d: i64,
    /// Hour start (Unix seconds) -> (worst state, up minutes, total minutes).
    pub hours: HashMap<i64, (i64, i64, i64)>,
}

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let options = SqliteConnectOptions::from_str("sqlite://")?
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .with_context(|| format!("opening database {}", path.display()))?;
        Self::migrate(pool).await
    }

    /// A private in-memory database (demo mode, tests).
    pub async fn in_memory() -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await?;
        Self::migrate(pool).await
    }

    async fn migrate(pool: SqlitePool) -> anyhow::Result<Self> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    // ---- users ----

    pub async fn count_users(&self) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?)
    }

    pub async fn count_owners(&self) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'owner'")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        role: Role,
        allowed_services: &[String],
        can_act: bool,
    ) -> anyhow::Result<i64> {
        let id = sqlx::query(
            "INSERT INTO users (username, password_hash, role, allowed_services, can_act, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(username)
        .bind(password_hash)
        .bind(role.as_str())
        .bind(serde_json::to_string(allowed_services)?)
        .bind(can_act)
        .bind(now())
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        Ok(id)
    }

    pub async fn user(&self, id: i64) -> anyhow::Result<Option<User>> {
        Ok(
            sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .map(|r| r.into_user().0),
        )
    }

    /// The user and their password hash.
    pub async fn user_by_name(&self, username: &str) -> anyhow::Result<Option<(User, String)>> {
        Ok(
            sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE username = ?")
                .bind(username)
                .fetch_optional(&self.pool)
                .await?
                .map(UserRow::into_user),
        )
    }

    pub async fn password_hash(&self, id: i64) -> anyhow::Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        Ok(
            sqlx::query_as::<_, UserRow>("SELECT * FROM users ORDER BY id")
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| r.into_user().0)
                .collect(),
        )
    }

    pub async fn update_user(
        &self,
        id: i64,
        role: Role,
        allowed_services: &[String],
        can_act: bool,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE users SET role = ?, allowed_services = ?, can_act = ? WHERE id = ?")
            .bind(role.as_str())
            .bind(serde_json::to_string(allowed_services)?)
            .bind(can_act)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_password(&self, id: i64, password_hash: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(password_hash)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_user(&self, id: i64) -> anyhow::Result<bool> {
        Ok(sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected()
            > 0)
    }

    // ---- sessions ----

    pub async fn create_session(
        &self,
        token_hash: &str,
        user_id: i64,
        ttl: i64,
    ) -> anyhow::Result<()> {
        let now = now();
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, last_seen_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(token_hash)
        .bind(user_id)
        .bind(now)
        .bind(now + ttl)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The user of a valid session; slides the expiry forward (at most once a minute).
    pub async fn session_user(&self, token_hash: &str, ttl: i64) -> anyhow::Result<Option<User>> {
        let now = now();
        let user_id: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM sessions WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        let Some(user_id) = user_id else {
            return Ok(None);
        };
        sqlx::query(
            "UPDATE sessions SET last_seen_at = ?, expires_at = ?
             WHERE token_hash = ? AND last_seen_at < ?",
        )
        .bind(now)
        .bind(now + ttl)
        .bind(token_hash)
        .bind(now - 60)
        .execute(&self.pool)
        .await?;
        self.user(user_id).await
    }

    pub async fn delete_session(&self, token_hash: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Remove a user's sessions, optionally keeping one (the caller's).
    pub async fn delete_user_sessions(
        &self,
        user_id: i64,
        keep: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash IS NOT ?")
            .bind(user_id)
            .bind(keep)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---- devices & pairing ----

    pub async fn create_device(
        &self,
        user_id: i64,
        name: &str,
        platform: &str,
        token_hash: &str,
    ) -> anyhow::Result<Device> {
        let now = now();
        let id = sqlx::query(
            "INSERT INTO devices (user_id, name, platform, token_hash, created_at, last_seen_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(name)
        .bind(platform)
        .bind(token_hash)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        Ok(Device {
            id,
            user_id,
            name: name.to_owned(),
            platform: platform.to_owned(),
            created_at: ts(now),
            last_seen_at: ts(now),
        })
    }

    /// The device and its user for a bearer token; updates `last_seen_at`.
    pub async fn device_user(&self, token_hash: &str) -> anyhow::Result<Option<(Device, User)>> {
        let row = sqlx::query_as::<_, DeviceRow>(
            "SELECT id, user_id, name, platform, created_at, last_seen_at FROM devices WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let now = now();
        if row.last_seen_at < now - 60 {
            sqlx::query("UPDATE devices SET last_seen_at = ? WHERE id = ?")
                .bind(now)
                .bind(row.id)
                .execute(&self.pool)
                .await?;
        }
        let user = self.user(row.user_id).await?;
        Ok(user.map(|u| (row.into(), u)))
    }

    pub async fn list_devices(&self, user_id: Option<i64>) -> anyhow::Result<Vec<Device>> {
        let rows = sqlx::query_as::<_, DeviceRow>(
            "SELECT id, user_id, name, platform, created_at, last_seen_at FROM devices
             WHERE ? IS NULL OR user_id = ? ORDER BY id",
        )
        .bind(user_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Device::from).collect())
    }

    pub async fn device(&self, id: i64) -> anyhow::Result<Option<Device>> {
        Ok(sqlx::query_as::<_, DeviceRow>(
            "SELECT id, user_id, name, platform, created_at, last_seen_at FROM devices WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .map(Device::from))
    }

    pub async fn delete_device(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM devices WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_pairing(
        &self,
        code_hash: &str,
        user_id: i64,
        created_by: i64,
        expires_at: i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO pairing_codes (code_hash, user_id, created_by, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(code_hash)
        .bind(user_id)
        .bind(created_by)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Consume a pairing code. Returns the user the device will belong to.
    pub async fn take_pairing(&self, code_hash: &str) -> anyhow::Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "DELETE FROM pairing_codes WHERE code_hash = ? AND expires_at > ? RETURNING user_id",
        )
        .bind(code_hash)
        .bind(now())
        .fetch_optional(&self.pool)
        .await?)
    }

    // ---- services ----

    pub async fn list_services(&self) -> anyhow::Result<Vec<ServiceRow>> {
        Ok(
            sqlx::query_as::<_, ServiceDbRow>("SELECT * FROM services ORDER BY sort_order, name")
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(ServiceRow::from)
                .collect(),
        )
    }

    pub async fn service(&self, id: &str) -> anyhow::Result<Option<ServiceRow>> {
        Ok(
            sqlx::query_as::<_, ServiceDbRow>("SELECT * FROM services WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .map(ServiceRow::from),
        )
    }

    /// Insert or replace a service.
    pub async fn save_service(&self, s: &ServiceRow) -> anyhow::Result<()> {
        let now = now();
        sqlx::query(
            "INSERT INTO services (id, kind, preset, name, base_url, enabled, category, settings,
                                   secrets, linked_vmid, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET
                kind = excluded.kind, preset = excluded.preset, name = excluded.name,
                base_url = excluded.base_url, enabled = excluded.enabled,
                category = excluded.category, settings = excluded.settings,
                secrets = excluded.secrets, linked_vmid = excluded.linked_vmid,
                sort_order = excluded.sort_order, updated_at = excluded.updated_at",
        )
        .bind(&s.id)
        .bind(&s.kind)
        .bind(&s.preset)
        .bind(&s.name)
        .bind(&s.base_url)
        .bind(s.enabled)
        .bind(&s.category)
        .bind(serde_json::to_string(&s.settings)?)
        .bind(serde_json::to_string(&s.secrets)?)
        .bind(s.linked_vmid.map(i64::from))
        .bind(s.sort_order)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_service(&self, id: &str) -> anyhow::Result<bool> {
        Ok(sqlx::query("DELETE FROM services WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected()
            > 0)
    }

    // ---- activity ----

    pub async fn log_activity(
        &self,
        user: Option<&User>,
        action: &str,
        target: &str,
        result: ActivityResult,
        detail: Option<&str>,
    ) -> anyhow::Result<ActivityEntry> {
        let now = now();
        let id = sqlx::query(
            "INSERT INTO activity_log (ts, user_id, username, action, target, result, detail)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(now)
        .bind(user.map(|u| u.id))
        .bind(user.map(|u| u.username.as_str()))
        .bind(action)
        .bind(target)
        .bind(result.as_str())
        .bind(detail)
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        Ok(ActivityEntry {
            id,
            at: ts(now),
            user_id: user.map(|u| u.id),
            username: user.map(|u| u.username.clone()),
            action: action.to_owned(),
            target: target.to_owned(),
            result,
            detail: detail.map(str::to_owned),
        })
    }

    pub async fn finish_activity(
        &self,
        id: i64,
        result: ActivityResult,
        detail: Option<&str>,
    ) -> anyhow::Result<Option<ActivityEntry>> {
        Ok(sqlx::query_as::<_, ActivityRow>(
            "UPDATE activity_log SET result = ?, detail = COALESCE(?, detail) WHERE id = ? RETURNING *",
        )
        .bind(result.as_str())
        .bind(detail)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .map(ActivityEntry::from))
    }

    pub async fn list_activity(
        &self,
        limit: i64,
        before_id: Option<i64>,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<ActivityEntry>> {
        Ok(sqlx::query_as::<_, ActivityRow>(
            "SELECT * FROM activity_log
             WHERE (? IS NULL OR id < ?) AND (? IS NULL OR user_id = ?)
             ORDER BY id DESC LIMIT ?",
        )
        .bind(before_id)
        .bind(before_id)
        .bind(user_id)
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(ActivityEntry::from)
        .collect())
    }

    // ---- settings ----

    pub async fn setting(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---- uptime ----

    /// Record a sample for the minute containing `at`. The worst state in a minute wins.
    pub async fn record_uptime(
        &self,
        target: &str,
        at: i64,
        state: i64,
        response_ms: Option<u64>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO uptime_minutes (target, minute, state, response_ms) VALUES (?, ?, ?, ?)
             ON CONFLICT (target, minute) DO UPDATE SET
                state = MIN(state, excluded.state),
                response_ms = COALESCE(excluded.response_ms, response_ms)",
        )
        .bind(target)
        .bind(at / 60 * 60)
        .bind(state)
        .bind(response_ms.map(|v| v as i64))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Uptime for every target over the last 24 h (with hourly buckets) and 7 days.
    pub async fn uptime_stats(&self, now: i64) -> anyhow::Result<HashMap<String, UptimeStats>> {
        let mut out: HashMap<String, UptimeStats> = HashMap::new();
        let week: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT target, SUM(state > 0), COUNT(*) FROM uptime_minutes
             WHERE minute >= ? GROUP BY target",
        )
        .bind(now - 7 * 86_400)
        .fetch_all(&self.pool)
        .await?;
        for (target, up, total) in week {
            let s = out.entry(target).or_default();
            s.up_7d = up;
            s.total_7d = total;
        }
        let hours: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT target, minute / 3600 * 3600 AS hour, MIN(state), SUM(state > 0), COUNT(*)
             FROM uptime_minutes WHERE minute >= ? GROUP BY target, hour",
        )
        .bind(now - 86_400)
        .fetch_all(&self.pool)
        .await?;
        for (target, hour, worst, up, total) in hours {
            let s = out.entry(target).or_default();
            s.up_24h += up;
            s.total_24h += total;
            s.hours.insert(hour, (worst, up, total));
        }
        Ok(out)
    }

    /// Move minute samples older than 7 days into hourly rows; keep hours for 90 days.
    pub async fn downsample_uptime(&self, now: i64) -> anyhow::Result<()> {
        let cutoff = (now - 7 * 86_400) / 3600 * 3600;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO uptime_hours (target, hour, healthy_minutes, warning_minutes, down_minutes, avg_response_ms)
             SELECT target, minute / 3600 * 3600 AS h, SUM(state = 2), SUM(state = 1), SUM(state = 0),
                    CAST(AVG(response_ms) AS INTEGER)
             FROM uptime_minutes WHERE minute < ? GROUP BY target, h
             ON CONFLICT (target, hour) DO UPDATE SET
                healthy_minutes = healthy_minutes + excluded.healthy_minutes,
                warning_minutes = warning_minutes + excluded.warning_minutes,
                down_minutes = down_minutes + excluded.down_minutes",
        )
        .bind(cutoff)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM uptime_minutes WHERE minute < ?")
            .bind(cutoff)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM uptime_hours WHERE hour < ?")
            .bind(now - 90 * 86_400)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Remove expired sessions and pairing codes.
    pub async fn cleanup_expired(&self, now: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM pairing_codes WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn uptime_downsampling_keeps_counts() {
        let db = Db::in_memory().await.unwrap();
        let now = 1_800_000_000;
        let old = now - 8 * 86_400;
        for i in 0..60 {
            db.record_uptime("svc:a", old + i * 60, if i < 45 { 2 } else { 0 }, Some(10))
                .await
                .unwrap();
        }
        db.record_uptime("svc:a", now - 120, 2, Some(10))
            .await
            .unwrap();
        db.record_uptime("svc:a", now - 120, 0, None).await.unwrap();
        db.downsample_uptime(now).await.unwrap();

        let (healthy, down): (i64, i64) =
            sqlx::query_as("SELECT SUM(healthy_minutes), SUM(down_minutes) FROM uptime_hours")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!((healthy, down), (45, 15));
        let stats = db.uptime_stats(now).await.unwrap();
        let a = &stats["svc:a"];
        // Only the recent minute is left, and the worst state of that minute won.
        assert_eq!((a.up_24h, a.total_24h, a.up_7d, a.total_7d), (0, 1, 0, 1));
    }

    #[tokio::test]
    async fn pairing_code_is_single_use() {
        let db = Db::in_memory().await.unwrap();
        let uid = db
            .create_user("owner", "x", Role::Owner, &[], false)
            .await
            .unwrap();
        db.create_pairing("h", uid, uid, now() + 60).await.unwrap();
        assert_eq!(db.take_pairing("h").await.unwrap(), Some(uid));
        assert_eq!(db.take_pairing("h").await.unwrap(), None);
        db.create_pairing("old", uid, uid, now() - 1).await.unwrap();
        assert_eq!(db.take_pairing("old").await.unwrap(), None);
    }
}
