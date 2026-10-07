//! Nginx Proxy Manager (read-only): proxy hosts and certificate expiry.
//!
//! Checked against NPM's `backend/schema` (OpenAPI): `POST /api/tokens` with
//! `{identity, secret}` returns `{token, expires}` (a JWT used as `Authorization: Bearer`),
//! `GET /api/nginx/proxy-hosts`, `GET /api/nginx/certificates` and the public `GET /api/`
//! (status and version). Older releases send booleans as `0`/`1` and dates as
//! `YYYY-MM-DD HH:MM:SS`; both forms are accepted. Certificate `meta` (which can hold a
//! custom certificate's private key) is never read.

use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use utoipa::ToSchema;

use super::http::{ServiceHttp, validate_http};
use super::{
    ConnectionTest, Health, InstanceConfig, ModuleData, PollOutcome, ServiceKind, ServiceModule,
    describe_error,
};

/// Certificates expiring within this many days are flagged (amber in the UI).
pub const CERT_WARN_DAYS: i64 = 14;
/// ... and within this many days (or expired) red.
pub const CERT_CRITICAL_DAYS: i64 = 3;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProxyHost {
    pub id: i64,
    pub domains: Vec<String>,
    /// `http://10.0.20.108:8096`
    pub forward: String,
    pub enabled: bool,
    pub certificate_id: Option<i64>,
    pub ssl_forced: bool,
    /// Nginx accepted the host's config (`meta.nginx_online`), when reported.
    pub online: Option<bool>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Certificate {
    pub id: i64,
    pub name: String,
    pub domains: Vec<String>,
    /// `letsencrypt` or `other`.
    pub provider: String,
    pub expires_at: Option<DateTime<Utc>>,
    /// Whole days left; negative when expired.
    pub days_left: Option<i64>,
    /// Number of proxy hosts using it.
    pub hosts: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NpmData {
    pub hosts: Vec<ProxyHost>,
    pub certificates: Vec<Certificate>,
}

impl NpmData {
    /// Certificates in use that expire within [`CERT_WARN_DAYS`].
    pub fn expiring(&self) -> Vec<&Certificate> {
        self.certificates
            .iter()
            .filter(|c| c.hosts > 0 && c.days_left.is_some_and(|d| d < CERT_WARN_DAYS))
            .collect()
    }

    pub fn headline(&self) -> (String, Option<String>) {
        let enabled = self.hosts.iter().filter(|h| h.enabled).count();
        let expiring = self.expiring();
        let detail = if let Some(c) = expiring.iter().min_by_key(|c| c.days_left) {
            Some(cert_phrase(c))
        } else {
            self.certificates
                .iter()
                .filter_map(|c| c.days_left)
                .min()
                .map(|d| format!("next certificate renewal in {d} d"))
        };
        (
            format!(
                "{enabled} proxy host{}",
                if enabled == 1 { "" } else { "s" }
            ),
            detail,
        )
    }

    fn health(&self) -> (Health, Option<String>) {
        if let Some(c) = self.expiring().iter().min_by_key(|c| c.days_left) {
            return (Health::Warning, Some(cert_phrase(c)));
        }
        if let Some(h) = self
            .hosts
            .iter()
            .find(|h| h.enabled && h.online == Some(false))
        {
            return (
                Health::Warning,
                Some(format!(
                    "{} is offline in nginx",
                    h.domains.first().map(String::as_str).unwrap_or("a host")
                )),
            );
        }
        (Health::Healthy, None)
    }
}

fn cert_phrase(c: &Certificate) -> String {
    match c.days_left {
        Some(d) if d < 0 => format!("certificate for {} expired", c.name),
        Some(0) => format!("certificate for {} expires today", c.name),
        Some(d) => format!("certificate for {} expires in {d} d", c.name),
        None => format!("certificate for {}", c.name),
    }
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["password"])?;
    cfg.setting_str("username")
        .ok_or("email (username) is required")?;
    Ok(())
}

struct Token {
    value: String,
    expires: DateTime<Utc>,
}

pub struct NpmModule {
    http: Option<ServiceHttp>,
    identity: String,
    secret: String,
    token: Mutex<Option<Token>>,
}

impl NpmModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        Ok(Self {
            http: Some(ServiceHttp::new(cfg, &[])?),
            identity: cfg.setting_str("username").unwrap_or_default().to_owned(),
            secret: cfg.secret("password").unwrap_or_default().to_owned(),
            token: Mutex::new(None),
        })
    }

    pub fn demo() -> Self {
        Self {
            http: None,
            identity: String::new(),
            secret: String::new(),
            token: Mutex::new(None),
        }
    }

    async fn login(&self, http: &ServiceHttp) -> anyhow::Result<Token> {
        let response = http
            .request(Method::POST, "/api/tokens")
            .json(&json!({"identity": self.identity, "secret": self.secret}))
            .send()
            .await?;
        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::BAD_REQUEST
        ) {
            anyhow::bail!("authentication failed (check the email and password)");
        }
        let body: Value = ServiceHttp::send_checked(response)?.json().await?;
        if body.get("requires_2fa").is_some() {
            anyhow::bail!("the account uses two-factor authentication; use one without it");
        }
        let value = body
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("no token in the login response"))?
            .to_owned();
        let expires = body
            .get("expires")
            .and_then(Value::as_str)
            .and_then(parse_date)
            .unwrap_or_else(|| Utc::now() + Duration::hours(1));
        Ok(Token { value, expires })
    }

    /// GET with a valid token, logging in (again) when needed.
    async fn get(&self, http: &ServiceHttp, path: &str) -> anyhow::Result<Value> {
        for attempt in 0..2 {
            let token = {
                let mut guard = self.token.lock().await;
                let fresh = guard
                    .as_ref()
                    .is_some_and(|t| t.expires - Duration::minutes(5) > Utc::now());
                if !fresh || attempt > 0 {
                    *guard = Some(self.login(http).await?);
                }
                guard.as_ref().map(|t| t.value.clone()).unwrap_or_default()
            };
            let response = http
                .request(Method::GET, path)
                .bearer_auth(&token)
                .send()
                .await?;
            if response.status() == StatusCode::UNAUTHORIZED && attempt == 0 {
                continue;
            }
            return Ok(ServiceHttp::send_checked(response)?.json().await?);
        }
        unreachable!()
    }

    async fn fetch(&self) -> anyhow::Result<(NpmData, Option<String>)> {
        let Some(http) = &self.http else {
            return Ok((demo_data(), Some("2.12.6".into())));
        };
        let status: Value = http.get("/api/", &[]).await?;
        let hosts = self.get(http, "/api/nginx/proxy-hosts").await?;
        let certs = self.get(http, "/api/nginx/certificates").await?;
        let version = status.get("version").map(|v| {
            let n = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
            format!("{}.{}.{}", n("major"), n("minor"), n("revision"))
        });
        Ok((parse(&hosts, &certs, Utc::now()), version))
    }
}

/// RFC 3339 or NPM's older `YYYY-MM-DD HH:MM:SS` (UTC).
fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|n| n.and_utc())
        })
}

/// `true`/`false` or `1`/`0`.
fn flag(v: &Value, key: &str) -> bool {
    match v.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        _ => false,
    }
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse(hosts: &Value, certs: &Value, now: DateTime<Utc>) -> NpmData {
    let hosts: Vec<ProxyHost> = hosts
        .as_array()
        .map(|a| {
            a.iter()
                .map(|h| {
                    let meta = h.get("meta");
                    ProxyHost {
                        id: h.get("id").and_then(Value::as_i64).unwrap_or_default(),
                        domains: strings(h, "domain_names"),
                        forward: format!(
                            "{}://{}:{}",
                            h.get("forward_scheme")
                                .and_then(Value::as_str)
                                .unwrap_or("http"),
                            h.get("forward_host")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                            h.get("forward_port").and_then(Value::as_u64).unwrap_or(0)
                        ),
                        enabled: flag(h, "enabled"),
                        certificate_id: h
                            .get("certificate_id")
                            .and_then(Value::as_i64)
                            .filter(|id| *id > 0),
                        ssl_forced: flag(h, "ssl_forced"),
                        online: meta
                            .and_then(|m| m.get("nginx_online"))
                            .and_then(Value::as_bool),
                        error: meta
                            .and_then(|m| m.get("nginx_err"))
                            .and_then(Value::as_str)
                            .filter(|e| !e.is_empty())
                            .map(str::to_owned),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let mut certificates: Vec<Certificate> = certs
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| {
                    let id = c.get("id").and_then(Value::as_i64).unwrap_or_default();
                    let domains = strings(c, "domain_names");
                    let expires_at = c
                        .get("expires_on")
                        .and_then(Value::as_str)
                        .and_then(parse_date);
                    Certificate {
                        id,
                        name: c
                            .get("nice_name")
                            .and_then(Value::as_str)
                            .filter(|n| !n.is_empty())
                            .map(str::to_owned)
                            .or_else(|| domains.first().cloned())
                            .unwrap_or_else(|| format!("#{id}")),
                        domains,
                        provider: c
                            .get("provider")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        expires_at,
                        days_left: expires_at.map(|e| (e - now).num_seconds().div_euclid(86_400)),
                        hosts: hosts
                            .iter()
                            .filter(|h| h.certificate_id == Some(id))
                            .count() as u32,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    certificates.sort_by_key(|c| c.days_left.unwrap_or(i64::MAX));
    let mut hosts = hosts;
    hosts.sort_by(|a, b| a.domains.first().cmp(&b.domains.first()));
    NpmData {
        hosts,
        certificates,
    }
}

fn demo_data() -> NpmData {
    let now = Utc::now();
    let host = |id, domain: &str, ip: &str, port, cert| ProxyHost {
        id,
        domains: vec![domain.into()],
        forward: format!("http://{ip}:{port}"),
        enabled: true,
        certificate_id: Some(cert),
        ssl_forced: true,
        online: Some(true),
        error: None,
    };
    let hosts = vec![
        host(1, "jellyfin.example.net", "10.0.20.105", 8096, 1),
        host(2, "requests.example.net", "10.0.20.126", 5055, 1),
        host(3, "cloud.example.net", "10.0.20.113", 80, 2),
        host(4, "auth.example.net", "10.0.20.121", 9000, 3),
        host(5, "vault.example.net", "10.0.20.114", 80, 3),
        host(6, "docs.example.net", "10.0.20.117", 8000, 4),
    ];
    let cert = |id, name: &str, domains: &[&str], days: i64| {
        let expires_at = now + Duration::days(days) + Duration::hours(6);
        Certificate {
            id,
            name: name.into(),
            domains: domains.iter().map(|d| (*d).to_owned()).collect(),
            provider: "letsencrypt".into(),
            expires_at: Some(expires_at),
            days_left: Some(days),
            hosts: hosts
                .iter()
                .filter(|h| h.certificate_id == Some(id))
                .count() as u32,
        }
    };
    NpmData {
        certificates: vec![
            cert(4, "docs.example.net", &["docs.example.net"], 9),
            cert(3, "*.example.net", &["*.example.net", "example.net"], 41),
            cert(2, "cloud.example.net", &["cloud.example.net"], 63),
            cert(
                1,
                "media",
                &["jellyfin.example.net", "requests.example.net"],
                77,
            ),
        ],
        hosts,
    }
}

#[async_trait]
impl ServiceModule for NpmModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::NginxProxyManager
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let (data, version) = self.fetch().await?;
        let (health, reason) = data.health();
        Ok(PollOutcome {
            health,
            health_reason: reason,
            response_ms: Some(started.elapsed().as_millis() as u64),
            version,
            data: ModuleData::Npm(Box::new(data)),
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((d, version)) => ConnectionTest {
                ok: true,
                message: format!(
                    "{} proxy hosts, {} certificates",
                    d.hosts.len(),
                    d.certificates.len()
                ),
                version,
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
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(url: &str) -> InstanceConfig {
        InstanceConfig {
            id: "npm".into(),
            kind: ServiceKind::NginxProxyManager,
            name: "Nginx Proxy Manager".into(),
            base_url: Some(url.into()),
            settings: json!({"username": "admin@example.com"}),
            secrets: [("password".to_owned(), "pw".to_owned())].into(),
        }
    }

    #[tokio::test]
    async fn logs_in_and_reads_hosts_and_certificates() {
        let s = MockServer::start().await;
        let expires = (Utc::now() + Duration::days(1)).to_rfc3339();
        Mock::given(method("POST"))
            .and(path("/api/tokens"))
            .and(body_json(
                json!({"identity": "admin@example.com", "secret": "pw"}),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"token": "jwt1", "expires": expires})),
            )
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"status": "OK", "setup": true, "version": {"major": 2, "minor": 12, "revision": 6}}),
            ))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/nginx/proxy-hosts"))
            .and(header("authorization", "Bearer jwt1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "domain_names": ["b.example.net"], "forward_scheme": "http", "forward_host": "10.0.0.5",
                 "forward_port": 8096, "enabled": 1, "certificate_id": 7, "ssl_forced": 1,
                 "meta": {"nginx_online": true, "nginx_err": null}},
                {"id": 2, "domain_names": ["a.example.net"], "forward_scheme": "https", "forward_host": "10.0.0.6",
                 "forward_port": 443, "enabled": false, "certificate_id": 0, "ssl_forced": false, "meta": {}}
            ])))
            .mount(&s)
            .await;
        let soon = (Utc::now() + Duration::days(5) + Duration::hours(1))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        Mock::given(method("GET"))
            .and(path("/api/nginx/certificates"))
            .and(header("authorization", "Bearer jwt1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 7, "provider": "letsencrypt", "nice_name": "", "domain_names": ["b.example.net"],
                 "expires_on": soon, "meta": {"certificate_key": "SECRET"}},
                {"id": 8, "provider": "other", "nice_name": "Unused", "domain_names": ["x.example.net"],
                 "expires_on": "2020-01-01T00:00:00.000Z"}
            ])))
            .mount(&s)
            .await;
        let m = NpmModule::new(&cfg(&s.uri())).unwrap();
        let out = m.poll().await.unwrap();
        let ModuleData::Npm(d) = out.data else {
            panic!()
        };
        assert_eq!(out.version.as_deref(), Some("2.12.6"));
        assert_eq!(d.hosts[0].domains, ["a.example.net"]);
        assert_eq!(d.hosts[1].forward, "http://10.0.0.5:8096");
        assert!(d.hosts[1].enabled && d.hosts[1].ssl_forced);
        assert_eq!(d.hosts[0].certificate_id, None);
        // The expired certificate is unused, so only the 5-day one counts.
        assert_eq!((d.certificates[0].id, d.certificates[0].hosts), (8, 0));
        assert!(d.certificates[0].days_left.unwrap() < 0);
        let used = d.certificates.iter().find(|c| c.id == 7).unwrap();
        assert_eq!(
            (used.days_left, used.hosts, used.name.as_str()),
            (Some(5), 1, "b.example.net")
        );
        assert_eq!(out.health, Health::Warning);
        assert_eq!(
            out.health_reason.as_deref(),
            Some("certificate for b.example.net expires in 5 d")
        );
        assert!(!serde_json::to_string(&*d).unwrap().contains("SECRET"));
        // A second poll reuses the token (login expected once).
        m.poll().await.unwrap();
    }

    #[tokio::test]
    async fn bad_credentials() {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "OK"})))
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/tokens"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(json!({"error": {"message": "Invalid password"}})),
            )
            .mount(&s)
            .await;
        let m = NpmModule::new(&cfg(&s.uri())).unwrap();
        let err = m.poll().await.unwrap_err().to_string();
        assert!(err.contains("authentication failed"), "{err}");
    }
}
