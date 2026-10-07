//! Demo Proxmox backend: a fake three-node cluster with the owner's containers.
//! Power actions succeed after a short delay.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use rand::Rng;
use serde_json::{Value, json};

use super::api::{
    ClusterResource, ClusterStatusEntry, CpuInfo, GuestStatusCurrent, MemInfo, NodeStatus,
    ProxmoxApi, TaskListEntry, TaskStatus,
};
use super::{GuestType, PowerAction, RrdRange};

const GB: u64 = 1024 * 1024 * 1024;

/// (vmid, name, node, type, cores, memory GB, tags)
const GUESTS: &[(u32, &str, &str, GuestType, u32, u64, &str)] = &[
    (102, "wg-home", "pve-01", GuestType::Lxc, 1, 1, "network"),
    (
        103,
        "apt-cacher-ng",
        "pve-03",
        GuestType::Lxc,
        1,
        1,
        "infra;svc-apt-cacher-ng",
    ),
    (104, "lxc-updater", "pve-03", GuestType::Lxc, 1, 1, "infra"),
    (105, "jellyfin-pub", "pve-02", GuestType::Lxc, 4, 4, "media"),
    (108, "jellyfin", "pve-01", GuestType::Lxc, 4, 8, "media"),
    (
        111,
        "smb-share",
        "pve-03",
        GuestType::Lxc,
        1,
        1,
        "infra;svc-smb-share",
    ),
    (112, "technitium", "pve-01", GuestType::Lxc, 2, 2, "network"),
    (113, "nextcloud", "pve-02", GuestType::Lxc, 2, 4, "apps"),
    (
        114,
        "vaultwarden",
        "pve-01",
        GuestType::Lxc,
        1,
        1,
        "network",
    ),
    (
        115,
        "nginx-proxy-manager",
        "pve-01",
        GuestType::Lxc,
        1,
        1,
        "network",
    ),
    (116, "drawio", "pve-03", GuestType::Lxc, 1, 1, "apps"),
    (
        117,
        "bookstack",
        "pve-03",
        GuestType::Lxc,
        1,
        2,
        "apps;svc-bookstack",
    ),
    (118, "podgrab", "pve-03", GuestType::Lxc, 1, 1, "media"),
    (119, "navidrome", "pve-02", GuestType::Lxc, 2, 2, "media"),
    (120, "qbittorrent", "pve-01", GuestType::Lxc, 2, 4, "media"),
    (121, "authentik", "pve-02", GuestType::Lxc, 2, 4, "network"),
    (
        122,
        "kiwix",
        "pve-03",
        GuestType::Lxc,
        1,
        2,
        "apps;svc-kiwix",
    ),
    (123, "sonarr", "pve-02", GuestType::Lxc, 2, 2, "media"),
    (124, "radarr", "pve-02", GuestType::Lxc, 2, 2, "media"),
    (125, "prowlarr", "pve-02", GuestType::Lxc, 1, 1, "media"),
    (126, "seerr", "pve-02", GuestType::Lxc, 1, 2, "media"),
    (127, "bazarr", "pve-02", GuestType::Lxc, 1, 1, "media"),
    (
        128,
        "mkdocs",
        "pve-03",
        GuestType::Lxc,
        1,
        1,
        "apps;svc-mkdocs",
    ),
    (200, "haos", "pve-01", GuestType::Qemu, 2, 4, "home"),
    (201, "win11-lab", "pve-03", GuestType::Qemu, 4, 8, "lab"),
];

/// (name, cores, memory GB, base CPU load)
const NODES: &[(&str, u64, u64, f64)] = &[
    ("pve-01", 16, 64, 0.18),
    ("pve-02", 12, 64, 0.24),
    ("pve-03", 8, 32, 0.09),
];

#[derive(Debug, Clone)]
struct DemoGuest {
    vmid: u32,
    name: &'static str,
    node: &'static str,
    ty: GuestType,
    cores: u32,
    mem_gb: u64,
    tags: &'static str,
    running: bool,
    started_at: i64,
}

#[derive(Debug, Clone)]
struct DemoTask {
    node: String,
    kind: String,
    vmid: u32,
    started: i64,
    ended: Option<i64>,
}

#[derive(Default)]
struct State {
    guests: Vec<DemoGuest>,
    tasks: HashMap<String, DemoTask>,
    counter: u32,
}

#[derive(Clone)]
pub struct DemoProxmox {
    state: Arc<Mutex<State>>,
    delay: Duration,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Default for DemoProxmox {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoProxmox {
    pub fn new() -> Self {
        Self::with_delay(Duration::from_secs(2))
    }

    pub fn with_delay(delay: Duration) -> Self {
        let boot = now() - 12 * 86_400;
        let guests = GUESTS
            .iter()
            .map(|&(vmid, name, node, ty, cores, mem_gb, tags)| DemoGuest {
                vmid,
                name,
                node,
                ty,
                cores,
                mem_gb,
                tags,
                running: vmid != 201,
                started_at: boot + (vmid as i64 % 17) * 3600,
            })
            .collect();
        Self {
            state: Arc::new(Mutex::new(State {
                guests,
                ..Default::default()
            })),
            delay,
        }
    }

    fn jitter(base: f64, spread: f64) -> f64 {
        (base + rand::thread_rng().gen_range(-spread..spread)).clamp(0.0, 1.0)
    }

    fn rrd(range: RrdRange, f: impl Fn(f64, i64) -> Value) -> Vec<Value> {
        let step = match range {
            RrdRange::Hour => 60,
            RrdRange::Day => 1200,
            RrdRange::Week => 10_800,
        };
        let end = now() / step * step;
        (0..70)
            .map(|i| {
                let t = end - (69 - i) * step;
                f((t as f64 / 3600.0).sin() * 0.5 + 0.5, t)
            })
            .collect()
    }
}

#[async_trait]
impl ProxmoxApi for DemoProxmox {
    async fn cluster_status(&self) -> anyhow::Result<Vec<ClusterStatusEntry>> {
        let mut out = vec![ClusterStatusEntry {
            kind: "cluster".into(),
            name: "homelab".into(),
            quorate: Some(true),
            nodes: Some(NODES.len() as u64),
            online: None,
            ip: None,
        }];
        out.extend(
            NODES
                .iter()
                .enumerate()
                .map(|(i, (name, ..))| ClusterStatusEntry {
                    kind: "node".into(),
                    name: (*name).into(),
                    quorate: None,
                    nodes: None,
                    online: Some(true),
                    ip: Some(format!("10.0.20.{}", 11 + i)),
                }),
        );
        Ok(out)
    }

    async fn resources(&self) -> anyhow::Result<Vec<ClusterResource>> {
        let state = self.state.lock().expect("lock");
        let mut out = Vec::new();
        for &(name, cores, mem_gb, load) in NODES {
            out.push(ClusterResource {
                id: format!("node/{name}"),
                kind: "node".into(),
                node: Some(name.into()),
                status: Some("online".into()),
                cpu: Some(Self::jitter(load, 0.04)),
                maxcpu: Some(cores as f64),
                mem: Some((mem_gb as f64 * GB as f64 * Self::jitter(0.58, 0.02)) as u64),
                maxmem: Some(mem_gb * GB),
                disk: Some(31 * GB),
                maxdisk: Some(94 * GB),
                uptime: Some(12 * 86_400 + 3 * 3600),
                ..Default::default()
            });
            for (storage, plugin, used, total, shared) in [
                ("local", "dir", 31, 94, false),
                ("local-zfs", "zfspool", 610, 1800, false),
            ] {
                out.push(ClusterResource {
                    id: format!("storage/{name}/{storage}"),
                    kind: "storage".into(),
                    node: Some(name.into()),
                    status: Some("available".into()),
                    storage: Some(storage.into()),
                    plugintype: Some(plugin.into()),
                    disk: Some(used * GB),
                    maxdisk: Some(total * GB),
                    shared: Some(shared),
                    content: Some(
                        if plugin == "dir" {
                            "iso,vztmpl,backup"
                        } else {
                            "images,rootdir"
                        }
                        .into(),
                    ),
                    ..Default::default()
                });
            }
            out.push(ClusterResource {
                id: format!("storage/{name}/nas-backup"),
                kind: "storage".into(),
                node: Some(name.into()),
                status: Some("available".into()),
                storage: Some("nas-backup".into()),
                plugintype: Some("nfs".into()),
                disk: Some(6_560 * GB),
                maxdisk: Some(8_000 * GB),
                shared: Some(true),
                content: Some("backup".into()),
                ..Default::default()
            });
        }
        for g in &state.guests {
            let mem = g.mem_gb * GB;
            out.push(ClusterResource {
                id: format!("{}/{}", g.ty.as_str(), g.vmid),
                kind: g.ty.as_str().into(),
                node: Some(g.node.into()),
                status: Some(if g.running { "running" } else { "stopped" }.into()),
                name: Some(g.name.into()),
                vmid: Some(g.vmid as u64),
                cpu: Some(if g.running {
                    Self::jitter(0.06, 0.05)
                } else {
                    0.0
                }),
                maxcpu: Some(g.cores as f64),
                mem: Some(if g.running {
                    (mem as f64 * Self::jitter(0.42, 0.05)) as u64
                } else {
                    0
                }),
                maxmem: Some(mem),
                disk: Some(if g.ty == GuestType::Lxc {
                    2 * GB + (g.vmid as u64 % 40) * 60_000_000
                } else {
                    0
                }),
                maxdisk: Some(if g.ty == GuestType::Lxc {
                    8 * GB
                } else {
                    64 * GB
                }),
                uptime: Some(if g.running {
                    (now() - g.started_at).max(0) as u64
                } else {
                    0
                }),
                netin: Some(g.vmid as u64 * 7_340_032),
                netout: Some(g.vmid as u64 * 1_048_576),
                tags: Some(g.tags.into()),
                template: Some(false),
                ..Default::default()
            });
        }
        Ok(out)
    }

    async fn node_status(&self, node: &str) -> anyhow::Result<NodeStatus> {
        let &(_, cores, mem_gb, load) = NODES
            .iter()
            .find(|n| n.0 == node)
            .ok_or_else(|| anyhow::anyhow!("no such node"))?;
        Ok(NodeStatus {
            cpu: Some(Self::jitter(load, 0.04)),
            cpuinfo: CpuInfo {
                cpus: Some(cores),
                model: Some("AMD Ryzen 7 5700G with Radeon Graphics".into()),
            },
            memory: MemInfo {
                total: Some(mem_gb * GB),
                used: Some((mem_gb as f64 * GB as f64 * Self::jitter(0.58, 0.02)) as u64),
            },
            rootfs: MemInfo {
                total: Some(94 * GB),
                used: Some(31 * GB),
            },
            uptime: Some(12 * 86_400 + 3 * 3600),
            loadavg: vec![json!("1.12"), json!("0.98"), json!("0.87")],
            pveversion: Some("pve-manager/9.0.10/demo".into()),
            kversion: Some("Linux 6.14.11-2-pve".into()),
        })
    }

    async fn node_rrd(&self, node: &str, range: RrdRange) -> anyhow::Result<Vec<Value>> {
        let &(_, cores, mem_gb, load) = NODES
            .iter()
            .find(|n| n.0 == node)
            .ok_or_else(|| anyhow::anyhow!("no such node"))?;
        Ok(Self::rrd(range, |w, t| {
            json!({
                "time": t, "cpu": load * (0.6 + 0.8 * w), "maxcpu": cores,
                "memused": (mem_gb * GB) as f64 * (0.55 + 0.05 * w), "memtotal": mem_gb * GB,
                "netin": 42_600_000.0 * w + 1_000_000.0, "netout": 6_100_000.0 * w + 200_000.0,
                "rootused": 31 * GB, "roottotal": 94 * GB, "loadavg": 1.0 + w, "iowait": 0.01 * w,
            })
        }))
    }

    async fn guest_rrd(
        &self,
        _node: &str,
        _ty: GuestType,
        vmid: u32,
        range: RrdRange,
    ) -> anyhow::Result<Vec<Value>> {
        let g = self.find(vmid)?;
        let mem = (g.mem_gb * GB) as f64;
        Ok(Self::rrd(range, |w, t| {
            json!({
                "time": t, "cpu": if g.running { 0.02 + 0.1 * w } else { 0.0 }, "maxcpu": g.cores,
                "mem": if g.running { mem * (0.35 + 0.1 * w) } else { 0.0 }, "maxmem": mem,
                "netin": 2_400_000.0 * w, "netout": 380_000.0 * w,
                "diskread": 120_000.0 * w, "diskwrite": 64_000.0 * w,
            })
        }))
    }

    async fn guest_status(
        &self,
        _node: &str,
        _ty: GuestType,
        vmid: u32,
    ) -> anyhow::Result<GuestStatusCurrent> {
        let g = self.find(vmid)?;
        Ok(GuestStatusCurrent {
            status: Some(if g.running { "running" } else { "stopped" }.into()),
            qmpstatus: (g.ty == GuestType::Qemu)
                .then(|| if g.running { "running" } else { "stopped" }.into()),
            cpus: Some(g.cores as f64),
            maxmem: Some(g.mem_gb * GB),
            maxdisk: Some(8 * GB),
            uptime: Some(if g.running {
                (now() - g.started_at) as u64
            } else {
                0
            }),
            lock: None,
            ha: json!({"managed": 0}),
        })
    }

    async fn power(
        &self,
        node: &str,
        ty: GuestType,
        vmid: u32,
        action: PowerAction,
    ) -> anyhow::Result<String> {
        let upid = {
            let mut state = self.state.lock().expect("lock");
            let guest = state
                .guests
                .iter()
                .find(|g| g.vmid == vmid && g.node == node)
                .ok_or_else(|| anyhow::anyhow!("guest {vmid} not found on {node}"))?;
            match (action, guest.running) {
                (PowerAction::Start, true) => anyhow::bail!("guest {vmid} is already running"),
                (PowerAction::Start, false) => {}
                (_, false) => anyhow::bail!("guest {vmid} is not running"),
                _ => {}
            }
            state.counter += 1;
            let prefix = if ty == GuestType::Qemu { "qm" } else { "vz" };
            let kind = format!("{prefix}{}", action.as_str());
            let started = now();
            let upid = format!(
                "UPID:{node}:{:08X}:{:08X}:{started:08X}:{kind}:{vmid}:quorum@pve!quorum:",
                40_000 + state.counter,
                state.counter
            );
            state.tasks.insert(
                upid.clone(),
                DemoTask {
                    node: node.into(),
                    kind,
                    vmid,
                    started,
                    ended: None,
                },
            );
            upid
        };
        let this = self.clone();
        let task_id = upid.clone();
        tokio::spawn(async move {
            tokio::time::sleep(this.delay).await;
            let mut state = this.state.lock().expect("lock");
            if let Some(g) = state.guests.iter_mut().find(|g| g.vmid == vmid) {
                g.running = !matches!(action, PowerAction::Shutdown | PowerAction::Stop);
                if g.running {
                    g.started_at = now();
                }
            }
            if let Some(t) = state.tasks.get_mut(&task_id) {
                t.ended = Some(now());
            }
        });
        Ok(upid)
    }

    async fn task_status(&self, node: &str, upid: &str) -> anyhow::Result<TaskStatus> {
        let state = self.state.lock().expect("lock");
        let t = state
            .tasks
            .get(upid)
            .filter(|t| t.node == node)
            .ok_or_else(|| anyhow::anyhow!("no such task"))?;
        Ok(TaskStatus {
            upid: Some(upid.into()),
            node: Some(t.node.clone()),
            status: if t.ended.is_some() {
                "stopped"
            } else {
                "running"
            }
            .into(),
            exitstatus: t.ended.map(|_| "OK".into()),
            kind: Some(t.kind.clone()),
            id: Some(t.vmid.to_string()),
            user: Some("quorum@pve!quorum".into()),
            starttime: Some(t.started as u64),
        })
    }

    async fn tasks(
        &self,
        node: &str,
        vmid: Option<u32>,
        limit: u32,
    ) -> anyhow::Result<Vec<TaskListEntry>> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<TaskListEntry> = state
            .tasks
            .iter()
            .filter(|(_, t)| t.node == node && vmid.is_none_or(|v| v == t.vmid))
            .map(|(upid, t)| TaskListEntry {
                upid: upid.clone(),
                node: Some(t.node.clone()),
                kind: Some(t.kind.clone()),
                id: Some(t.vmid.to_string()),
                user: Some("quorum@pve!quorum".into()),
                starttime: Some(t.started as u64),
                endtime: t.ended.map(|e| e as u64),
                status: t.ended.map(|_| "OK".into()),
            })
            .collect();
        // A nightly backup per guest, so guest detail has some history.
        if let Some(vmid) = vmid {
            let start = (now() / 86_400) * 86_400 + 2 * 3600 - 86_400;
            out.push(TaskListEntry {
                upid: format!("UPID:{node}:0000BEEF:00000001:{start:08X}:vzdump:{vmid}:root@pam:"),
                node: Some(node.into()),
                kind: Some("vzdump".into()),
                id: Some(vmid.to_string()),
                user: Some("root@pam".into()),
                starttime: Some(start as u64),
                endtime: Some(start as u64 + 95),
                status: Some("OK".into()),
            });
        }
        out.sort_by_key(|t| std::cmp::Reverse(t.starttime));
        out.truncate(limit as usize);
        Ok(out)
    }

    async fn vzdump_tasks(&self, node: &str, since: i64) -> anyhow::Result<Vec<TaskListEntry>> {
        // The nightly backup job (all guests on the node, so no task id) for the last 3 nights,
        // plus a manual backup of one guest. draw.io's backup fails on pve-03 every night.
        let mut out = Vec::new();
        for night in 0..3 {
            let start = Self::nightly(night);
            out.push(TaskListEntry {
                upid: format!("UPID:{node}:0000D00{night}:00000001:{start:08X}:vzdump::root@pam:"),
                node: Some(node.into()),
                kind: Some("vzdump".into()),
                id: None,
                user: Some("root@pam".into()),
                starttime: Some(start as u64),
                endtime: Some(start as u64 + 1260),
                status: Some(if node == "pve-03" {
                    "job errors".into()
                } else {
                    "OK".into()
                }),
            });
        }
        if node == "pve-02" {
            let start = now() - 5 * 3600;
            out.push(TaskListEntry {
                upid: format!("UPID:{node}:0000D0FF:00000001:{start:08X}:vzdump:113:root@pam:"),
                node: Some(node.into()),
                kind: Some("vzdump".into()),
                id: Some("113".into()),
                user: Some("root@pam".into()),
                starttime: Some(start as u64),
                endtime: Some(start as u64 + 240),
                status: Some("OK".into()),
            });
        }
        out.retain(|t| t.starttime.is_some_and(|s| s as i64 >= since));
        out.sort_by_key(|t| std::cmp::Reverse(t.starttime));
        Ok(out)
    }

    async fn task_log(&self, node: &str, upid: &str) -> anyhow::Result<Vec<String>> {
        if !upid.contains(":vzdump::") {
            return Ok(vec!["TASK OK".into()]);
        }
        let mut lines =
            vec!["INFO: starting new backup job: vzdump --all 1 --mode snapshot".into()];
        for (vmid, _, _, ty, ..) in GUESTS.iter().filter(|g| g.2 == node) {
            // mkdocs is excluded from the job (shows "no backup").
            if *vmid == 128 {
                continue;
            }
            let kind = if *ty == GuestType::Lxc { "lxc" } else { "qemu" };
            lines.push(format!("INFO: Starting Backup of VM {vmid} ({kind})"));
            if *vmid == 116 {
                lines.push(format!(
                    "ERROR: Backup of VM {vmid} failed - unable to create temporary directory '/var/tmp/vzdumptmp1234_{vmid}' - No space left on device"
                ));
            } else {
                lines.push(format!(
                    "INFO: Finished Backup of VM {vmid} (00:01:{:02})",
                    vmid % 60
                ));
            }
        }
        lines.push(if node == "pve-03" {
            "INFO: Backup job finished with errors".into()
        } else {
            "INFO: Backup job finished successfully".into()
        });
        lines.push("TASK OK".into());
        Ok(lines)
    }

    async fn permissions(&self) -> anyhow::Result<Vec<String>> {
        Ok(super::REQUIRED_PRIVILEGES
            .iter()
            .map(|s| s.to_string())
            .collect())
    }
}

impl DemoProxmox {
    /// Start of the nightly backup `nights_ago` nights back (02:00 UTC).
    fn nightly(nights_ago: i64) -> i64 {
        let today = (now() / 86_400) * 86_400 + 2 * 3600;
        let last = if today > now() { today - 86_400 } else { today };
        last - nights_ago * 86_400
    }

    fn find(&self, vmid: u32) -> anyhow::Result<DemoGuest> {
        self.state
            .lock()
            .expect("lock")
            .guests
            .iter()
            .find(|g| g.vmid == vmid)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("guest {vmid} not found"))
    }
}
