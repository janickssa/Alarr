//! Authentik (read-only): logins and failed logins in the last 24 h, recent events,
//! active sessions and outpost health.
//!
//! Checked against Authentik's `schema.yml` (API v3): `GET /api/v3/admin/version/`,
//! `GET /api/v3/events/events/?action=&ordering=-created&page_size=&page=` (paginated:
//! `pagination.next` is `0` on the last page), `GET /api/v3/core/authenticated_sessions/`,
//! `GET /api/v3/outposts/instances/` and `GET /api/v3/outposts/instances/{uuid}/health/`.
//! Auth is `Authorization: Bearer <token>`. Only a few event fields are kept (action, user,
//! client IP, time, app); event `context` is not passed on.

use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use super::http::{ServiceHttp, validate_http};
use super::{
    ConnectionTest, Health, InstanceConfig, ModuleData, PollOutcome, ServiceKind, ServiceModule,
    describe_error,
};

/// An outpost counts as healthy when it checked in this recently.
const OUTPOST_STALE_MIN: i64 = 5;
/// Pages of 100 events read when counting a day of logins.
const MAX_PAGES: u32 = 5;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuthEvent {
    /// `login`, `login_failed`, `logout`, `authorize_application`, …
    pub action: String,
    pub user: Option<String>,
    pub client_ip: Option<String>,
    pub app: Option<String>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Outpost {
    pub id: String,
    pub name: String,
    /// `proxy`, `ldap`, `radius`, `rac`
    pub kind: String,
    pub managed: bool,
    pub healthy: bool,
    pub last_seen: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub outdated: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuthentikData {
    pub logins_24h: u64,
    pub failed_24h: u64,
    /// True when more than [`MAX_PAGES`] pages of logins happened and the count is a floor.
    pub logins_capped: bool,
    pub sessions: Option<u64>,
    pub events: Vec<AuthEvent>,
    pub outposts: Vec<Outpost>,
    /// Newer Authentik release, when the server reports it is outdated.
    pub update_available: Option<String>,
}

impl AuthentikData {
    pub fn headline(&self) -> (String, Option<String>) {
        (
            format!(
                "{}{} login{} · 24 h",
                self.logins_24h,
                if self.logins_capped { "+" } else { "" },
                if self.logins_24h == 1 { "" } else { "s" }
            ),
            Some(if self.failed_24h > 0 {
                format!("{} failed", self.failed_24h)
            } else {
                "no failed logins".into()
            }),
        )
    }

    fn health(&self) -> (Health, Option<String>) {
        if let Some(o) = self.outposts.iter().find(|o| !o.healthy) {
            return (
                Health::Warning,
                Some(format!("outpost {} is not checking in", o.name)),
            );
        }
        (Health::Healthy, None)
    }
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["api_key"])
}

pub struct AuthentikModule {
    http: Option<ServiceHttp>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|x| !x.is_empty())
        .map(str::to_owned)
}

fn date(v: &Value, k: &str) -> Option<DateTime<Utc>> {
    v.get(k)
        .and_then(Value::as_str)
        .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
        .map(|d| d.with_timezone(&Utc))
}

pub fn parse_event(e: &Value) -> Option<AuthEvent> {
    let user = e.get("user").and_then(|u| s(u, "username")).or_else(|| {
        // Failed logins carry the attempted name in the context.
        e.get("context").and_then(|c| s(c, "username"))
    });
    Some(AuthEvent {
        action: s(e, "action")?,
        user,
        client_ip: s(e, "client_ip"),
        app: s(e, "app"),
        at: date(e, "created")?,
    })
}

impl AuthentikModule {
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

    /// Events of `action` since `since`, newest first, reading at most [`MAX_PAGES`] pages.
    async fn count_since(
        http: &ServiceHttp,
        action: &str,
        since: DateTime<Utc>,
    ) -> anyhow::Result<(u64, bool)> {
        let mut count = 0;
        for page in 1..=MAX_PAGES {
            let body: Value = http
                .get(
                    "/api/v3/events/events/",
                    &[
                        ("action", action.to_owned()),
                        ("ordering", "-created".into()),
                        ("page_size", "100".into()),
                        ("page", page.to_string()),
                    ],
                )
                .await?;
            let results = body
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for e in &results {
                if date(e, "created").is_some_and(|d| d >= since) {
                    count += 1;
                } else {
                    return Ok((count, false));
                }
            }
            let next = body
                .get("pagination")
                .and_then(|p| p.get("next"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            if next == 0.0 || results.is_empty() {
                return Ok((count, false));
            }
        }
        Ok((count, true))
    }

    async fn outposts(http: &ServiceHttp, now: DateTime<Utc>) -> anyhow::Result<Vec<Outpost>> {
        let body: Value = http
            .get("/api/v3/outposts/instances/", &[("page_size", "50".into())])
            .await?;
        let list = body
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::new();
        for o in list {
            let Some(id) = s(&o, "pk") else { continue };
            let health: Vec<Value> = http
                .get(&format!("/api/v3/outposts/instances/{id}/health/"), &[])
                .await
                .unwrap_or_default();
            out.push(outpost(&o, &health, now));
        }
        Ok(out)
    }

    async fn fetch(&self) -> anyhow::Result<(AuthentikData, Option<String>)> {
        let Some(http) = &self.http else {
            return Ok((demo_data(), Some("2025.8.4".into())));
        };
        let now = Utc::now();
        let since = now - Duration::hours(24);
        let recent_q = [
            ("ordering", "-created".to_owned()),
            ("page_size", "15".to_owned()),
        ];
        let sessions_q = [("page_size", "1".to_owned())];
        let (version, logins, failed, recent, sessions, outposts) = tokio::try_join!(
            http.get::<Value>("/api/v3/admin/version/", &[]),
            Self::count_since(http, "login", since),
            Self::count_since(http, "login_failed", since),
            http.get::<Value>("/api/v3/events/events/", &recent_q),
            async {
                // Needs a token with permission to view sessions; optional.
                Ok::<_, anyhow::Error>(
                    http.get::<Value>("/api/v3/core/authenticated_sessions/", &sessions_q)
                        .await
                        .ok(),
                )
            },
            Self::outposts(http, now),
        )?;
        let data = AuthentikData {
            logins_24h: logins.0,
            failed_24h: failed.0,
            logins_capped: logins.1,
            sessions: sessions
                .as_ref()
                .and_then(|b| b.get("pagination"))
                .and_then(|p| p.get("count"))
                .and_then(Value::as_f64)
                .map(|c| c as u64),
            events: recent
                .get("results")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(parse_event).collect())
                .unwrap_or_default(),
            outposts,
            update_available: version
                .get("outdated")
                .and_then(Value::as_bool)
                .filter(|o| *o)
                .and_then(|_| s(&version, "version_latest")),
        };
        Ok((data, s(&version, "version_current")))
    }
}

pub fn outpost(o: &Value, health: &[Value], now: DateTime<Utc>) -> Outpost {
    let last_seen = health.iter().filter_map(|h| date(h, "last_seen")).max();
    let latest = health
        .iter()
        .max_by_key(|h| date(h, "last_seen"))
        .cloned()
        .unwrap_or(Value::Null);
    Outpost {
        id: s(o, "pk").unwrap_or_default(),
        name: s(o, "name").unwrap_or_default(),
        kind: s(o, "type").unwrap_or_default(),
        managed: o.get("managed").is_some_and(|m| !m.is_null()),
        healthy: last_seen.is_some_and(|l| now - l < Duration::minutes(OUTPOST_STALE_MIN)),
        last_seen,
        version: s(&latest, "version"),
        outdated: latest
            .get("version_outdated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn demo_data() -> AuthentikData {
    let now = Utc::now();
    let ev = |action: &str, user: &str, ip: &str, app: Option<&str>, mins| AuthEvent {
        action: action.into(),
        user: Some(user.into()),
        client_ip: Some(ip.into()),
        app: app.map(str::to_owned),
        at: now - Duration::minutes(mins),
    };
    AuthentikData {
        logins_24h: 37,
        failed_24h: 4,
        logins_capped: false,
        sessions: Some(12),
        events: vec![
            ev("login", "janick", "10.0.20.31", None, 6),
            ev(
                "authorize_application",
                "janick",
                "10.0.20.31",
                Some("Nextcloud"),
                6,
            ),
            ev("login_failed", "admin", "203.0.113.24", None, 42),
            ev("login_failed", "admin", "203.0.113.24", None, 43),
            ev("login", "mara", "10.0.20.44", None, 95),
            ev(
                "authorize_application",
                "mara",
                "10.0.20.44",
                Some("Jellyfin"),
                96,
            ),
            ev("logout", "leo", "10.0.20.52", None, 180),
            ev("login", "leo", "10.0.20.52", None, 310),
        ],
        outposts: vec![
            Outpost {
                id: "a1".into(),
                name: "authentik Embedded Outpost".into(),
                kind: "proxy".into(),
                managed: true,
                healthy: true,
                last_seen: Some(now - Duration::seconds(40)),
                version: Some("2025.8.4".into()),
                outdated: false,
            },
            Outpost {
                id: "b2".into(),
                name: "ldap".into(),
                kind: "ldap".into(),
                managed: false,
                healthy: true,
                last_seen: Some(now - Duration::seconds(75)),
                version: Some("2025.8.4".into()),
                outdated: false,
            },
        ],
        update_available: None,
    }
}

#[async_trait]
impl ServiceModule for AuthentikModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Authentik
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
            data: ModuleData::Authentik(Box::new(data)),
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((d, version)) => ConnectionTest {
                ok: true,
                message: format!(
                    "{} logins in the last 24 h, {} outposts",
                    d.logins_24h,
                    d.outposts.len()
                ),
                version,
                warnings: if d.sessions.is_none() {
                    vec!["The token can't list sessions; active sessions won't show".into()]
                } else {
                    vec![]
                },
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
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(url: &str) -> InstanceConfig {
        InstanceConfig {
            id: "authentik".into(),
            kind: ServiceKind::Authentik,
            name: "Authentik".into(),
            base_url: Some(url.into()),
            settings: json!({}),
            secrets: [("api_key".to_owned(), "tok".to_owned())].into(),
        }
    }

    fn page(events: Vec<Value>, next: u32) -> Value {
        json!({"pagination": {"next": next, "previous": 0, "count": 999, "current": 1, "total_pages": 9,
                              "start_index": 1, "end_index": 100},
               "results": events, "autocomplete": {}})
    }

    #[tokio::test]
    async fn counts_logins_and_reads_outposts() {
        let s = MockServer::start().await;
        let now = Utc::now();
        let at = |h: i64| (now - Duration::hours(h)).to_rfc3339();
        Mock::given(method("GET"))
            .and(path("/api/v3/admin/version/"))
            .and(header("authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "version_current": "2025.8.4", "version_latest": "2025.10.1", "outdated": true})))
            .mount(&s)
            .await;
        // Logins: page 1 is all within 24 h, page 2 crosses the boundary.
        Mock::given(method("GET"))
            .and(path("/api/v3/events/events/"))
            .and(query_param("action", "login"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page(
                vec![
                    json!({"action": "login", "user": {"username": "a"}, "created": at(1)}),
                    json!({"action": "login", "user": {"username": "b"}, "created": at(2)}),
                ],
                2,
            )))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/events/events/"))
            .and(query_param("action", "login"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page(
                vec![
                    json!({"action": "login", "user": {"username": "c"}, "created": at(23)}),
                    json!({"action": "login", "user": {"username": "d"}, "created": at(30)}),
                ],
                3,
            )))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/events/events/"))
            .and(query_param("action", "login_failed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page(
                vec![
                    json!({"action": "login_failed", "user": {}, "context": {"username": "admin"},
                            "client_ip": "203.0.113.9", "created": at(3)}),
                ],
                0,
            )))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/events/events/"))
            .and(query_param("page_size", "15"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page(
                vec![json!({"action": "login_failed", "user": {}, "context": {"username": "admin", "password": "x"},
                            "client_ip": "203.0.113.9", "created": at(3)})],
                0,
            )))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/core/authenticated_sessions/"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/outposts/instances/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"pagination": {"next": 0}, "results": [
                {"pk": "u1", "name": "embedded", "type": "proxy", "managed": "goauthentik.io/outposts/embedded"},
                {"pk": "u2", "name": "ldap", "type": "ldap", "managed": null}
            ]})))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/outposts/instances/u1/health/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"uid": "x", "last_seen": (now - Duration::seconds(30)).to_rfc3339(), "version": "2025.8.4", "version_outdated": false}
            ])))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/outposts/instances/u2/health/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"uid": "y", "last_seen": (now - Duration::hours(2)).to_rfc3339(), "version": "2025.6.0", "version_outdated": true}
            ])))
            .mount(&s)
            .await;
        let m = AuthentikModule::new(&cfg(&s.uri())).unwrap();
        let out = m.poll().await.unwrap();
        let ModuleData::Authentik(d) = out.data else {
            panic!()
        };
        assert_eq!((d.logins_24h, d.failed_24h, d.logins_capped), (3, 1, false));
        assert_eq!(d.sessions, None);
        assert_eq!(d.events[0].user.as_deref(), Some("admin"));
        assert!(!serde_json::to_string(&*d).unwrap().contains("password"));
        assert_eq!(d.update_available.as_deref(), Some("2025.10.1"));
        assert!(d.outposts[0].healthy && d.outposts[0].managed);
        assert!(!d.outposts[1].healthy && d.outposts[1].outdated);
        assert_eq!(out.health, Health::Warning);
        assert_eq!(
            out.health_reason.as_deref(),
            Some("outpost ldap is not checking in")
        );
        assert_eq!(out.version.as_deref(), Some("2025.8.4"));
        assert_eq!(
            d.headline(),
            ("3 logins · 24 h".into(), Some("1 failed".into()))
        );
    }
}
