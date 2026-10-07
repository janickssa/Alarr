//! Proxmox VE module: cluster, nodes, storage, guests, RRD graphs, tasks and power actions.

pub mod api;
pub mod demo;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use self::api::{ClusterResource, ClusterStatusEntry, HttpProxmox, NodeStatus, ProxmoxApi};
use super::{
    ConnectionTest, Health, InstanceConfig, ModuleData, PollOutcome, ServiceKind, ServiceModule,
};

/// Privileges the Quorum role should have: exactly these, nothing more.
pub const REQUIRED_PRIVILEGES: &[&str] =
    &["VM.Audit", "Sys.Audit", "Datastore.Audit", "VM.PowerMgmt"];

/// Settings of a Proxmox instance (the `settings` object of the service).
/// The token secret is stored as the encrypted secret `token_secret`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, ToSchema)]
pub struct ProxmoxSettings {
    /// API token id, e.g. `quorum@pve!quorum`.
    pub token_id: String,
    /// SHA-256 fingerprint of the Proxmox certificate to pin (self-signed certificates).
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum GuestType {
    Qemu,
    Lxc,
}

impl GuestType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Qemu => "qemu",
            Self::Lxc => "lxc",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PowerAction {
    Start,
    Shutdown,
    Stop,
    Reboot,
}

impl PowerAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Shutdown => "shutdown",
            Self::Stop => "stop",
            Self::Reboot => "reboot",
        }
    }

    /// Needs `"confirm": true`.
    pub fn destructive(self) -> bool {
        !matches!(self, Self::Start)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RrdRange {
    #[default]
    Hour,
    Day,
    Week,
}

impl RrdRange {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum GuestStatus {
    Running,
    Stopped,
    Paused,
    Unknown,
}

impl GuestStatus {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("running") => Self::Running,
            Some("stopped") => Self::Stopped,
            Some("paused" | "suspended") => Self::Paused,
            _ => Self::Unknown,
        }
    }

    pub fn health(self) -> Health {
        match self {
            Self::Running => Health::Healthy,
            Self::Paused => Health::Warning,
            // A stopped guest is usually stopped on purpose. A linked service still
            // reports down when its guest is not running.
            Self::Stopped => Health::Disabled,
            Self::Unknown => Health::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProxmoxData {
    pub cluster: ClusterInfo,
    pub nodes: Vec<Node>,
    pub guests: Vec<Guest>,
    pub storage: Vec<StoragePool>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ClusterInfo {
    /// Cluster name; `None` for a standalone node.
    pub name: Option<String>,
    pub quorate: bool,
    pub nodes_online: u32,
    pub nodes_total: u32,
    pub pve_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Node {
    pub name: String,
    pub online: bool,
    pub ip: Option<String>,
    /// CPU usage, 0.0–1.0.
    pub cpu_usage: Option<f64>,
    pub cpu_count: Option<u64>,
    pub cpu_model: Option<String>,
    /// Bytes.
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    /// Root filesystem, bytes.
    pub disk_used: Option<u64>,
    pub disk_total: Option<u64>,
    pub uptime_s: Option<u64>,
    /// 1, 5 and 15 minute load averages.
    pub load_avg: Option<Vec<f64>>,
    /// Not exposed by the Proxmox API; filled by a future sensor source.
    pub temperature_c: Option<f64>,
    pub pve_version: Option<String>,
    pub kernel: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Guest {
    pub vmid: u32,
    pub name: String,
    pub node: String,
    pub guest_type: GuestType,
    pub status: GuestStatus,
    /// CPU usage, 0.0–1.0 of the guest's cores.
    pub cpu_usage: Option<f64>,
    pub cpu_count: Option<f64>,
    /// Bytes.
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub disk_used: Option<u64>,
    pub disk_total: Option<u64>,
    pub uptime_s: Option<u64>,
    /// Total bytes received/sent since start.
    pub net_in: Option<u64>,
    pub net_out: Option<u64>,
    pub tags: Vec<String>,
    pub template: bool,
    pub lock: Option<String>,
    pub ha_state: Option<String>,
    /// Quorum services linked to this guest (by service config or `svc-<id>` tag).
    pub linked_services: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StoragePool {
    /// `<node>/<storage>`.
    pub id: String,
    pub storage: String,
    pub node: String,
    pub plugin_type: Option<String>,
    /// Bytes.
    pub used: Option<u64>,
    pub total: Option<u64>,
    pub shared: bool,
    pub active: bool,
    pub content: Vec<String>,
}

impl StoragePool {
    pub fn usage(&self) -> Option<f64> {
        match (self.used, self.total) {
            (Some(u), Some(t)) if t > 0 => Some(u as f64 / t as f64),
            _ => None,
        }
    }
}

/// One RRD sample. Units: CPU 0.0–1.0, memory/disk in bytes, network/disk IO in bytes per second.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RrdPoint {
    pub time: DateTime<Utc>,
    pub cpu: Option<f64>,
    pub mem_used: Option<f64>,
    pub mem_total: Option<f64>,
    pub net_in: Option<f64>,
    pub net_out: Option<f64>,
    pub disk_used: Option<f64>,
    pub disk_total: Option<f64>,
    pub disk_read: Option<f64>,
    pub disk_write: Option<f64>,
    pub load_avg: Option<f64>,
    pub io_wait: Option<f64>,
}

impl RrdPoint {
    /// Normalise node (`memused`, `rootused`, …) and guest (`mem`, `disk`, …) RRD rows.
    pub fn from_raw(row: &Value) -> Option<Self> {
        let f = |k: &str| match row.get(k) {
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::String(s)) => s.parse().ok(),
            _ => None,
        };
        let either = |a: &str, b: &str| f(a).or_else(|| f(b));
        Some(Self {
            time: DateTime::from_timestamp(f("time")? as i64, 0)?,
            cpu: f("cpu"),
            mem_used: either("memused", "mem"),
            mem_total: either("memtotal", "maxmem"),
            net_in: f("netin"),
            net_out: f("netout"),
            disk_used: either("rootused", "disk"),
            disk_total: either("roottotal", "maxdisk"),
            disk_read: f("diskread"),
            disk_write: f("diskwrite"),
            load_avg: f("loadavg"),
            io_wait: f("iowait"),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Running,
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Task {
    pub upid: String,
    pub node: String,
    /// Task type, e.g. `vzstart`, `qmshutdown`, `vzdump`.
    pub kind: Option<String>,
    /// Usually the vmid.
    pub id: Option<String>,
    pub user: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub state: TaskState,
    /// Exit status text (`OK` or the error).
    pub exit_status: Option<String>,
}

fn ts(secs: Option<u64>) -> Option<DateTime<Utc>> {
    secs.and_then(|s| DateTime::from_timestamp(s as i64, 0))
}

impl Task {
    pub fn from_status(upid: &str, node: &str, s: api::TaskStatus) -> Self {
        let state = match (s.status.as_str(), s.exitstatus.as_deref()) {
            ("running", _) => TaskState::Running,
            (_, Some("OK")) => TaskState::Ok,
            _ => TaskState::Error,
        };
        Self {
            upid: s.upid.unwrap_or_else(|| upid.to_owned()),
            node: s.node.unwrap_or_else(|| node.to_owned()),
            kind: s.kind,
            id: s.id,
            user: s.user,
            started_at: ts(s.starttime),
            ended_at: None,
            state,
            exit_status: s.exitstatus,
        }
    }

    pub fn from_list(node: &str, t: api::TaskListEntry) -> Self {
        let state = match (t.endtime, t.status.as_deref()) {
            (None, _) => TaskState::Running,
            (_, Some("OK")) => TaskState::Ok,
            _ => TaskState::Error,
        };
        Self {
            upid: t.upid,
            node: t.node.unwrap_or_else(|| node.to_owned()),
            kind: t.kind,
            id: t.id,
            user: t.user,
            started_at: ts(t.starttime),
            ended_at: ts(t.endtime),
            state,
            exit_status: t.status,
        }
    }
}

/// The node a task runs on, from its UPID (`UPID:<node>:…`).
pub fn upid_node(upid: &str) -> Option<&str> {
    let mut parts = upid.split(':');
    (parts.next()? == "UPID").then_some(())?;
    parts.next().filter(|n| !n.is_empty())
}

fn pve_version(raw: Option<&str>) -> Option<String> {
    // "pve-manager/8.2.4/faa83925c9641325" -> "8.2.4"
    raw.map(|v| v.split('/').nth(1).unwrap_or(v).to_owned())
}

fn pct(value: f64) -> String {
    format!("{:.0} %", value * 100.0)
}

/// Turn raw API responses into the snapshot and its health.
pub fn build_data(
    status: &[ClusterStatusEntry],
    resources: &[ClusterResource],
    node_status: &HashMap<String, NodeStatus>,
) -> (ProxmoxData, Health, Option<String>) {
    let cluster_entry = status.iter().find(|e| e.kind == "cluster");
    let ips: HashMap<&str, Option<&String>> = status
        .iter()
        .filter(|e| e.kind == "node")
        .map(|e| (e.name.as_str(), e.ip.as_ref()))
        .collect();

    let mut nodes: Vec<Node> = resources
        .iter()
        .filter(|r| r.kind == "node")
        .filter_map(|r| {
            let name = r.node.clone()?;
            let ns = node_status.get(&name);
            Some(Node {
                online: r.status.as_deref() == Some("online"),
                ip: ips.get(name.as_str()).and_then(|ip| ip.cloned()),
                cpu_usage: ns.and_then(|s| s.cpu).or(r.cpu),
                cpu_count: ns
                    .and_then(|s| s.cpuinfo.cpus)
                    .or(r.maxcpu.map(|c| c as u64)),
                cpu_model: ns.and_then(|s| s.cpuinfo.model.clone()),
                mem_used: ns.and_then(|s| s.memory.used).or(r.mem),
                mem_total: ns.and_then(|s| s.memory.total).or(r.maxmem),
                disk_used: ns.and_then(|s| s.rootfs.used).or(r.disk),
                disk_total: ns.and_then(|s| s.rootfs.total).or(r.maxdisk),
                uptime_s: r.uptime.filter(|u| *u > 0),
                load_avg: ns.map(|s| {
                    s.loadavg
                        .iter()
                        .filter_map(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
                        .collect()
                }),
                temperature_c: None,
                pve_version: pve_version(ns.and_then(|s| s.pveversion.as_deref())),
                kernel: ns.and_then(|s| s.kversion.clone()),
                name,
            })
        })
        .collect();
    nodes.sort_by(|a, b| a.name.cmp(&b.name));

    let mut guests: Vec<Guest> = resources
        .iter()
        .filter(|r| r.kind == "qemu" || r.kind == "lxc")
        .filter_map(|r| {
            let vmid = u32::try_from(r.vmid?).ok()?;
            Some(Guest {
                vmid,
                name: r.name.clone().unwrap_or_else(|| vmid.to_string()),
                node: r.node.clone().unwrap_or_default(),
                guest_type: if r.kind == "qemu" {
                    GuestType::Qemu
                } else {
                    GuestType::Lxc
                },
                status: GuestStatus::parse(r.status.as_deref()),
                cpu_usage: r.cpu,
                cpu_count: r.maxcpu,
                mem_used: r.mem,
                mem_total: r.maxmem,
                disk_used: r.disk.filter(|d| *d > 0),
                disk_total: r.maxdisk,
                uptime_s: r.uptime.filter(|u| *u > 0),
                net_in: r.netin,
                net_out: r.netout,
                tags: r
                    .tags
                    .as_deref()
                    .unwrap_or_default()
                    .split([';', ',', ' '])
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned)
                    .collect(),
                template: r.template.unwrap_or(false),
                lock: r.lock.clone().filter(|l| !l.is_empty()),
                ha_state: r.hastate.clone(),
                linked_services: Vec::new(),
            })
        })
        .collect();
    guests.sort_by_key(|g| g.vmid);

    let mut storage: Vec<StoragePool> = resources
        .iter()
        .filter(|r| r.kind == "storage")
        .filter_map(|r| {
            let node = r.node.clone()?;
            let name = r.storage.clone()?;
            Some(StoragePool {
                id: format!("{node}/{name}"),
                storage: name,
                node,
                plugin_type: r.plugintype.clone(),
                used: r.disk,
                total: r.maxdisk,
                shared: r.shared.unwrap_or(false),
                active: r.status.as_deref() == Some("available"),
                content: r
                    .content
                    .as_deref()
                    .unwrap_or_default()
                    .split(',')
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned)
                    .collect(),
            })
        })
        .collect();
    storage.sort_by(|a, b| a.id.cmp(&b.id));

    let nodes_online = nodes.iter().filter(|n| n.online).count() as u32;
    let cluster = ClusterInfo {
        name: cluster_entry.map(|c| c.name.clone()),
        // A standalone node has no cluster entry and is always quorate.
        quorate: cluster_entry.and_then(|c| c.quorate).unwrap_or(true),
        nodes_online,
        nodes_total: nodes.len() as u32,
        pve_version: nodes.iter().find_map(|n| n.pve_version.clone()),
    };

    let mut problems = Vec::new();
    if !cluster.quorate {
        problems.push((Health::Down, "cluster not quorate".to_owned()));
    }
    for n in nodes.iter().filter(|n| !n.online) {
        problems.push((Health::Warning, format!("{} offline", n.name)));
    }
    for s in storage.iter().filter(|s| s.active) {
        if let Some(u) = s.usage().filter(|u| *u >= 0.9) {
            problems.push((Health::Warning, format!("storage {} at {}", s.id, pct(u))));
        }
    }
    let health = if problems.iter().any(|(h, _)| *h == Health::Down) {
        Health::Down
    } else if problems.is_empty() {
        Health::Healthy
    } else {
        Health::Warning
    };
    let reason = (!problems.is_empty()).then(|| {
        problems
            .into_iter()
            .map(|(_, r)| r)
            .collect::<Vec<_>>()
            .join(", ")
    });

    (
        ProxmoxData {
            cluster,
            nodes,
            guests,
            storage,
        },
        health,
        reason,
    )
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    let url = cfg.base_url.as_deref().ok_or("base URL is required")?;
    if !url.starts_with("https://") {
        return Err("Proxmox base URL must start with https://".into());
    }
    let s: ProxmoxSettings = serde_json::from_value(cfg.settings.clone())
        .map_err(|e| format!("invalid settings: {e}"))?;
    if !s.token_id.contains('!') || !s.token_id.contains('@') {
        return Err("token_id must look like user@realm!tokenname".into());
    }
    if let Some(fp) = &s.fingerprint {
        crate::tls::parse_fingerprint(fp).map_err(|e| e.to_string())?;
    }
    if cfg.secret("token_secret").is_none() {
        return Err("token_secret is required".into());
    }
    Ok(())
}

pub struct ProxmoxModule {
    api: Arc<dyn ProxmoxApi>,
    base_url: Option<String>,
}

impl ProxmoxModule {
    pub fn from_config(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        let s: ProxmoxSettings = serde_json::from_value(cfg.settings.clone())?;
        let base_url = cfg.base_url.clone().unwrap_or_default();
        let api = HttpProxmox::new(
            &base_url,
            &s.token_id,
            cfg.secret("token_secret").unwrap_or_default(),
            s.fingerprint.as_deref(),
        )?;
        Ok(Self {
            api: Arc::new(api),
            base_url: Some(base_url),
        })
    }

    pub fn demo() -> Self {
        Self::with_api(Arc::new(demo::DemoProxmox::new()))
    }

    pub fn with_api(api: Arc<dyn ProxmoxApi>) -> Self {
        Self {
            api,
            base_url: None,
        }
    }

    pub fn api(&self) -> &dyn ProxmoxApi {
        self.api.as_ref()
    }

    async fn fetch(&self) -> anyhow::Result<(ProxmoxData, Health, Option<String>, u64)> {
        let started = Instant::now();
        let (status, resources) =
            tokio::try_join!(self.api.cluster_status(), self.api.resources())?;
        let elapsed = started.elapsed().as_millis() as u64;
        let online: Vec<String> = resources
            .iter()
            .filter(|r| r.kind == "node" && r.status.as_deref() == Some("online"))
            .filter_map(|r| r.node.clone())
            .collect();
        let results =
            futures_util::future::join_all(online.iter().map(|n| self.api.node_status(n))).await;
        let node_status: HashMap<String, NodeStatus> = online
            .into_iter()
            .zip(results)
            .filter_map(|(n, r)| r.ok().map(|s| (n, s)))
            .collect();
        let (data, health, reason) = build_data(&status, &resources, &node_status);
        Ok((data, health, reason, elapsed))
    }
}

#[async_trait]
impl ServiceModule for ProxmoxModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Proxmox
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let (data, health, health_reason, ms) = self.fetch().await?;
        Ok(PollOutcome {
            health,
            health_reason,
            response_ms: Some(ms),
            version: data.cluster.pve_version.clone(),
            data: ModuleData::Proxmox(Box::new(data)),
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((data, _, _, ms)) => {
                let mut warnings = Vec::new();
                match self.api.permissions().await {
                    Ok(privs) => {
                        let missing: Vec<_> = REQUIRED_PRIVILEGES
                            .iter()
                            .filter(|p| !privs.iter().any(|q| q == *p))
                            .collect();
                        if !missing.is_empty() {
                            warnings.push(format!(
                                "token is missing privileges: {}",
                                missing.iter().map(|s| **s).collect::<Vec<_>>().join(", ")
                            ));
                        }
                        let mut extra: Vec<_> = privs
                            .iter()
                            .filter(|p| !REQUIRED_PRIVILEGES.contains(&p.as_str()))
                            .cloned()
                            .collect();
                        extra.sort();
                        if !extra.is_empty() {
                            warnings.push(format!(
                                "token has more privileges than Quorum needs: {}",
                                extra.join(", ")
                            ));
                        }
                    }
                    Err(e) => warnings.push(format!("could not read token privileges: {e}")),
                }
                ConnectionTest {
                    ok: true,
                    message: format!(
                        "{} nodes, {} guests",
                        data.cluster.nodes_total,
                        data.guests.len()
                    ),
                    response_ms: Some(ms),
                    version: data.cluster.pve_version,
                    presented_fingerprint: None,
                    warnings,
                }
            }
            Err(e) => {
                let cert = e
                    .downcast_ref::<reqwest::Error>()
                    .is_some_and(crate::tls::is_cert_error);
                let presented_fingerprint = match (&self.base_url, cert) {
                    (Some(url), true) => crate::tls::fetch_fingerprint(url).await,
                    _ => None,
                };
                ConnectionTest {
                    ok: false,
                    message: if cert {
                        "certificate not trusted: pin its fingerprint".into()
                    } else {
                        e.to_string()
                    },
                    presented_fingerprint,
                    ..Default::default()
                }
            }
        }
    }

    fn as_proxmox(&self) -> Option<&ProxmoxModule> {
        Some(self)
    }
}

#[cfg(test)]
mod tests;
