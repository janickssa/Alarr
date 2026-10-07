//! iOS push through the Apple Push Notification service (APNs), with token-based auth: the
//! `.p8` key from `[apns]` signs a JWT, sent with every request (`a2` crate). Checked against
//! Apple's "Sending notification requests to APNs" and "Generating a remote notification"
//! docs: `apns-topic` is the bundle id, `apns-push-type: alert`, and `aps` carries `alert`,
//! `sound`, `category`, `thread-id` and `interruption-level`.
//!
//! Each paired iPhone registers its device token (`POST /devices/{id}/push-token`) together
//! with its environment: debug builds from Xcode get sandbox tokens, TestFlight and ad hoc
//! builds production tokens. Tokens APNs rejects as no longer valid (HTTP 410 `Unregistered`,
//! `BadDeviceToken`, `DeviceTokenNotForTopic`) are removed.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Map, Value, json};

use super::AlertType;
use super::channels::{ChannelInfo, Notification, NotificationChannel};
use crate::config::ApnsConfig;
use crate::db::{ActivityResult, Db, PushTarget, User};
use crate::events::{ActionEvent, Event, EventHub};

/// Notification categories registered by the iPhone app.
pub const CATEGORY_REQUEST: &str = "SEERR_REQUEST";
pub const CATEGORY_DOWN: &str = "SERVICE_DOWN";

/// One notification as APNs gets it (minus the device token).
#[derive(Debug, Clone, PartialEq)]
pub struct PushMessage {
    pub title: String,
    pub body: String,
    pub category: Option<&'static str>,
    /// `passive`, `active` or `time-sensitive`.
    pub interruption_level: &'static str,
    /// Groups notifications in Notification Center (per service).
    pub thread_id: String,
    /// Custom keys next to `aps`: `path` (deep link), `service_id`, `request_id`, `alert_type`.
    pub data: Map<String, Value>,
}

impl PushMessage {
    pub fn from_notification(n: &Notification) -> Self {
        let category = match (n.alert_type, n.recovery) {
            (_, true) => None,
            (Some(AlertType::NewRequest), _) if n.request_id.is_some() => Some(CATEGORY_REQUEST),
            (Some(AlertType::ServiceDown | AlertType::NodeOffline), _) => Some(CATEGORY_DOWN),
            _ => None,
        };
        let interruption_level = match (n.alert_type, n.recovery) {
            (_, true) => "passive",
            (Some(AlertType::ServiceDown | AlertType::NodeOffline), false) => "time-sensitive",
            _ => "active",
        };
        let mut data = Map::new();
        if let Some(p) = &n.path {
            data.insert("path".into(), json!(p));
        }
        if let Some(id) = &n.service_id {
            data.insert("service_id".into(), json!(id));
        }
        if let Some(id) = n.request_id {
            data.insert("request_id".into(), json!(id));
        }
        if let Some(t) = n.alert_type {
            data.insert("alert_type".into(), json!(t.as_str()));
        }
        Self {
            title: n.title.clone(),
            body: n.message.clone(),
            category,
            interruption_level,
            thread_id: n.service_id.clone().unwrap_or_else(|| "quorum".into()),
            data,
        }
    }

    /// The JSON body sent to APNs.
    pub fn body(&self) -> Value {
        let mut aps = json!({
            "alert": { "title": self.title, "body": self.body },
            "sound": "default",
            "thread-id": self.thread_id,
            "interruption-level": self.interruption_level,
        });
        if let Some(c) = self.category {
            aps["category"] = json!(c);
        }
        let mut body = self.data.clone();
        body.insert("aps".into(), aps);
        Value::Object(body)
    }
}

#[derive(Debug)]
pub enum SendError {
    /// The token is no longer valid for this app: forget it.
    Unregistered(String),
    Failed(String),
}

#[async_trait]
pub trait ApnsSender: Send + Sync {
    async fn send(&self, token: &str, sandbox: bool, msg: &PushMessage) -> Result<(), SendError>;
}

/// APNs over HTTP/2 with the `a2` client, one connection per environment.
pub struct A2Sender {
    production: a2::Client,
    sandbox: a2::Client,
    topic: String,
}

impl A2Sender {
    pub fn new(cfg: &ApnsConfig) -> anyhow::Result<Self> {
        let key = read_key(&cfg.key_path)?;
        let client = |endpoint| {
            a2::Client::token(
                key.as_slice(),
                cfg.key_id.clone(),
                cfg.team_id.clone(),
                a2::ClientConfig::new(endpoint),
            )
        };
        Ok(Self {
            production: client(a2::Endpoint::Production)?,
            sandbox: client(a2::Endpoint::Sandbox)?,
            topic: cfg.bundle_id.clone(),
        })
    }
}

fn read_key(path: &Path) -> anyhow::Result<Vec<u8>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path)
            && meta.permissions().mode() & 0o077 != 0
        {
            tracing::warn!(path = %path.display(), "APNs key is readable by other users; chmod 600 it");
        }
    }
    std::fs::read(path).map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))
}

/// `a2` payload with the fields its builder lacks (`thread-id`, `interruption-level`).
#[derive(Debug, Serialize)]
struct Payload<'a> {
    #[serde(flatten)]
    body: &'a Value,
    #[serde(skip)]
    token: &'a str,
    #[serde(skip)]
    options: a2::NotificationOptions<'a>,
}

impl a2::request::payload::PayloadLike for Payload<'_> {
    fn get_device_token(&self) -> &str {
        self.token
    }

    fn get_options(&self) -> &a2::NotificationOptions<'_> {
        &self.options
    }
}

#[async_trait]
impl ApnsSender for A2Sender {
    async fn send(&self, token: &str, sandbox: bool, msg: &PushMessage) -> Result<(), SendError> {
        let body = msg.body();
        let payload = Payload {
            body: &body,
            token,
            options: a2::NotificationOptions {
                apns_topic: Some(&self.topic),
                apns_push_type: Some(a2::PushType::Alert),
                apns_priority: Some(if msg.interruption_level == "passive" {
                    a2::Priority::Normal
                } else {
                    a2::Priority::High
                }),
                ..Default::default()
            },
        };
        let client = if sandbox {
            &self.sandbox
        } else {
            &self.production
        };
        match client.send(payload).await {
            Ok(_) => Ok(()),
            Err(a2::Error::ResponseError(r)) => {
                let reason = r.error.as_ref().map(|e| format!("{:?}", e.reason));
                let gone = r.code == 410
                    || r.error.as_ref().is_some_and(|e| {
                        matches!(
                            e.reason,
                            a2::ErrorReason::Unregistered
                                | a2::ErrorReason::BadDeviceToken
                                | a2::ErrorReason::DeviceTokenNotForTopic
                        )
                    });
                let text = format!(
                    "APNs answered HTTP {}{}",
                    r.code,
                    reason.map(|r| format!(" ({r})")).unwrap_or_default()
                );
                Err(if gone {
                    SendError::Unregistered(text)
                } else {
                    SendError::Failed(text)
                })
            }
            Err(e) => Err(SendError::Failed(e.to_string())),
        }
    }
}

/// The `push` notification channel: every paired iPhone with push turned on, minus the
/// devices that muted the alert type and guests who can't see the service.
pub struct PushChannel {
    db: Db,
    events: EventHub,
    sender: Arc<dyn ApnsSender>,
    bundle_id: String,
}

impl PushChannel {
    pub fn new(db: Db, events: EventHub, sender: Arc<dyn ApnsSender>, bundle_id: &str) -> Self {
        Self {
            db,
            events,
            sender,
            bundle_id: bundle_id.to_owned(),
        }
    }

    async fn wants(&self, user: &User, t: &PushTarget, n: &Notification) -> anyhow::Result<bool> {
        if user.role == crate::db::Role::Guest {
            let visible = n
                .service_id
                .as_ref()
                .is_some_and(|id| user.allowed_services.contains(id));
            if !visible {
                return Ok(false);
            }
        }
        if let Some(at) = n.alert_type {
            let muted = self.db.alert_mutes(user.id, Some(t.device_id)).await?;
            if muted.iter().any(|m| m == at.as_str()) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[async_trait]
impl NotificationChannel for PushChannel {
    fn id(&self) -> &'static str {
        "push"
    }

    fn info(&self) -> ChannelInfo {
        ChannelInfo {
            id: "push".into(),
            label: "iOS push".into(),
            configured: true,
            detail: format!("APNs for {} (iPhones that turned push on)", self.bundle_id),
        }
    }

    async fn send(&self, n: &Notification) -> anyhow::Result<()> {
        let targets = self.db.push_targets().await?;
        if targets.is_empty() {
            anyhow::bail!("no paired iPhone has push turned on");
        }
        let msg = PushMessage::from_notification(n);
        let mut users: HashMap<i64, Option<User>> = HashMap::new();
        let (mut sent, mut skipped) = (Vec::new(), 0usize);
        let (mut removed, mut failed) = (Vec::new(), Vec::new());
        for t in &targets {
            if let std::collections::hash_map::Entry::Vacant(e) = users.entry(t.user_id) {
                e.insert(self.db.user(t.user_id).await?);
            }
            let Some(user) = users[&t.user_id].as_ref() else {
                continue;
            };
            if !self.wants(user, t, n).await? {
                skipped += 1;
                continue;
            }
            match self.sender.send(&t.token, t.sandbox, &msg).await {
                Ok(()) => sent.push(t.name.clone()),
                Err(SendError::Unregistered(e)) => {
                    tracing::info!(device = t.device_id, error = %e, "removing APNs token");
                    self.db.clear_push_token(t.device_id).await?;
                    removed.push(t.name.clone());
                }
                Err(SendError::Failed(e)) => {
                    tracing::warn!(device = t.device_id, error = %e, "push failed");
                    failed.push(format!("{}: {e}", t.name));
                }
            }
        }

        let mut parts = Vec::new();
        if !sent.is_empty() {
            parts.push(format!("sent to {}", sent.join(", ")));
        }
        if skipped > 0 {
            parts.push(format!("{skipped} muted or not allowed"));
        }
        if !removed.is_empty() {
            parts.push(format!(
                "removed {} (no longer registered)",
                removed.join(", ")
            ));
        }
        if !failed.is_empty() {
            parts.push(format!("failed: {}", failed.join("; ")));
        }
        let ok = failed.is_empty() || !sent.is_empty();
        let detail = format!("{} — {}", n.title, parts.join(" · "));
        match self
            .db
            .log_activity(
                None,
                "push.send",
                "push",
                if ok {
                    ActivityResult::Ok
                } else {
                    ActivityResult::Error
                },
                Some(&detail),
            )
            .await
        {
            Ok(entry) => self.events.send(Event::Action(ActionEvent {
                service_id: n.service_id.clone(),
                entry,
            })),
            Err(e) => tracing::warn!(error = %e, "could not log push delivery"),
        }
        if !ok {
            anyhow::bail!("{}", failed.join("; "));
        }
        if sent.is_empty() && removed.is_empty() && skipped == 0 {
            anyhow::bail!("no device accepted the notification");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::rules::Severity;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        sent: Mutex<Vec<(String, bool, PushMessage)>>,
        gone: Vec<String>,
    }

    #[async_trait]
    impl ApnsSender for Fake {
        async fn send(
            &self,
            token: &str,
            sandbox: bool,
            msg: &PushMessage,
        ) -> Result<(), SendError> {
            if self.gone.iter().any(|g| g == token) {
                return Err(SendError::Unregistered("APNs answered HTTP 410".into()));
            }
            self.sent
                .lock()
                .unwrap()
                .push((token.into(), sandbox, msg.clone()));
            Ok(())
        }
    }

    fn down() -> Notification {
        Notification {
            title: "Sonarr is down".into(),
            message: "timeout".into(),
            severity: Severity::Critical,
            recovery: false,
            click: None,
            alert_type: Some(AlertType::ServiceDown),
            service_id: Some("sonarr".into()),
            path: Some("/services/sonarr".into()),
            request_id: None,
        }
    }

    #[test]
    fn payload_shape() {
        let body = PushMessage::from_notification(&down()).body();
        assert_eq!(body["aps"]["interruption-level"], "time-sensitive");
        assert_eq!(body["aps"]["category"], CATEGORY_DOWN);
        assert_eq!(body["aps"]["thread-id"], "sonarr");
        assert_eq!(body["aps"]["alert"]["title"], "Sonarr is down");
        assert_eq!(body["path"], "/services/sonarr");

        let mut req = down();
        req.alert_type = Some(AlertType::NewRequest);
        req.service_id = Some("seerr".into());
        req.request_id = Some(42);
        let body = PushMessage::from_notification(&req).body();
        assert_eq!(body["aps"]["category"], CATEGORY_REQUEST);
        assert_eq!(body["aps"]["interruption-level"], "active");
        assert_eq!(body["request_id"], 42);

        let mut back = down();
        back.recovery = true;
        let body = PushMessage::from_notification(&back).body();
        assert_eq!(body["aps"]["interruption-level"], "passive");
        assert!(body["aps"].get("category").is_none());
    }

    #[tokio::test]
    async fn delivers_skips_muted_and_removes_dead_tokens() {
        let db = Db::in_memory().await.unwrap();
        let owner = db
            .create_user("owner", "x", crate::db::Role::Owner, &[], false)
            .await
            .unwrap();
        let guest = db
            .create_user(
                "guest",
                "x",
                crate::db::Role::Guest,
                &["seerr".into()],
                false,
            )
            .await
            .unwrap();
        let phone = db
            .create_device(owner, "iPhone", "ios", "h1")
            .await
            .unwrap();
        let old = db
            .create_device(owner, "Old iPhone", "ios", "h2")
            .await
            .unwrap();
        let muted = db.create_device(owner, "iPad", "ios", "h3").await.unwrap();
        let gphone = db.create_device(guest, "Guest", "ios", "h4").await.unwrap();
        db.set_push_token(phone.id, "aa", "sandbox").await.unwrap();
        db.set_push_token(old.id, "bb", "production").await.unwrap();
        db.set_push_token(muted.id, "cc", "production")
            .await
            .unwrap();
        db.set_push_token(gphone.id, "dd", "production")
            .await
            .unwrap();
        db.set_alert_mutes(owner, Some(muted.id), &["service_down".into()])
            .await
            .unwrap();

        let fake = Arc::new(Fake {
            gone: vec!["bb".into()],
            ..Default::default()
        });
        let ch = PushChannel::new(db.clone(), EventHub::new(), fake.clone(), "com.example.q");
        ch.send(&down()).await.unwrap();

        let sent = fake.sent.lock().unwrap().clone();
        assert_eq!(sent.len(), 1, "only the unmuted owner phone: {sent:?}");
        assert_eq!((sent[0].0.as_str(), sent[0].1), ("aa", true));
        let left: Vec<_> = db
            .push_targets()
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.token)
            .collect();
        assert_eq!(left, ["aa", "cc", "dd"], "the 410 token is gone");
        let log = db.list_activity(10, None, None).await.unwrap();
        assert!(log.iter().any(|e| e.action == "push.send"
            && e.detail.as_deref().unwrap_or("").contains("sent to iPhone")));
    }

    #[tokio::test]
    async fn a_token_moves_to_the_new_device() {
        let db = Db::in_memory().await.unwrap();
        let u = db
            .create_user("o", "x", crate::db::Role::Owner, &[], false)
            .await
            .unwrap();
        let a = db.create_device(u, "a", "ios", "h1").await.unwrap();
        let b = db.create_device(u, "b", "ios", "h2").await.unwrap();
        db.set_push_token(a.id, "tok", "production").await.unwrap();
        db.set_push_token(b.id, "tok", "production").await.unwrap();
        let t = db.push_targets().await.unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].device_id, b.id);
    }
}

#[cfg(test)]
mod key_tests {
    use base64::Engine;

    #[test]
    fn loads_a_p8_key() {
        // A fresh P-256 key in the `.p8` format Apple hands out (PKCS#8 PEM).
        let rng = ring::rand::SystemRandom::new();
        let der = ring::signature::EcdsaKeyPair::generate_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &rng,
        )
        .unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(der.as_ref());
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
        let dir = std::env::temp_dir().join(format!("quorum-apns-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("AuthKey_TEST.p8");
        std::fs::write(&path, pem).unwrap();
        let cfg = crate::config::ApnsConfig {
            enabled: true,
            team_id: "TEAM123456".into(),
            key_id: "KEY1234567".into(),
            key_path: path.clone(),
            bundle_id: "com.example.quorum".into(),
            environment: "production".into(),
        };
        assert!(super::A2Sender::new(&cfg).is_ok());
        let missing = crate::config::ApnsConfig {
            key_path: dir.join("missing.p8"),
            ..cfg
        };
        assert!(super::A2Sender::new(&missing).is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
