//! Service modules: the shared trait, common types and the module registry.

pub mod arr;
pub mod authentik;
pub mod bazarr;
pub mod catalogue;
pub mod health_check;
pub mod http;
pub mod jellyfin;
pub mod media_demo;
pub mod navidrome;
pub mod nextcloud;
pub mod npm;
pub mod prowlarr;
pub mod proxmox;
pub mod qbittorrent;
pub mod seerr;
pub mod technitium;

use std::collections::{BTreeMap, HashMap};
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
    Jellyfin,
    Seerr,
    Sonarr,
    Radarr,
    Prowlarr,
    Bazarr,
    Qbittorrent,
    Navidrome,
    Technitium,
    NginxProxyManager,
    Authentik,
    Nextcloud,
}

impl ServiceKind {
    pub const ALL: [ServiceKind; 14] = [
        Self::Proxmox,
        Self::HealthCheck,
        Self::Jellyfin,
        Self::Seerr,
        Self::Sonarr,
        Self::Radarr,
        Self::Prowlarr,
        Self::Bazarr,
        Self::Qbittorrent,
        Self::Navidrome,
        Self::Technitium,
        Self::NginxProxyManager,
        Self::Authentik,
        Self::Nextcloud,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proxmox => "proxmox",
            Self::HealthCheck => "health_check",
            Self::Jellyfin => "jellyfin",
            Self::Seerr => "seerr",
            Self::Sonarr => "sonarr",
            Self::Radarr => "radarr",
            Self::Prowlarr => "prowlarr",
            Self::Bazarr => "bazarr",
            Self::Qbittorrent => "qbittorrent",
            Self::Navidrome => "navidrome",
            Self::Technitium => "technitium",
            Self::NginxProxyManager => "nginx_proxy_manager",
            Self::Authentik => "authentik",
            Self::Nextcloud => "nextcloud",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == value)
    }

    pub fn level(self) -> IntegrationLevel {
        match self {
            Self::HealthCheck => IntegrationLevel::HealthCheck,
            Self::NginxProxyManager | Self::Authentik | Self::Nextcloud => {
                IntegrationLevel::ReadOnly
            }
            _ => IntegrationLevel::Full,
        }
    }

    /// Default polling interval and the faster one used while a client is viewing it.
    pub fn poll_intervals(self) -> (std::time::Duration, std::time::Duration) {
        use std::time::Duration;
        match self {
            Self::Proxmox => (Duration::from_secs(10), Duration::from_secs(3)),
            Self::HealthCheck => (Duration::from_secs(30), Duration::from_secs(30)),
            Self::Qbittorrent => (Duration::from_secs(10), Duration::from_secs(2)),
            Self::Jellyfin | Self::Navidrome => (Duration::from_secs(15), Duration::from_secs(5)),
            Self::Seerr
            | Self::Sonarr
            | Self::Radarr
            | Self::Prowlarr
            | Self::Bazarr
            | Self::Authentik => (Duration::from_secs(60), Duration::from_secs(15)),
            Self::Technitium => (Duration::from_secs(30), Duration::from_secs(10)),
            Self::Nextcloud => (Duration::from_secs(120), Duration::from_secs(30)),
            // Certificates change slowly.
            Self::NginxProxyManager => (Duration::from_secs(300), Duration::from_secs(60)),
        }
    }

    /// Names of the secrets this kind uses (stored encrypted).
    pub fn secret_names(self) -> &'static [&'static str] {
        match self {
            Self::Proxmox => &["token_secret"],
            Self::HealthCheck => &[],
            Self::Qbittorrent | Self::Navidrome | Self::NginxProxyManager | Self::Nextcloud => {
                &["password"]
            }
            _ => &["api_key"],
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
    Jellyfin(Box<jellyfin::JellyfinData>),
    Seerr(Box<seerr::SeerrData>),
    /// Sonarr and Radarr.
    Arr(Box<arr::ArrData>),
    Prowlarr(Box<prowlarr::ProwlarrData>),
    Bazarr(Box<bazarr::BazarrData>),
    Qbittorrent(Box<qbittorrent::QbittorrentData>),
    Navidrome(Box<navidrome::NavidromeData>),
    Technitium(Box<technitium::TechnitiumData>),
    /// Nginx Proxy Manager.
    Npm(Box<npm::NpmData>),
    Authentik(Box<authentik::AuthentikData>),
    Nextcloud(Box<nextcloud::NextcloudData>),
}

impl ModuleData {
    /// Card headline and detail line for Home, when the module has data for it.
    pub fn headline(&self) -> Option<(String, Option<String>)> {
        Some(match self {
            Self::Jellyfin(d) => d.headline(),
            Self::Seerr(d) => d.headline(),
            Self::Arr(d) => d.headline(),
            Self::Prowlarr(d) => d.headline(),
            Self::Bazarr(d) => d.headline(),
            Self::Qbittorrent(d) => d.headline(),
            Self::Navidrome(d) => d.headline(),
            Self::Technitium(d) => d.headline(),
            Self::Npm(d) => d.headline(),
            Self::Authentik(d) => d.headline(),
            Self::Nextcloud(d) => d.headline(),
            Self::None | Self::Proxmox(_) | Self::HealthCheck(_) => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActionDescriptor {
    pub id: String,
    pub label: String,
    /// Requires `"confirm": true` in the request body.
    pub destructive: bool,
}

impl ActionDescriptor {
    pub fn new(id: &str, label: &str, destructive: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            destructive,
        }
    }
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

    /// Read-only queries served by `GET /services/{id}/query/{name}` (e.g. `library`, `search`).
    fn queries(&self) -> Vec<&'static str> {
        Vec::new()
    }

    async fn query(
        &self,
        name: &str,
        _args: &HashMap<String, String>,
    ) -> Result<Value, ModuleError> {
        Err(ModuleError::UnknownAction(name.to_owned()))
    }

    /// Fetch an image (poster, cover) from the service, for `GET /services/{id}/image`.
    async fn image(&self, _path: &str) -> Result<(bytes::Bytes, String), ModuleError> {
        Err(ModuleError::UnknownAction("images".into()))
    }

    /// Proxmox access for the Proxmox endpoints; `None` for every other module.
    fn as_proxmox(&self) -> Option<&proxmox::ProxmoxModule> {
        None
    }

    /// Sonarr/Radarr library access for the media endpoints.
    fn as_arr(&self) -> Option<&arr::ArrModule> {
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
        ServiceKind::Jellyfin => jellyfin::validate(cfg),
        ServiceKind::Seerr => seerr::validate(cfg),
        ServiceKind::Sonarr | ServiceKind::Radarr => arr::validate(cfg),
        ServiceKind::Prowlarr => prowlarr::validate(cfg),
        ServiceKind::Bazarr => bazarr::validate(cfg),
        ServiceKind::Qbittorrent => qbittorrent::validate(cfg),
        ServiceKind::Navidrome => navidrome::validate(cfg),
        ServiceKind::Technitium => technitium::validate(cfg),
        ServiceKind::NginxProxyManager => npm::validate(cfg),
        ServiceKind::Authentik => authentik::validate(cfg),
        ServiceKind::Nextcloud => nextcloud::validate(cfg),
    }
}

/// Build the module for an instance. In demo mode, modules return fake data.
pub fn build_module(cfg: &InstanceConfig, demo: bool) -> anyhow::Result<Arc<dyn ServiceModule>> {
    Ok(match (cfg.kind, demo) {
        (ServiceKind::Proxmox, false) => Arc::new(proxmox::ProxmoxModule::from_config(cfg)?),
        (ServiceKind::Proxmox, true) => Arc::new(proxmox::ProxmoxModule::demo()),
        (ServiceKind::HealthCheck, false) => Arc::new(health_check::HealthCheckModule::new(cfg)?),
        (ServiceKind::HealthCheck, true) => Arc::new(health_check::DemoHealthCheck::new(cfg)),
        (ServiceKind::Jellyfin, false) => Arc::new(jellyfin::JellyfinModule::new(cfg)?),
        (ServiceKind::Jellyfin, true) => Arc::new(jellyfin::JellyfinModule::demo(&cfg.id)),
        (ServiceKind::Seerr, false) => Arc::new(seerr::SeerrModule::new(cfg)?),
        (ServiceKind::Seerr, true) => Arc::new(seerr::SeerrModule::demo()),
        (ServiceKind::Sonarr, false) => Arc::new(arr::ArrModule::new(cfg, arr::ArrFlavor::Sonarr)?),
        (ServiceKind::Sonarr, true) => Arc::new(arr::ArrModule::demo(arr::ArrFlavor::Sonarr)),
        (ServiceKind::Radarr, false) => Arc::new(arr::ArrModule::new(cfg, arr::ArrFlavor::Radarr)?),
        (ServiceKind::Radarr, true) => Arc::new(arr::ArrModule::demo(arr::ArrFlavor::Radarr)),
        (ServiceKind::Prowlarr, false) => Arc::new(prowlarr::ProwlarrModule::new(cfg)?),
        (ServiceKind::Prowlarr, true) => Arc::new(prowlarr::ProwlarrModule::demo()),
        (ServiceKind::Bazarr, false) => Arc::new(bazarr::BazarrModule::new(cfg)?),
        (ServiceKind::Bazarr, true) => Arc::new(bazarr::BazarrModule::demo()),
        (ServiceKind::Qbittorrent, false) => Arc::new(qbittorrent::QbittorrentModule::new(cfg)?),
        (ServiceKind::Qbittorrent, true) => Arc::new(qbittorrent::QbittorrentModule::demo()),
        (ServiceKind::Navidrome, false) => Arc::new(navidrome::NavidromeModule::new(cfg)?),
        (ServiceKind::Navidrome, true) => Arc::new(navidrome::NavidromeModule::demo()),
        (ServiceKind::Technitium, false) => Arc::new(technitium::TechnitiumModule::new(cfg)?),
        (ServiceKind::Technitium, true) => Arc::new(technitium::TechnitiumModule::demo()),
        (ServiceKind::NginxProxyManager, false) => Arc::new(npm::NpmModule::new(cfg)?),
        (ServiceKind::NginxProxyManager, true) => Arc::new(npm::NpmModule::demo()),
        (ServiceKind::Authentik, false) => Arc::new(authentik::AuthentikModule::new(cfg)?),
        (ServiceKind::Authentik, true) => Arc::new(authentik::AuthentikModule::demo()),
        (ServiceKind::Nextcloud, false) => Arc::new(nextcloud::NextcloudModule::new(cfg)?),
        (ServiceKind::Nextcloud, true) => Arc::new(nextcloud::NextcloudModule::demo()),
    })
}
