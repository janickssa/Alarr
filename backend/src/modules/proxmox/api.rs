//! Proxmox VE API access: raw response types, the [`ProxmoxApi`] trait and the HTTP client.
//!
//! Field names checked against the pve-manager API schema (`PVE/API2/Cluster.pm`,
//! `PVE/API2/Nodes.pm`). Only read endpoints and guest power endpoints are used:
//! the token needs `VM.Audit`, `Sys.Audit`, `Datastore.Audit` and `VM.PowerMgmt`.

use std::time::Duration;

use anyhow::{Context, bail};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::{GuestType, PowerAction, RrdRange};

/// Proxmox returns booleans as `0`/`1`, numbers sometimes as strings.
pub(crate) fn de_opt_bool<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::Bool(b)) => Some(b),
        Some(Value::Number(n)) => Some(n.as_f64().unwrap_or(0.0) != 0.0),
        Some(Value::String(s)) => Some(s == "1" || s.eq_ignore_ascii_case("true")),
        _ => None,
    })
}

pub(crate) fn de_opt_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    })
}

fn de_opt_u64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    Ok(de_opt_f64(d)?.filter(|v| *v >= 0.0).map(|v| v as u64))
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterStatusEntry {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub quorate: Option<bool>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub nodes: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub online: Option<bool>,
    pub ip: Option<String>,
}

/// One entry of `GET /cluster/resources`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClusterResource {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub node: Option<String>,
    pub status: Option<String>,
    pub name: Option<String>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub vmid: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub cpu: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub maxcpu: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub mem: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub maxmem: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub disk: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub maxdisk: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub uptime: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub netin: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub netout: Option<u64>,
    pub tags: Option<String>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub template: Option<bool>,
    pub lock: Option<String>,
    pub hastate: Option<String>,
    pub storage: Option<String>,
    pub plugintype: Option<String>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub shared: Option<bool>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MemInfo {
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub total: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub used: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CpuInfo {
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub cpus: Option<u64>,
    pub model: Option<String>,
}

/// `GET /nodes/{node}/status`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NodeStatus {
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub cpu: Option<f64>,
    #[serde(default)]
    pub cpuinfo: CpuInfo,
    #[serde(default)]
    pub memory: MemInfo,
    #[serde(default)]
    pub rootfs: MemInfo,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub uptime: Option<u64>,
    #[serde(default)]
    pub loadavg: Vec<Value>,
    pub pveversion: Option<String>,
    pub kversion: Option<String>,
}

/// `GET /nodes/{node}/{qemu|lxc}/{vmid}/status/current` (fields shared by both types).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GuestStatusCurrent {
    pub status: Option<String>,
    /// QEMU only: `running`, `paused`, `prelaunch`, …
    pub qmpstatus: Option<String>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub cpus: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub maxmem: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub maxdisk: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub uptime: Option<u64>,
    pub lock: Option<String>,
    #[serde(default)]
    pub ha: Value,
}

/// `GET /nodes/{node}/tasks/{upid}/status`.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskStatus {
    pub upid: Option<String>,
    pub node: Option<String>,
    /// `running` or `stopped`.
    pub status: String,
    /// `OK` or an error message once stopped.
    pub exitstatus: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub id: Option<String>,
    pub user: Option<String>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub starttime: Option<u64>,
}

/// One entry of `GET /nodes/{node}/tasks`.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskListEntry {
    pub upid: String,
    pub node: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub id: Option<String>,
    pub user: Option<String>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub starttime: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64")]
    pub endtime: Option<u64>,
    /// Exit status once finished (`OK`, error text); absent while running.
    pub status: Option<String>,
}

/// The Proxmox calls Quorum makes. Implemented over HTTP and by the demo backend.
#[async_trait]
pub trait ProxmoxApi: Send + Sync {
    async fn cluster_status(&self) -> anyhow::Result<Vec<ClusterStatusEntry>>;
    async fn resources(&self) -> anyhow::Result<Vec<ClusterResource>>;
    async fn node_status(&self, node: &str) -> anyhow::Result<NodeStatus>;
    async fn node_rrd(&self, node: &str, range: RrdRange) -> anyhow::Result<Vec<Value>>;
    async fn guest_rrd(
        &self,
        node: &str,
        ty: GuestType,
        vmid: u32,
        range: RrdRange,
    ) -> anyhow::Result<Vec<Value>>;
    async fn guest_status(
        &self,
        node: &str,
        ty: GuestType,
        vmid: u32,
    ) -> anyhow::Result<GuestStatusCurrent>;
    /// Start a power action. Returns the task UPID.
    async fn power(
        &self,
        node: &str,
        ty: GuestType,
        vmid: u32,
        action: PowerAction,
    ) -> anyhow::Result<String>;
    async fn task_status(&self, node: &str, upid: &str) -> anyhow::Result<TaskStatus>;
    async fn tasks(
        &self,
        node: &str,
        vmid: Option<u32>,
        limit: u32,
    ) -> anyhow::Result<Vec<TaskListEntry>>;
    /// Finished `vzdump` tasks on a node since a Unix time, newest first
    /// (`GET /nodes/{node}/tasks?typefilter=vzdump&since=`; needs `Sys.Audit`).
    async fn vzdump_tasks(&self, node: &str, since: i64) -> anyhow::Result<Vec<TaskListEntry>>;
    /// Lines of a task log (`GET /nodes/{node}/tasks/{upid}/log`; needs `Sys.Audit`).
    async fn task_log(&self, node: &str, upid: &str) -> anyhow::Result<Vec<String>>;
    /// Privileges of the token on `/` (for the connection test).
    async fn permissions(&self) -> anyhow::Result<Vec<String>>;
}

/// One line of `GET /nodes/{node}/tasks/{upid}/log`.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskLogLine {
    #[serde(default)]
    pub t: String,
}

/// Node names and UPIDs end up in URL paths; only allow the characters Proxmox uses.
pub fn safe_segment(value: &str) -> anyhow::Result<&str> {
    let ok = !value.is_empty()
        && value.len() <= 255
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | ':' | '@' | '!'));
    if !ok {
        bail!("invalid identifier");
    }
    Ok(value)
}

pub struct HttpProxmox {
    client: reqwest::Client,
    base: String,
    auth: String,
}

impl HttpProxmox {
    pub fn new(
        base_url: &str,
        token_id: &str,
        token_secret: &str,
        fingerprint: Option<&str>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            client: crate::tls::http_client(fingerprint, Duration::from_secs(10))?,
            base: format!("{}/api2/json", base_url.trim_end_matches('/')),
            auth: format!("PVEAPIToken={token_id}={token_secret}"),
        })
    }

    async fn send<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> anyhow::Result<T> {
        let response = req.header("Authorization", &self.auth).send().await?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            bail!("authentication failed (check the API token)");
        }
        if !status.is_success() {
            // Proxmox puts the reason in the status line, e.g.
            // "403 Permission check failed (/, Sys.Audit)".
            let reason = response
                .extensions()
                .get::<hyper::ext::ReasonPhrase>()
                .and_then(|r| std::str::from_utf8(r.as_bytes()).ok())
                .map(str::to_owned)
                .or_else(|| status.canonical_reason().map(str::to_owned))
                .unwrap_or_else(|| "error".into());
            bail!("{}", http_error(status.as_u16(), &reason));
        }
        let envelope: Envelope<T> = response
            .json()
            .await
            .context("unexpected Proxmox response")?;
        Ok(envelope.data)
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> anyhow::Result<T> {
        self.send(self.client.get(format!("{}{path}", self.base)).query(query))
            .await
    }
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

#[async_trait]
impl ProxmoxApi for HttpProxmox {
    async fn cluster_status(&self) -> anyhow::Result<Vec<ClusterStatusEntry>> {
        self.get("/cluster/status", &[]).await
    }

    async fn resources(&self) -> anyhow::Result<Vec<ClusterResource>> {
        self.get("/cluster/resources", &[]).await
    }

    async fn node_status(&self, node: &str) -> anyhow::Result<NodeStatus> {
        self.get(&format!("/nodes/{}/status", safe_segment(node)?), &[])
            .await
    }

    async fn node_rrd(&self, node: &str, range: RrdRange) -> anyhow::Result<Vec<Value>> {
        let query = [
            ("timeframe", range.as_str().to_owned()),
            ("cf", "AVERAGE".to_owned()),
        ];
        self.get(&format!("/nodes/{}/rrddata", safe_segment(node)?), &query)
            .await
    }

    async fn guest_rrd(
        &self,
        node: &str,
        ty: GuestType,
        vmid: u32,
        range: RrdRange,
    ) -> anyhow::Result<Vec<Value>> {
        let query = [
            ("timeframe", range.as_str().to_owned()),
            ("cf", "AVERAGE".to_owned()),
        ];
        let path = format!(
            "/nodes/{}/{}/{vmid}/rrddata",
            safe_segment(node)?,
            ty.as_str()
        );
        self.get(&path, &query).await
    }

    async fn guest_status(
        &self,
        node: &str,
        ty: GuestType,
        vmid: u32,
    ) -> anyhow::Result<GuestStatusCurrent> {
        let path = format!(
            "/nodes/{}/{}/{vmid}/status/current",
            safe_segment(node)?,
            ty.as_str()
        );
        self.get(&path, &[]).await
    }

    async fn power(
        &self,
        node: &str,
        ty: GuestType,
        vmid: u32,
        action: PowerAction,
    ) -> anyhow::Result<String> {
        let url = format!(
            "{}/nodes/{}/{}/{vmid}/status/{}",
            self.base,
            safe_segment(node)?,
            ty.as_str(),
            action.as_str()
        );
        let empty: [(&str, &str); 0] = [];
        self.send(self.client.post(url).form(&empty)).await
    }

    async fn task_status(&self, node: &str, upid: &str) -> anyhow::Result<TaskStatus> {
        let path = format!(
            "/nodes/{}/tasks/{}/status",
            safe_segment(node)?,
            safe_segment(upid)?
        );
        self.get(&path, &[]).await
    }

    async fn tasks(
        &self,
        node: &str,
        vmid: Option<u32>,
        limit: u32,
    ) -> anyhow::Result<Vec<TaskListEntry>> {
        let mut query = vec![("limit", limit.to_string())];
        if let Some(vmid) = vmid {
            query.push(("vmid", vmid.to_string()));
        }
        self.get(&format!("/nodes/{}/tasks", safe_segment(node)?), &query)
            .await
    }

    async fn vzdump_tasks(&self, node: &str, since: i64) -> anyhow::Result<Vec<TaskListEntry>> {
        self.get(
            &format!("/nodes/{}/tasks", safe_segment(node)?),
            &[
                ("typefilter", "vzdump".to_owned()),
                ("since", since.to_string()),
                ("limit", "500".to_owned()),
            ],
        )
        .await
    }

    async fn task_log(&self, node: &str, upid: &str) -> anyhow::Result<Vec<String>> {
        let lines: Vec<TaskLogLine> = self
            .get(
                &format!(
                    "/nodes/{}/tasks/{}/log",
                    safe_segment(node)?,
                    safe_segment(upid)?
                ),
                &[("limit", "50000".to_owned())],
            )
            .await?;
        Ok(lines.into_iter().map(|l| l.t).collect())
    }

    async fn permissions(&self) -> anyhow::Result<Vec<String>> {
        let perms: serde_json::Map<String, Value> = self
            .get("/access/permissions", &[("path", "/".to_owned())])
            .await?;
        Ok(perms
            .get("/")
            .and_then(Value::as_object)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default())
    }
}

/// The message for a failed Proxmox call, with a hint for missing privileges.
fn http_error(code: u16, reason: &str) -> String {
    let hint = if code == 403 {
        " — the API token lacks a privilege. With privilege separation, give QuorumRole to \
         both the user and the token (deploy/README.md, section 1)"
    } else {
        ""
    };
    format!("Proxmox returned HTTP {code} {reason}{hint}")
}

#[cfg(test)]
mod error_tests {
    #[test]
    fn forbidden_explains_acls() {
        let m = super::http_error(403, "Permission check failed (/, Sys.Audit)");
        assert!(m.starts_with("Proxmox returned HTTP 403 Permission check failed (/, Sys.Audit)"));
        assert!(m.contains("both the user and the token"), "{m}");
        assert_eq!(
            super::http_error(500, "boom"),
            "Proxmox returned HTTP 500 boom"
        );
    }
}
