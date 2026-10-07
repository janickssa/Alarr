//! WireGuard (read-only, via the Quorum agent in `agent/`): peers, last handshake,
//! transfer and online state (handshake within 3 minutes).
//!
//! The agent serves `GET /peers` with `Authorization: Bearer <token>` and never sends
//! private or preshared keys. When nothing answers (connection refused, or a 404 from
//! something else on the port) the module reports the "agent not installed" state rather
//! than an error.

use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::http::{ServiceHttp, fmt_bytes, validate_http};
use super::{
    ConnectionTest, Health, InstanceConfig, ModuleData, PollOutcome, ServiceKind, ServiceModule,
    describe_error,
};

/// A peer counts as online when its last handshake is this recent. WireGuard re-handshakes
/// every 2 minutes while traffic flows.
pub const ONLINE_WITHIN_SECS: i64 = 180;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WgInterface {
    pub name: String,
    pub public_key: String,
    pub listen_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WgPeer {
    pub interface: String,
    pub public_key: String,
    /// From the agent's names file, when set.
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub last_handshake: Option<DateTime<Utc>>,
    pub online: bool,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WireguardData {
    /// False when no agent answers: the page shows install steps.
    pub agent_installed: bool,
    pub agent_version: Option<String>,
    pub interfaces: Vec<WgInterface>,
    pub peers: Vec<WgPeer>,
}

impl WireguardData {
    pub fn online(&self) -> usize {
        self.peers.iter().filter(|p| p.online).count()
    }

    pub fn headline(&self) -> (String, Option<String>) {
        if !self.agent_installed {
            return ("Agent not installed".into(), None);
        }
        let (rx, tx) = self
            .peers
            .iter()
            .fold((0, 0), |(r, t), p| (r + p.rx_bytes, t + p.tx_bytes));
        (
            format!("{} of {} peers online", self.online(), self.peers.len()),
            Some(format!("↓ {} · ↑ {}", fmt_bytes(rx), fmt_bytes(tx))),
        )
    }
}

#[derive(Deserialize)]
struct AgentPeers {
    version: Option<String>,
    #[serde(default)]
    interfaces: Vec<AgentInterface>,
    #[serde(default)]
    peers: Vec<AgentPeer>,
}

#[derive(Deserialize)]
struct AgentInterface {
    name: String,
    public_key: String,
    listen_port: Option<u16>,
}

#[derive(Deserialize)]
struct AgentPeer {
    interface: String,
    public_key: String,
    name: Option<String>,
    endpoint: Option<String>,
    #[serde(default)]
    allowed_ips: Vec<String>,
    latest_handshake: Option<i64>,
    #[serde(default)]
    rx_bytes: u64,
    #[serde(default)]
    tx_bytes: u64,
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["api_key"])
}

pub struct WireguardModule {
    http: Option<ServiceHttp>,
}

fn convert(a: AgentPeers, now: DateTime<Utc>) -> WireguardData {
    let mut peers: Vec<WgPeer> = a
        .peers
        .into_iter()
        .map(|p| {
            let last = p
                .latest_handshake
                .and_then(|t| DateTime::from_timestamp(t, 0));
            WgPeer {
                interface: p.interface,
                public_key: p.public_key,
                name: p.name,
                endpoint: p.endpoint,
                allowed_ips: p.allowed_ips,
                online: last.is_some_and(|l| now - l < Duration::seconds(ONLINE_WITHIN_SECS)),
                last_handshake: last,
                rx_bytes: p.rx_bytes,
                tx_bytes: p.tx_bytes,
            }
        })
        .collect();
    // Online first, then most recent handshake.
    peers.sort_by(|a, b| {
        b.online
            .cmp(&a.online)
            .then(b.last_handshake.cmp(&a.last_handshake))
    });
    WireguardData {
        agent_installed: true,
        agent_version: a.version,
        interfaces: a
            .interfaces
            .into_iter()
            .map(|i| WgInterface {
                name: i.name,
                public_key: i.public_key,
                listen_port: i.listen_port,
            })
            .collect(),
        peers,
    }
}

fn missing() -> WireguardData {
    WireguardData {
        agent_installed: false,
        agent_version: None,
        interfaces: vec![],
        peers: vec![],
    }
}

impl WireguardModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        let key = cfg.secret("api_key").unwrap_or_default();
        Ok(Self {
            http: Some(ServiceHttp::new(
                cfg,
                &[("authorization", format!("Bearer {key}"))],
            )?),
        })
    }

    pub fn demo() -> Self {
        Self { http: None }
    }

    async fn fetch(&self) -> anyhow::Result<WireguardData> {
        let Some(http) = &self.http else {
            return Ok(demo_data());
        };
        let response = match http.request(Method::GET, "/peers").send().await {
            Ok(r) => r,
            Err(e) if e.is_connect() => return Ok(missing()),
            Err(e) => return Err(e.into()),
        };
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(missing());
        }
        let body: AgentPeers = ServiceHttp::send_checked(response)?.json().await?;
        Ok(convert(body, Utc::now()))
    }
}

fn demo_data() -> WireguardData {
    let now = Utc::now();
    let peer = |name: &str,
                key: &str,
                ip: &str,
                endpoint: Option<&str>,
                secs: Option<i64>,
                rx: u64,
                tx: u64| {
        AgentPeer {
            interface: "wg0".into(),
            public_key: key.into(),
            name: Some(name.into()),
            endpoint: endpoint.map(str::to_owned),
            allowed_ips: vec![ip.into()],
            latest_handshake: secs.map(|s| (now - Duration::seconds(s)).timestamp()),
            rx_bytes: rx,
            tx_bytes: tx,
        }
    };
    const MB: u64 = 1024 * 1024;
    convert(
        AgentPeers {
            version: Some("0.1.0".into()),
            interfaces: vec![AgentInterface {
                name: "wg0".into(),
                public_key: "hJ0p3mWZ0rQm1b5wq5yX2bWm0yq7L0sJvHq6nY3c1Hk=".into(),
                listen_port: Some(51820),
            }],
            peers: vec![
                peer(
                    "Janick's phone",
                    "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=",
                    "10.8.0.2/32",
                    Some("198.51.100.23:40112"),
                    Some(47),
                    412 * MB,
                    2_380 * MB,
                ),
                peer(
                    "Laptop",
                    "TrMvSoP4jYQlY6RIzBgbssQqY3vxI2Pi+y71lOWWXX0=",
                    "10.8.0.3/32",
                    Some("203.0.113.88:51820"),
                    Some(95),
                    1_840 * MB,
                    6_120 * MB,
                ),
                peer(
                    "Mara's tablet",
                    "gN65BkIKy1eCE9pP1wdc8ROUtkHLF2PfAqYdyYBz6EA=",
                    "10.8.0.4/32",
                    Some("192.0.2.14:60341"),
                    Some(3 * 3600),
                    96 * MB,
                    880 * MB,
                ),
                peer(
                    "Backup site",
                    "Hq7nR5f3VZb1wN8tY2kL6mC4xJ9pD0sA3gE5hU7iO1Q=",
                    "10.8.0.10/32",
                    None,
                    None,
                    0,
                    0,
                ),
            ],
        },
        now,
    )
}

#[async_trait]
impl ServiceModule for WireguardModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Wireguard
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let data = self.fetch().await?;
        let (health, reason) = if data.agent_installed {
            (Health::Healthy, None)
        } else {
            (Health::Unknown, Some("Quorum agent not installed".into()))
        };
        Ok(PollOutcome {
            health,
            health_reason: reason,
            response_ms: Some(started.elapsed().as_millis() as u64),
            version: data.agent_version.as_ref().map(|v| format!("agent {v}")),
            data: ModuleData::Wireguard(Box::new(data)),
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok(d) if !d.agent_installed => ConnectionTest {
                ok: false,
                message: "No Quorum agent answers at this address. Install it in the WireGuard container (agent/README.md).".into(),
                ..Default::default()
            },
            Ok(d) => ConnectionTest {
                ok: true,
                message: format!("{} peers on {} interfaces", d.peers.len(), d.interfaces.len()),
                version: d.agent_version,
                ..Default::default()
            },
            Err(e) => ConnectionTest {
                ok: false,
                message: describe_error(&e),
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(url: &str) -> InstanceConfig {
        InstanceConfig {
            id: "wireguard".into(),
            kind: ServiceKind::Wireguard,
            name: "WireGuard".into(),
            base_url: Some(url.into()),
            settings: json!({}),
            secrets: [("api_key".to_owned(), "t".repeat(32))].into(),
        }
    }

    #[tokio::test]
    async fn reads_peers_from_the_agent() {
        let s = MockServer::start().await;
        let now = Utc::now().timestamp();
        Mock::given(method("GET"))
            .and(path("/peers"))
            .and(header("authorization", format!("Bearer {}", "t".repeat(32)).as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "version": "0.1.0",
                "interfaces": [{"name": "wg0", "public_key": "PUB=", "listen_port": 51820}],
                "peers": [
                    {"interface": "wg0", "public_key": "OLD=", "name": null, "endpoint": null, "allowed_ips": ["10.8.0.3/32"],
                     "latest_handshake": now - 600, "rx_bytes": 1, "tx_bytes": 2, "persistent_keepalive": null},
                    {"interface": "wg0", "public_key": "NEW=", "name": "phone", "endpoint": "203.0.113.7:51820",
                     "allowed_ips": ["10.8.0.2/32"], "latest_handshake": now - 30, "rx_bytes": 1024, "tx_bytes": 2048,
                     "persistent_keepalive": 25}
                ]
            })))
            .mount(&s)
            .await;
        let out = WireguardModule::new(&cfg(&s.uri()))
            .unwrap()
            .poll()
            .await
            .unwrap();
        let ModuleData::Wireguard(d) = out.data else {
            panic!()
        };
        assert!(d.agent_installed);
        assert_eq!(out.version.as_deref(), Some("agent 0.1.0"));
        assert_eq!(d.peers[0].name.as_deref(), Some("phone"));
        assert!(d.peers[0].online && !d.peers[1].online);
        assert_eq!(d.headline().0, "1 of 2 peers online");
    }

    #[tokio::test]
    async fn missing_agent_is_a_state_not_an_error() {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&s)
            .await;
        let out = WireguardModule::new(&cfg(&s.uri()))
            .unwrap()
            .poll()
            .await
            .unwrap();
        assert_eq!(out.health, Health::Unknown);
        let ModuleData::Wireguard(d) = out.data else {
            panic!()
        };
        assert!(!d.agent_installed);

        // Nothing listening at all.
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let m = WireguardModule::new(&cfg(&format!("http://{closed}"))).unwrap();
        assert!(!m.test_connection().await.ok);
        assert_eq!(m.poll().await.unwrap().health, Health::Unknown);
    }

    #[tokio::test]
    async fn wrong_token() {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&s)
            .await;
        let err = WireguardModule::new(&cfg(&s.uri()))
            .unwrap()
            .poll()
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("authentication failed"), "{err}");
    }
}
