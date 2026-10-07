//! Notification channels. Alerts go to every channel the owner picked for the alert type.
//!
//! - `ntfy`: JSON publish to the ntfy root URL (`POST /` with `{topic, title, message,
//!   priority, tags, click}`), checked against ntfy's "Publishing" docs. An access token,
//!   when needed, comes from `QUORUM_NTFY_TOKEN` (never from the config file).
//! - `push`: iOS push through APNs, built in phase 6. Until then it reports
//!   "not configured".

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use super::rules::Severity;
use crate::config::Config;

#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub severity: Severity,
    /// A "back to normal" message (lower priority, check mark).
    pub recovery: bool,
    /// Opened when the notification is tapped.
    pub click: Option<String>,
}

/// What the settings page shows about a channel.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChannelInfo {
    /// `ntfy` or `push`.
    pub id: String,
    pub label: String,
    pub configured: bool,
    /// Where it delivers (e.g. ntfy server and topic), or how to set it up.
    pub detail: String,
}

/// Delivery failed because the channel isn't set up (not an error worth logging).
#[derive(Debug)]
pub struct NotConfigured;

impl std::fmt::Display for NotConfigured {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("not configured")
    }
}

impl std::error::Error for NotConfigured {}

#[async_trait]
pub trait NotificationChannel: Send + Sync {
    fn id(&self) -> &'static str;
    fn info(&self) -> ChannelInfo;
    async fn send(&self, n: &Notification) -> anyhow::Result<()>;
}

pub struct NtfyChannel {
    client: reqwest::Client,
    url: String,
    topic: String,
    token: Option<String>,
}

impl NtfyChannel {
    pub fn new(url: &str, topic: &str, token: Option<String>) -> anyhow::Result<Self> {
        Ok(Self {
            client: crate::tls::http_client(None, Duration::from_secs(10))?,
            url: format!("{}/", url.trim_end_matches('/')),
            topic: topic.to_owned(),
            token,
        })
    }

    fn priority(n: &Notification) -> u8 {
        match (n.recovery, n.severity) {
            (true, _) => 2,
            (false, Severity::Critical) => 4,
            (false, Severity::Warning | Severity::Info) => 3,
        }
    }

    fn tags(n: &Notification) -> Vec<&'static str> {
        vec![match (n.recovery, n.severity) {
            (true, _) => "white_check_mark",
            (false, Severity::Critical) => "rotating_light",
            (false, Severity::Warning) => "warning",
            (false, Severity::Info) => "bell",
        }]
    }
}

#[async_trait]
impl NotificationChannel for NtfyChannel {
    fn id(&self) -> &'static str {
        "ntfy"
    }

    fn info(&self) -> ChannelInfo {
        ChannelInfo {
            id: "ntfy".into(),
            label: "ntfy".into(),
            configured: true,
            detail: format!("{}{}", self.url, self.topic),
        }
    }

    async fn send(&self, n: &Notification) -> anyhow::Result<()> {
        let mut body = json!({
            "topic": self.topic,
            "title": n.title,
            "message": n.message,
            "priority": Self::priority(n),
            "tags": Self::tags(n),
        });
        if let Some(click) = &n.click {
            body["click"] = json!(click);
        }
        let mut req = self.client.post(&self.url).json(&body);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let response = req.send().await?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!(
                "ntfy answered HTTP {}{}",
                status.as_u16(),
                if status.as_u16() == 401 || status.as_u16() == 403 {
                    " (set QUORUM_NTFY_TOKEN?)"
                } else {
                    ""
                }
            );
        }
        Ok(())
    }
}

/// ntfy without `[ntfy]` in the config.
pub struct Unconfigured {
    id: &'static str,
    label: &'static str,
    detail: &'static str,
}

#[async_trait]
impl NotificationChannel for Unconfigured {
    fn id(&self) -> &'static str {
        self.id
    }

    fn info(&self) -> ChannelInfo {
        ChannelInfo {
            id: self.id.into(),
            label: self.label.into(),
            configured: false,
            detail: self.detail.into(),
        }
    }

    async fn send(&self, _n: &Notification) -> anyhow::Result<()> {
        Err(NotConfigured.into())
    }
}

/// All channels, in display order.
pub fn from_config(config: &Config) -> Vec<Arc<dyn NotificationChannel>> {
    let token = std::env::var("QUORUM_NTFY_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty());
    let ntfy: Arc<dyn NotificationChannel> = match &config.ntfy {
        Some(c) => match NtfyChannel::new(&c.url, &c.topic, token) {
            Ok(ch) => Arc::new(ch),
            Err(e) => {
                tracing::warn!(error = %e, "ntfy channel disabled");
                Arc::new(Unconfigured {
                    id: "ntfy",
                    label: "ntfy",
                    detail: "Could not set up the ntfy client; see the server log",
                })
            }
        },
        None => Arc::new(Unconfigured {
            id: "ntfy",
            label: "ntfy",
            detail: "Add [ntfy] url and topic to quorum.toml",
        }),
    };
    vec![
        ntfy,
        Arc::new(Unconfigured {
            id: "push",
            label: "iOS push",
            detail: "Arrives with the iPhone app",
        }),
    ]
}

pub const CHANNEL_IDS: [&str; 2] = ["ntfy", "push"];

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn publishes_json_to_the_root_url() {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("authorization", "Bearer tk_abc"))
            .and(body_partial_json(json!({
                "topic": "quorum", "title": "Sonarr is down", "priority": 4,
                "tags": ["rotating_light"], "click": "https://q.example/services/sonarr"
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&s)
            .await;
        let ch = NtfyChannel::new(&s.uri(), "quorum", Some("tk_abc".into())).unwrap();
        ch.send(&Notification {
            title: "Sonarr is down".into(),
            message: "timeout".into(),
            severity: Severity::Critical,
            recovery: false,
            click: Some("https://q.example/services/sonarr".into()),
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn reports_auth_errors() {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&s)
            .await;
        let ch = NtfyChannel::new(&s.uri(), "quorum", None).unwrap();
        let err = ch
            .send(&Notification {
                title: "t".into(),
                message: "m".into(),
                severity: Severity::Info,
                recovery: false,
                click: None,
            })
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("QUORUM_NTFY_TOKEN"), "{err}");
    }
}
