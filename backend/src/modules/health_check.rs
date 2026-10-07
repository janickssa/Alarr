//! Generic health check: HTTP(S) GET with an expected status, or a TCP connect.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    ConnectionTest, Health, InstanceConfig, ModuleData, PollOutcome, ServiceKind, ServiceModule,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    #[default]
    Http,
    Tcp,
}

/// Settings of a health-check instance (the `settings` object of the service).
#[derive(Debug, Clone, Default, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct HealthCheckSettings {
    pub check: CheckType,
    /// HTTP: path appended to the base URL (default `/`).
    pub path: Option<String>,
    /// HTTP: expected status code. Default: any 2xx or 3xx.
    pub expected_status: Option<u16>,
    /// TCP: `host:port` to connect to.
    pub address: Option<String>,
    /// Timeout in milliseconds (default 5000).
    pub timeout_ms: Option<u64>,
    /// Responses slower than this are reported as a warning (default 2000 ms).
    pub slow_ms: Option<u64>,
    /// SHA-256 fingerprint to pin for a self-signed HTTPS certificate.
    pub tls_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HealthCheckData {
    pub check: CheckType,
    /// URL or `host:port` that was checked.
    pub target: String,
    /// HTTP status code of the last response.
    pub status_code: Option<u16>,
}

fn settings(cfg: &InstanceConfig) -> Result<HealthCheckSettings, String> {
    serde_json::from_value(cfg.settings.clone()).map_err(|e| format!("invalid settings: {e}"))
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    let s = settings(cfg)?;
    match s.check {
        CheckType::Http => {
            let url = cfg.base_url.as_deref().ok_or("base URL is required")?;
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err("base URL must start with http:// or https://".into());
            }
            if let Some(fp) = &s.tls_fingerprint {
                crate::tls::parse_fingerprint(fp).map_err(|e| e.to_string())?;
            }
        }
        CheckType::Tcp => {
            let addr = s
                .address
                .as_deref()
                .ok_or("address (host:port) is required")?;
            let port = addr.rsplit_once(':').map(|(_, p)| p);
            if port.and_then(|p| p.parse::<u16>().ok()).is_none() {
                return Err("address must be host:port".into());
            }
        }
    }
    Ok(())
}

pub struct HealthCheckModule {
    settings: HealthCheckSettings,
    target: String,
    client: Option<reqwest::Client>,
    timeout: Duration,
}

impl HealthCheckModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        let settings = settings(cfg).map_err(anyhow::Error::msg)?;
        let timeout = Duration::from_millis(settings.timeout_ms.unwrap_or(5000));
        let (target, client) = match settings.check {
            CheckType::Http => {
                let base = cfg
                    .base_url
                    .as_deref()
                    .unwrap_or_default()
                    .trim_end_matches('/');
                let path = settings.path.as_deref().unwrap_or("/");
                let path = if path.starts_with('/') {
                    path.to_owned()
                } else {
                    format!("/{path}")
                };
                let client = crate::tls::http_client(settings.tls_fingerprint.as_deref(), timeout)?;
                (format!("{base}{path}"), Some(client))
            }
            CheckType::Tcp => (settings.address.clone().unwrap_or_default(), None),
        };
        Ok(Self {
            settings,
            target,
            client,
            timeout,
        })
    }

    async fn check(&self) -> anyhow::Result<(u64, Option<u16>)> {
        let started = Instant::now();
        let status = match &self.client {
            Some(client) => {
                let response = client.get(&self.target).send().await?;
                Some(response.status().as_u16())
            }
            None => {
                tokio::time::timeout(self.timeout, tokio::net::TcpStream::connect(&self.target))
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!("timeout after {} ms", self.timeout.as_millis())
                    })??;
                None
            }
        };
        Ok((started.elapsed().as_millis() as u64, status))
    }

    fn status_ok(&self, status: u16) -> bool {
        match self.settings.expected_status {
            Some(expected) => status == expected,
            None => (200..400).contains(&status),
        }
    }
}

#[async_trait]
impl ServiceModule for HealthCheckModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::HealthCheck
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let data = |status_code| {
            ModuleData::HealthCheck(HealthCheckData {
                check: self.settings.check,
                target: self.target.clone(),
                status_code,
            })
        };
        let (ms, status) = match self.check().await {
            Ok(r) => r,
            Err(e) => {
                return Ok(PollOutcome {
                    health: Health::Down,
                    health_reason: Some(super::describe_error(&e)),
                    response_ms: None,
                    version: None,
                    data: data(None),
                });
            }
        };
        let (health, reason) = match status {
            Some(code) if !self.status_ok(code) => (Health::Down, Some(format!("HTTP {code}"))),
            _ if ms > self.settings.slow_ms.unwrap_or(2000) => {
                (Health::Warning, Some(format!("slow response ({ms} ms)")))
            }
            _ => (Health::Healthy, None),
        };
        Ok(PollOutcome {
            health,
            health_reason: reason,
            response_ms: Some(ms),
            version: None,
            data: data(status),
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.check().await {
            Ok((ms, status)) => {
                let ok = status.is_none_or(|s| self.status_ok(s));
                ConnectionTest {
                    ok,
                    message: match status {
                        Some(s) if ok => format!("HTTP {s} in {ms} ms"),
                        Some(s) => format!("unexpected HTTP {s}"),
                        None => format!("TCP connect in {ms} ms"),
                    },
                    response_ms: Some(ms),
                    ..Default::default()
                }
            }
            Err(e) => {
                let mut test = ConnectionTest {
                    ok: false,
                    message: super::describe_error(&e),
                    ..Default::default()
                };
                let cert = e
                    .downcast_ref::<reqwest::Error>()
                    .is_some_and(crate::tls::is_cert_error);
                if cert && self.target.starts_with("https://") {
                    test.presented_fingerprint = crate::tls::fetch_fingerprint(&self.target).await;
                }
                test
            }
        }
    }
}

/// Demo health check with stable, slightly jittering fake results.
pub struct DemoHealthCheck {
    id: String,
    target: String,
}

impl DemoHealthCheck {
    pub fn new(cfg: &InstanceConfig) -> Self {
        Self {
            id: cfg.id.clone(),
            target: cfg.base_url.clone().unwrap_or_else(|| "demo".into()),
        }
    }
}

#[async_trait]
impl ServiceModule for DemoHealthCheck {
    fn kind(&self) -> ServiceKind {
        ServiceKind::HealthCheck
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        use rand::Rng;
        let base = 12 + (crate::crypto::sha256_hex(&self.id).as_bytes()[0] as u64 % 60);
        let ms = base + rand::thread_rng().gen_range(0..8);
        let data = |code| {
            ModuleData::HealthCheck(HealthCheckData {
                check: CheckType::Http,
                target: self.target.clone(),
                status_code: code,
            })
        };
        let (health, reason, ms, code) = match self.id.as_str() {
            "podgrab" => (
                Health::Warning,
                Some("slow response (2410 ms)".into()),
                2410,
                Some(200),
            ),
            "drawio" => (
                Health::Down,
                Some("connection refused or host unreachable".into()),
                0,
                None,
            ),
            _ => (Health::Healthy, None, ms, Some(200)),
        };
        Ok(PollOutcome {
            health,
            health_reason: reason,
            response_ms: (ms > 0).then_some(ms),
            version: None,
            data: data(code),
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        ConnectionTest {
            ok: true,
            message: "HTTP 200 in 23 ms (demo)".into(),
            response_ms: Some(23),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(base_url: &str, settings: serde_json::Value) -> InstanceConfig {
        InstanceConfig {
            id: "vaultwarden".into(),
            kind: ServiceKind::HealthCheck,
            name: "Vaultwarden".into(),
            base_url: Some(base_url.into()),
            settings,
            secrets: Default::default(),
        }
    }

    #[tokio::test]
    async fn http_check_healthy_on_expected_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/alive"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let m = HealthCheckModule::new(&cfg(&server.uri(), json!({"path": "/alive"}))).unwrap();
        let out = m.poll().await.unwrap();
        assert_eq!(out.health, Health::Healthy);
        assert!(out.response_ms.is_some());
        assert!(m.test_connection().await.ok);
    }

    #[tokio::test]
    async fn http_check_down_on_wrong_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let m = HealthCheckModule::new(&cfg(&server.uri(), json!({}))).unwrap();
        let out = m.poll().await.unwrap();
        assert_eq!(out.health, Health::Down);
        assert_eq!(out.health_reason.as_deref(), Some("HTTP 503"));
    }

    #[tokio::test]
    async fn http_check_warns_when_slow() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(80)))
            .mount(&server)
            .await;
        let m = HealthCheckModule::new(&cfg(&server.uri(), json!({"slow_ms": 20}))).unwrap();
        assert_eq!(m.poll().await.unwrap().health, Health::Warning);
    }

    #[tokio::test]
    async fn tcp_check() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut c = cfg(
            "http://unused",
            json!({"check": "tcp", "address": addr.to_string()}),
        );
        c.base_url = None;
        let m = HealthCheckModule::new(&c).unwrap();
        assert_eq!(m.poll().await.unwrap().health, Health::Healthy);
        drop(listener);
        assert_eq!(m.poll().await.unwrap().health, Health::Down);
    }

    #[test]
    fn validation() {
        assert!(validate(&cfg("ftp://x", json!({}))).is_err());
        assert!(validate(&cfg("http://x", json!({"check": "tcp", "address": "x"}))).is_err());
        assert!(validate(&cfg("http://x", json!({"tls_fingerprint": "zz"}))).is_err());
        assert!(validate(&cfg("https://x", json!({}))).is_ok());
    }
}
