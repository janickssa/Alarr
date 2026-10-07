use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::api::HttpProxmox;
use super::*;

const AUTH: &str = "PVEAPIToken=quorum@pve!quorum=secret-uuid";

async fn mock(server: &MockServer, p: &str, data: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(p))
        .and(header("Authorization", AUTH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": data })))
        .mount(server)
        .await;
}

async fn cluster(server: &MockServer) {
    mock(
        server,
        "/api2/json/cluster/status",
        json!([
            {"type": "cluster", "id": "cluster", "name": "homelab", "quorate": 1, "nodes": 2, "version": 4},
            {"type": "node", "id": "node/pve-01", "name": "pve-01", "online": 1, "ip": "10.0.20.11", "local": 1, "nodeid": 1},
            {"type": "node", "id": "node/pve-02", "name": "pve-02", "online": 0, "ip": "10.0.20.12", "local": 0, "nodeid": 2}
        ]),
    )
    .await;
    mock(
        server,
        "/api2/json/cluster/resources",
        json!([
            {"id": "node/pve-01", "type": "node", "node": "pve-01", "status": "online", "cpu": 0.12, "maxcpu": 16, "mem": 1000, "maxmem": 4000, "uptime": 3600},
            {"id": "node/pve-02", "type": "node", "node": "pve-02", "status": "offline"},
            {"id": "lxc/123", "type": "lxc", "vmid": 123, "name": "sonarr", "node": "pve-01", "status": "running",
             "cpu": 0.02, "maxcpu": 2, "mem": 500, "maxmem": 2000, "disk": 300, "maxdisk": 800, "uptime": 99,
             "netin": 1, "netout": 2, "tags": "media;svc-sonarr", "template": 0},
            {"id": "qemu/200", "type": "qemu", "vmid": 200, "name": "haos", "node": "pve-01", "status": "stopped", "template": 0},
            {"id": "storage/pve-01/local-zfs", "type": "storage", "node": "pve-01", "storage": "local-zfs",
             "status": "available", "disk": 95, "maxdisk": 100, "plugintype": "zfspool", "shared": 0, "content": "images,rootdir"}
        ]),
    )
    .await;
    mock(
        server,
        "/api2/json/nodes/pve-01/status",
        json!({
            "cpu": 0.15, "cpuinfo": {"cpus": 16, "model": "Ryzen"}, "memory": {"total": 4000, "used": 1200},
            "rootfs": {"total": 100, "used": 30}, "uptime": 3600, "loadavg": ["0.50", "0.40", "0.30"],
            "pveversion": "pve-manager/9.0.10/abc", "kversion": "Linux 6.14"
        }),
    )
    .await;
}

fn module(server: &MockServer) -> ProxmoxModule {
    let api = HttpProxmox::new(&server.uri(), "quorum@pve!quorum", "secret-uuid", None).unwrap();
    ProxmoxModule::with_api(Arc::new(api))
}

#[tokio::test]
async fn poll_builds_cluster_snapshot() {
    let server = MockServer::start().await;
    cluster(&server).await;
    let out = module(&server).poll().await.unwrap();
    let ModuleData::Proxmox(data) = out.data else {
        panic!("wrong data")
    };
    assert_eq!(data.cluster.name.as_deref(), Some("homelab"));
    assert!(data.cluster.quorate);
    assert_eq!(
        (data.cluster.nodes_online, data.cluster.nodes_total),
        (1, 2)
    );
    assert_eq!(out.version.as_deref(), Some("9.0.10"));

    let n1 = &data.nodes[0];
    assert_eq!(n1.ip.as_deref(), Some("10.0.20.11"));
    assert_eq!(n1.cpu_usage, Some(0.15));
    assert_eq!(n1.mem_used, Some(1200));
    assert_eq!(n1.load_avg.as_deref(), Some(&[0.5, 0.4, 0.3][..]));

    assert_eq!(data.guests.len(), 2);
    let sonarr = &data.guests[0];
    assert_eq!(
        (sonarr.vmid, sonarr.guest_type, sonarr.status),
        (123, GuestType::Lxc, GuestStatus::Running)
    );
    assert_eq!(sonarr.tags, vec!["media", "svc-sonarr"]);
    assert_eq!(data.guests[1].status, GuestStatus::Stopped);

    // Node offline and storage at 95 % are both warnings.
    assert_eq!(out.health, Health::Warning);
    let reason = out.health_reason.unwrap();
    assert!(reason.contains("pve-02 offline"), "{reason}");
    assert!(reason.contains("pve-01/local-zfs at 95 %"), "{reason}");
}

#[tokio::test]
async fn poll_fails_on_bad_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let err = module(&server).poll().await.unwrap_err();
    assert!(err.to_string().contains("authentication failed"));
}

#[tokio::test]
async fn power_posts_to_status_endpoint() {
    let server = MockServer::start().await;
    let upid = "UPID:pve-01:0001:0002:66F0:vzshutdown:123:quorum@pve!quorum:";
    Mock::given(method("POST"))
        .and(path("/api2/json/nodes/pve-01/lxc/123/status/shutdown"))
        .and(header("Authorization", AUTH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": upid })))
        .expect(1)
        .mount(&server)
        .await;
    let m = module(&server);
    let got = m
        .api()
        .power("pve-01", GuestType::Lxc, 123, PowerAction::Shutdown)
        .await
        .unwrap();
    assert_eq!(got, upid);
    assert_eq!(upid_node(&got), Some("pve-01"));
}

#[tokio::test]
async fn task_status_and_rrd() {
    let server = MockServer::start().await;
    let upid = "UPID:pve-01:0001:0002:66F0:vzstart:123:quorum@pve!quorum:";
    mock(
        &server,
        &format!("/api2/json/nodes/pve-01/tasks/{upid}/status"),
        json!({"upid": upid, "node": "pve-01", "status": "stopped", "exitstatus": "OK", "type": "vzstart", "id": "123", "starttime": 1_700_000_000}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api2/json/nodes/pve-01/rrddata"))
        .and(query_param("timeframe", "day"))
        .and(query_param("cf", "AVERAGE"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            {"time": 1_700_000_000, "cpu": 0.1, "memused": 5.0, "memtotal": 10.0, "netin": 3.0},
            {"time": 1_700_000_060}
        ]})))
        .mount(&server)
        .await;
    let m = module(&server);
    let task = Task::from_status(
        upid,
        "pve-01",
        m.api().task_status("pve-01", upid).await.unwrap(),
    );
    assert_eq!(task.state, TaskState::Ok);
    let rrd = m.api().node_rrd("pve-01", RrdRange::Day).await.unwrap();
    let points: Vec<_> = rrd.iter().filter_map(RrdPoint::from_raw).collect();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].mem_used, Some(5.0));
    assert_eq!(points[1].cpu, None);
}

#[test]
fn rejects_unsafe_path_segments() {
    assert!(api::safe_segment("pve-01").is_ok());
    assert!(api::safe_segment("../etc").is_err());
    assert!(api::safe_segment("a/b").is_err());
    assert!(api::safe_segment("").is_err());
}

#[test]
fn upid_parsing() {
    assert_eq!(
        upid_node("UPID:pve-02:00:00:00:qmstart:200:root@pam:"),
        Some("pve-02")
    );
    assert_eq!(upid_node("nope:pve-02"), None);
}

#[test]
fn validation_requires_https_token_and_secret() {
    let mut cfg = InstanceConfig {
        id: "pve".into(),
        kind: ServiceKind::Proxmox,
        name: "Proxmox".into(),
        base_url: Some("https://10.0.20.11:8006".into()),
        settings: json!({"token_id": "quorum@pve!quorum"}),
        secrets: [("token_secret".to_owned(), "x".to_owned())].into(),
    };
    assert!(validate(&cfg).is_ok());
    cfg.base_url = Some("http://10.0.20.11:8006".into());
    assert!(validate(&cfg).is_err());
    cfg.base_url = Some("https://10.0.20.11:8006".into());
    cfg.secrets.clear();
    assert!(validate(&cfg).is_err());
}

#[tokio::test]
async fn demo_power_cycle() {
    let api = demo::DemoProxmox::with_delay(std::time::Duration::from_millis(10));
    let m = ProxmoxModule::with_api(Arc::new(api));
    let upid = m
        .api()
        .power("pve-02", GuestType::Lxc, 123, PowerAction::Shutdown)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let task = m.api().task_status("pve-02", &upid).await.unwrap();
    assert_eq!(task.exitstatus.as_deref(), Some("OK"));
    let out = m.poll().await.unwrap();
    let ModuleData::Proxmox(data) = out.data else {
        panic!()
    };
    let sonarr = data.guests.iter().find(|g| g.vmid == 123).unwrap();
    assert_eq!(sonarr.status, GuestStatus::Stopped);
    assert!(
        m.api()
            .power("pve-02", GuestType::Lxc, 123, PowerAction::Stop)
            .await
            .is_err()
    );
}
