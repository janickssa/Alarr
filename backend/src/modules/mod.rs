//! Service modules: the shared trait, common types and the module registry.

pub mod catalogue;
pub mod health_check;
pub mod proxmox;

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Kind of service module. Adding a service = adding a variant, a module file and a
/// registry entry in [`build_module`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    Proxmox,
    HealthCheck,
}

impl ServiceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proxmox => "proxmox",
            Self::HealthCheck => "health_check",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "proxmox" => Some(Self::Proxmox),
            "health_check" => Some(Self::HealthCheck),
            _ => None,
        }
    }

    pub fn level(self) -> IntegrationLevel {
        match self {
            Self::Proxmox => IntegrationLevel::Full,
            Self::HealthCheck => IntegrationLevel::HealthCheck,
        }
    }

    /// Default polling interval and the faster one used while a client is viewing it.
    pub fn poll_intervals(self) -> (std::time::Duration, std::time::Duration) {
        use std::time::Duration;
        match self {
            Self::Proxmox => (Duration::from_secs(10), Duration::from_secs(3)),
            Self::HealthCheck => (Duration::from_secs(30), Duration::from_secs(30)),
        }
    }

    /// Names of the secrets this kind uses (stored encrypted).
    pub fn secret_names(self) -> &'static [&'static str] {
        match self {
            Self::Proxmox => &["token_secret"],
            Self::HealthCheck => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationLevel {
    Full,
    ReadOnly,
    HealthCheck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Healthy,
    Warning,
    Down,
    InProgress,
    Disabled,
    Unknown,
}

impl Health {
    /// Uptime state stored per minute: 2 healthy, 1 warning, 0 down. `None` = not counted.
    pub fn uptime_state(self) -> Option<i64> {
        match self {
            Self::Healthy => Some(2),
            Self::Warning | Self::InProgress => Some(1),
            Self::Down => Some(0),
            Self::Disabled | Self::Unknown => None,
        }
    }

    pub fn is_problem(self) -> bool {
        matches!(self, Self::Warning | Self::Down)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Infrastructure,
    Media,
    NetworkSecurity,
    Apps,
}

/// Stored configuration of one service instance (secrets already decrypted).
#[derive(Clone)]
pub struct InstanceConfig {
    pub id: String,
    pub kind: ServiceKind,
    pub name: String,
    pub base_url: Option<String>,
    pub settings: Value,
    pub secrets: BTreeMap<String, String>,
}

impl std::fmt::Debug for InstanceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanceConfig")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("settings", &self.settings)
            .field("secrets", &self.secrets.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl InstanceConfig {
    pub fn setting_str(&self, key: &str) -> Option<&str> {
        self.settings
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    }

    pub fn setting_u64(&self, key: &str) -> Option<u64> {
        self.settings.get(key).and_then(Value::as_u64)
    }

    pub fn secret(&self, key: &str) -> Option<&str> {
        self.secrets
            .get(key)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }
}

/// What a module returns from one poll. The poller adds the id and timestamp.
#[derive(Debug, Clone)]
pub struct PollOutcome {
    pub health: Health,
    pub health_reason: Option<String>,
    pub response_ms: Option<u64>,
    pub version: Option<String>,
    pub data: ModuleData,
}

/// Cached state of one service instance: a common header plus module-specific data.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ServiceSnapshot {
    pub service_id: String,
    pub kind: ServiceKind,
    pub health: Health,
    /// Short human-readable reason when not healthy.
    pub health_reason: Option<String>,
    pub response_ms: Option<u64>,
    pub updated_at: DateTime<Utc>,
    pub version: Option<String>,
    /// True when the last poll failed and `data` is from an earlier successful poll.
    pub stale: bool,
    pub data: ModuleData,
}

/// Module-specific snapshot payload.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ModuleData {
    None,
    Proxmox(Box<proxmox::ProxmoxData>),
    HealthCheck(health_check::HealthCheckData),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActionDescriptor {
    pub id: String,
    pub label: String,
    /// Requires `"confirm": true` in the request body.
    pub destructive: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    /// Proxmox task id when the action started a task.
    pub task: Option<String>,
}

/// Result of testing a (possibly unsaved) service configuration.
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct ConnectionTest {
    pub ok: bool,
    pub message: String,
    pub response_ms: Option<u64>,
    pub version: Option<String>,
    /// SHA-256 fingerprint of the certificate the server presented, when it was not trusted.
    /// Shown to the owner so they can pin it after checking it on the host.
    pub presented_fingerprint: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum ModuleError {
    UnknownAction(String),
    InvalidArgs(String),
    Failed(anyhow::Error),
}

impl std::fmt::Display for ModuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownAction(a) => write!(f, "unknown action: {a}"),
            Self::InvalidArgs(m) => write!(f, "invalid arguments: {m}"),
            Self::Failed(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ModuleError {}

impl From<anyhow::Error> for ModuleError {
    fn from(e: anyhow::Error) -> Self {
        Self::Failed(e)
    }
}

#[async_trait]
pub trait ServiceModule: Send + Sync {
    fn kind(&self) -> ServiceKind;

    fn level(&self) -> IntegrationLevel {
        self.kind().level()
    }

    /// Fetch health and data. An `Err` is reported as `down` by the poller.
    async fn poll(&self) -> anyhow::Result<PollOutcome>;

    fn actions(&self) -> Vec<ActionDescriptor> {
        Vec::new()
    }

    async fn run_action(&self, action: &str, _args: Value) -> Result<ActionResult, ModuleError> {
        Err(ModuleError::UnknownAction(action.to_owned()))
    }

    async fn test_connection(&self) -> ConnectionTest;

    /// Proxmox access for the Proxmox endpoints; `None` for every other module.
    fn as_proxmox(&self) -> Option<&proxmox::ProxmoxModule> {
        None
    }
}

/// A short, user-facing description of a poll or connection error.
pub fn describe_error(err: &anyhow::Error) -> String {
    if let Some(e) = err.downcast_ref::<reqwest::Error>() {
        if e.is_timeout() {
            return "timeout".into();
        }
        if crate::tls::is_cert_error(e) {
            return "certificate not trusted".into();
        }
        if e.is_connect() {
            return "connection refused or host unreachable".into();
        }
    }
    err.to_string()
}

/// Check that a config has what its kind needs. Returns a message for the owner.
pub fn validate_config(cfg: &InstanceConfig) -> Result<(), String> {
    match cfg.kind {
        ServiceKind::Proxmox => proxmox::validate(cfg),
        ServiceKind::HealthCheck => health_check::validate(cfg),
    }
}

/// Build the module for an instance. In demo mode, modules return fake data.
pub fn build_module(cfg: &InstanceConfig, demo: bool) -> anyhow::Result<Arc<dyn ServiceModule>> {
    Ok(match (cfg.kind, demo) {
        (ServiceKind::Proxmox, false) => Arc::new(proxmox::ProxmoxModule::from_config(cfg)?),
        (ServiceKind::Proxmox, true) => Arc::new(proxmox::ProxmoxModule::demo()),
        (ServiceKind::HealthCheck, false) => Arc::new(health_check::HealthCheckModule::new(cfg)?),
        (ServiceKind::HealthCheck, true) => Arc::new(health_check::DemoHealthCheck::new(cfg)),
    })
}
