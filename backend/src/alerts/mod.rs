//! Alerts engine: turns conditions in the cached snapshots into alerts, deduplicates them
//! (one active alert per key, resolved when the condition clears), applies per-type
//! settings and quiet hours, and delivers through the notification channels.

pub mod channels;
pub mod push;
pub mod rules;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use utoipa::ToSchema;

use crate::db::{Alert, AlertSettingRow, Delivery, NewAlert};
use crate::events::{AlertEvent, Event};
use crate::state::AppState;
use channels::{NotConfigured, Notification, NotificationChannel};
use rules::{Condition, Severity};

/// At most this many notifications per minute across all channels; the rest are recorded
/// as `rate_limited`.
const MAX_SENDS_PER_MINUTE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    ServiceDown,
    NodeOffline,
    DiskFull,
    DownloadFailed,
    CertificateExpiring,
    NewRequest,
}

impl AlertType {
    pub const ALL: [AlertType; 6] = [
        Self::ServiceDown,
        Self::NodeOffline,
        Self::DiskFull,
        Self::DownloadFailed,
        Self::CertificateExpiring,
        Self::NewRequest,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ServiceDown => "service_down",
            Self::NodeOffline => "node_offline",
            Self::DiskFull => "disk_full",
            Self::DownloadFailed => "download_failed",
            Self::CertificateExpiring => "certificate_expiring",
            Self::NewRequest => "new_request",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ServiceDown => "Service down",
            Self::NodeOffline => "Node offline",
            Self::DiskFull => "Disk almost full",
            Self::DownloadFailed => "Download failed",
            Self::CertificateExpiring => "Certificate expiring",
            Self::NewRequest => "New request",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::ServiceDown => "A service has been down for a minute",
            Self::NodeOffline => "A Proxmox node has been offline for a minute",
            Self::DiskFull => {
                "Storage, a node's root disk, a container disk or a media disk is 90 % full or more"
            }
            Self::DownloadFailed => {
                "qBittorrent reports a torrent error, or Sonarr/Radarr a failed download"
            }
            Self::CertificateExpiring => {
                "A certificate in use in Nginx Proxy Manager expires within 14 days"
            }
            Self::NewRequest => "Someone requested a movie or series in Seerr",
        }
    }
}

/// Quiet hours in the server's local time, `HH:MM`. The window may cross midnight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QuietHours {
    pub start: String,
    pub end: String,
}

impl QuietHours {
    fn parse(s: &str) -> Option<NaiveTime> {
        NaiveTime::parse_from_str(s, "%H:%M").ok()
    }

    pub fn validate(&self) -> Result<(), String> {
        match (Self::parse(&self.start), Self::parse(&self.end)) {
            (Some(a), Some(b)) if a != b => Ok(()),
            (Some(_), Some(_)) => Err("quiet hours start and end must differ".into()),
            _ => Err("quiet hours must be HH:MM".into()),
        }
    }

    pub fn contains(&self, t: NaiveTime) -> bool {
        let (Some(a), Some(b)) = (Self::parse(&self.start), Self::parse(&self.end)) else {
            return false;
        };
        if a < b {
            t >= a && t < b
        } else {
            t >= a || t < b
        }
    }
}

/// Settings for one alert type.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TypeSettings {
    pub alert_type: AlertType,
    pub enabled: bool,
    /// `ntfy` and/or `push`.
    pub channels: Vec<String>,
    pub quiet_hours: Option<QuietHours>,
}

impl TypeSettings {
    fn default_for(t: AlertType) -> Self {
        Self {
            alert_type: t,
            enabled: true,
            channels: vec!["ntfy".into()],
            quiet_hours: None,
        }
    }

    fn from_row(t: AlertType, r: &AlertSettingRow) -> Self {
        Self {
            alert_type: t,
            enabled: r.enabled,
            channels: serde_json::from_str(&r.channels).unwrap_or_default(),
            quiet_hours: r
                .quiet_hours
                .as_deref()
                .and_then(|q| serde_json::from_str(q).ok()),
        }
    }

    pub fn to_row(&self) -> AlertSettingRow {
        AlertSettingRow {
            alert_type: self.alert_type.as_str().to_owned(),
            enabled: self.enabled,
            channels: serde_json::to_string(&self.channels).unwrap_or_else(|_| "[]".into()),
            quiet_hours: self
                .quiet_hours
                .as_ref()
                .and_then(|q| serde_json::to_string(q).ok()),
        }
    }
}

/// Settings for every type, defaults filled in.
pub async fn load_settings(state: &AppState) -> anyhow::Result<HashMap<AlertType, TypeSettings>> {
    let rows = state.db.alert_settings().await?;
    Ok(AlertType::ALL
        .into_iter()
        .map(|t| {
            let s = rows
                .iter()
                .find(|r| r.alert_type == t.as_str())
                .map(|r| TypeSettings::from_row(t, r))
                .unwrap_or_else(|| TypeSettings::default_for(t));
            (t, s)
        })
        .collect())
}

/// Server-local time of day, for quiet hours.
fn local_time(at: DateTime<Utc>) -> NaiveTime {
    at.with_timezone(&chrono::Local).time()
}

pub struct Engine {
    state: AppState,
    /// When each not-yet-alerted condition was first seen (for delays).
    pending: HashMap<String, DateTime<Utc>>,
    sent: VecDeque<DateTime<Utc>>,
}

impl Engine {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            pending: HashMap::new(),
            sent: VecDeque::new(),
        }
    }

    fn click_url(&self, path: &str) -> Option<String> {
        self.state
            .config
            .server
            .public_url
            .as_deref()
            .map(|u| format!("{}{path}", u.trim_end_matches('/')))
    }

    /// Deliver to the channels picked for the type, honouring quiet hours and the rate limit.
    async fn deliver(
        &mut self,
        n: &Notification,
        settings: &TypeSettings,
        now: DateTime<Utc>,
    ) -> Vec<Delivery> {
        let quiet = settings
            .quiet_hours
            .as_ref()
            .is_some_and(|q| q.contains(local_time(now)));
        let channels: Vec<Arc<dyn NotificationChannel>> = self.state.channels.clone();
        let mut out = Vec::new();
        for id in &settings.channels {
            let Some(ch) = channels.iter().find(|c| c.id() == id) else {
                continue;
            };
            let d = |result: &str, detail: Option<String>| Delivery {
                channel: id.clone(),
                result: result.into(),
                detail,
            };
            if quiet {
                out.push(d("quiet_hours", None));
                continue;
            }
            while self
                .sent
                .front()
                .is_some_and(|t| now - *t > Duration::minutes(1))
            {
                self.sent.pop_front();
            }
            if !ch.info().configured {
                out.push(d("not_configured", None));
                continue;
            }
            if self.sent.len() >= MAX_SENDS_PER_MINUTE {
                out.push(d("rate_limited", None));
                continue;
            }
            self.sent.push_back(now);
            match ch.send(n).await {
                Ok(()) => out.push(d("sent", None)),
                Err(e) if e.downcast_ref::<NotConfigured>().is_some() => {
                    out.push(d("not_configured", None));
                }
                Err(e) => {
                    tracing::warn!(channel = %id, error = %e, "alert delivery failed");
                    out.push(d("failed", Some(e.to_string())));
                }
            }
        }
        out
    }

    fn broadcast(&self, alert: Alert) {
        self.state.events.send(Event::Alert(AlertEvent {
            service_id: alert.service_id.clone(),
            alert,
        }));
    }

    /// Evaluate the rules once and raise/resolve alerts.
    pub async fn tick(&mut self, now: DateTime<Utc>) -> anyhow::Result<()> {
        let rows = self.state.db.list_services().await?;
        let names: HashMap<String, String> = rows
            .iter()
            .filter(|r| r.enabled)
            .map(|r| (r.id.clone(), r.name.clone()))
            .collect();
        let snapshots: Vec<_> = self
            .state
            .cache
            .all()
            .into_iter()
            .filter(|s| names.contains_key(&s.service_id))
            .collect();
        let (conditions, stale) = rules::evaluate(&snapshots, &names, now);
        let settings = load_settings(&self.state).await?;
        let active: HashMap<String, Alert> = self
            .state
            .db
            .active_alerts()
            .await?
            .into_iter()
            .map(|a| (a.key.clone(), a))
            .collect();

        let keys: HashSet<&str> = conditions.iter().map(|c| c.key.as_str()).collect();
        self.pending.retain(|k, _| keys.contains(k.as_str()));

        for c in &conditions {
            if active.contains_key(&c.key) {
                continue;
            }
            let s = &settings[&c.alert_type];
            if !s.enabled {
                continue;
            }
            let first = *self.pending.entry(c.key.clone()).or_insert(now);
            if (now - first).num_seconds() < c.delay_s {
                continue;
            }
            if c.once && self.state.db.alert_key_seen(&c.key).await? {
                continue;
            }
            self.fire(c, s, now).await?;
        }

        for (key, a) in active {
            if keys.contains(key.as_str()) {
                continue;
            }
            // Data from a service that is currently failing is frozen.
            let frozen = a.alert_type != AlertType::ServiceDown.as_str()
                && a.service_id.as_ref().is_some_and(|id| stale.contains(id));
            if frozen {
                continue;
            }
            if let Some(resolved) = self.state.db.resolve_alert(&key, now.timestamp()).await? {
                self.recovered(&resolved, &settings, now).await;
                self.broadcast(resolved);
            }
        }
        Ok(())
    }

    async fn fire(
        &mut self,
        c: &Condition,
        settings: &TypeSettings,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let mut alert = self
            .state
            .db
            .insert_alert(
                &NewAlert {
                    alert_type: c.alert_type.as_str().to_owned(),
                    key: c.key.clone(),
                    service_id: c.service_id.clone(),
                    severity: c.severity.as_str().to_owned(),
                    title: c.title.clone(),
                    message: c.message.clone(),
                },
                now.timestamp(),
            )
            .await?;
        tracing::info!(key = %c.key, title = %c.title, "alert");
        let path = match (&c.service_id, c.alert_type) {
            (_, AlertType::NodeOffline | AlertType::DiskFull) => "/proxmox".to_owned(),
            (Some(id), _) => format!("/services/{id}"),
            (None, _) => "/alerts".to_owned(),
        };
        // `new_request:<service>:<request id>`
        let request_id = (c.alert_type == AlertType::NewRequest)
            .then(|| c.key.rsplit(':').next()?.parse().ok())
            .flatten();
        let delivery = self
            .deliver(
                &Notification {
                    title: c.title.clone(),
                    message: c.message.clone(),
                    severity: c.severity,
                    recovery: false,
                    click: self.click_url(&path),
                    alert_type: Some(c.alert_type),
                    service_id: c.service_id.clone(),
                    path: Some(path),
                    request_id,
                },
                settings,
                now,
            )
            .await;
        self.state
            .db
            .set_alert_delivery(alert.id, &delivery)
            .await?;
        alert.delivery = delivery;
        self.broadcast(alert);
        Ok(())
    }

    /// "Back up" message for down/offline alerts that were actually sent.
    async fn recovered(
        &mut self,
        a: &Alert,
        settings: &HashMap<AlertType, TypeSettings>,
        now: DateTime<Utc>,
    ) {
        let Some(t) = AlertType::parse(&a.alert_type) else {
            return;
        };
        if !matches!(t, AlertType::ServiceDown | AlertType::NodeOffline)
            || !a.delivery.iter().any(|d| d.result == "sent")
        {
            return;
        }
        let title = a
            .title
            .replace(" is down", " is back up")
            .replace(" is offline", " is back online");
        let minutes = (now - a.created_at).num_minutes().max(1);
        let n = Notification {
            title,
            message: format!("After {minutes} min"),
            severity: Severity::Info,
            recovery: true,
            click: None,
            alert_type: Some(t),
            service_id: a.service_id.clone(),
            path: a.service_id.as_ref().map(|id| format!("/services/{id}")),
            request_id: None,
        };
        let s = &settings[&t];
        self.deliver(&n, s, now).await;
    }

    /// Send a test notification on one channel (Settings → Alerts → Send test).
    pub async fn test(state: &AppState, channel: &str) -> anyhow::Result<()> {
        let ch = state
            .channels
            .iter()
            .find(|c| c.id() == channel)
            .ok_or_else(|| anyhow::anyhow!("unknown channel"))?;
        ch.send(&Notification {
            title: format!("Test from {}", crate::APP_NAME),
            message: "Alerts will arrive here.".into(),
            severity: Severity::Info,
            recovery: false,
            click: None,
            alert_type: None,
            service_id: None,
            path: None,
            request_id: None,
        })
        .await
    }
}

/// Run the engine: re-evaluate after snapshot and health events (at most every 2 s) and
/// every 15 s for delayed conditions.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut rx = state.events.subscribe();
        let mut engine = Engine::new(state);
        let mut tick = tokio::time::interval(StdDuration::from_secs(15));
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                ev = rx.recv() => match ev {
                    Ok(Event::Snapshot(_) | Event::Health(_)) | Err(RecvError::Lagged(_)) => {
                        tokio::time::sleep(StdDuration::from_secs(2)).await;
                        // Drain what arrived meanwhile; one evaluation covers it.
                        while rx.try_recv().is_ok() {}
                    }
                    Ok(_) => continue,
                    Err(RecvError::Closed) => return,
                },
            }
            if let Err(e) = engine.tick(Utc::now()).await {
                tracing::warn!(error = %e, "alert evaluation failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_hours_cross_midnight() {
        let q = QuietHours {
            start: "22:00".into(),
            end: "07:00".into(),
        };
        let t = |s: &str| NaiveTime::parse_from_str(s, "%H:%M").unwrap();
        assert!(q.contains(t("23:30")) && q.contains(t("06:59")));
        assert!(!q.contains(t("07:00")) && !q.contains(t("12:00")));
        let day = QuietHours {
            start: "09:00".into(),
            end: "17:00".into(),
        };
        assert!(day.contains(t("09:00")) && !day.contains(t("17:00")));
        assert!(
            QuietHours {
                start: "7".into(),
                end: "8".into()
            }
            .validate()
            .is_err()
        );
    }
}
