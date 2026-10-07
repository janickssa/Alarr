//! Technitium DNS: 24 h query stats, allowed vs blocked chart, top lists, and pausing
//! blocking for a while (with resume).
//!
//! Checked against Technitium's `APIDOCS.md` and `WebServiceSettingsApi.cs`:
//! `GET /api/dashboard/stats/get?type=LastDay&utc=true`, `GET /api/settings/get`
//! (`enableBlocking`, `temporaryDisableBlockingTill`), `GET /api/settings/temporaryDisableBlocking?minutes=`
//! and `GET /api/settings/set?enableBlocking=true`. The API token goes in the
//! `Authorization: Bearer` header (v15+). Responses carry `status`: `ok`, `error` or
//! `invalid-token` (HTTP 200 either way).

use std::sync::Mutex;
use std::time::Instant;

use anyhow::bail;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Timelike, Utc};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use super::http::{ServiceHttp, fmt_count as group, validate_http};
use super::{
    ActionDescriptor, ActionResult, ConnectionTest, Health, InstanceConfig, ModuleData,
    ModuleError, PollOutcome, ServiceKind, ServiceModule, describe_error,
};

/// Pause lengths offered in the UI, minutes.
pub const PAUSE_MINUTES: [u64; 3] = [5, 30, 60];

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsPoint {
    pub at: DateTime<Utc>,
    pub total: u64,
    pub blocked: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TopEntry {
    /// Domain name, or client IP for top clients.
    pub name: String,
    /// Client host name from reverse lookup, when known.
    pub domain: Option<String>,
    pub hits: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TechnitiumData {
    pub total_queries: u64,
    pub blocked: u64,
    pub cached: u64,
    pub clients: u64,
    pub block_list_zones: u64,
    /// Queries per interval over the last 24 h.
    pub chart: Vec<DnsPoint>,
    pub top_clients: Vec<TopEntry>,
    pub top_domains: Vec<TopEntry>,
    pub top_blocked: Vec<TopEntry>,
    pub blocking_enabled: bool,
    /// Set while blocking is paused; it resumes by itself at this time.
    pub paused_until: Option<DateTime<Utc>>,
}

impl TechnitiumData {
    pub fn blocked_fraction(&self) -> Option<f64> {
        (self.total_queries > 0).then(|| self.blocked as f64 / self.total_queries as f64)
    }

    pub fn headline(&self) -> (String, Option<String>) {
        let pct = self
            .blocked_fraction()
            .map(|f| format!("{:.0} % blocked", f * 100.0))
            .unwrap_or_else(|| "No queries".into());
        let detail = if let Some(until) = self.paused_until {
            format!("Blocking paused · {} left", minutes_left(until))
        } else if !self.blocking_enabled {
            "Blocking off".into()
        } else {
            format!("{} queries · 24 h", group(self.total_queries))
        };
        (pct, Some(detail))
    }

    fn health(&self) -> (Health, Option<String>) {
        if let Some(until) = self.paused_until {
            (
                Health::Warning,
                Some(format!("Blocking paused · {} left", minutes_left(until))),
            )
        } else if !self.blocking_enabled {
            (Health::Warning, Some("Blocking is off".into()))
        } else {
            (Health::Healthy, None)
        }
    }
}

/// Minutes until `until`, rounded up: "4 min".
fn minutes_left(until: DateTime<Utc>) -> String {
    let secs = (until - Utc::now()).num_seconds().max(0);
    format!("{} min", (secs + 59) / 60)
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["api_key"])
}

pub struct TechnitiumModule {
    http: Option<ServiceHttp>,
    /// Demo mode only: when blocking was paused until.
    demo_paused: Mutex<Option<DateTime<Utc>>>,
}

impl TechnitiumModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        let key = cfg.secret("api_key").unwrap_or_default();
        Ok(Self {
            http: Some(ServiceHttp::new(
                cfg,
                &[("authorization", format!("Bearer {key}"))],
            )?),
            demo_paused: Mutex::new(None),
        })
    }

    pub fn demo() -> Self {
        Self {
            http: None,
            demo_paused: Mutex::new(None),
        }
    }

    /// Call an endpoint and unwrap Technitium's `{status, response}` envelope.
    async fn api(
        http: &ServiceHttp,
        path: &str,
        query: &[(&str, String)],
    ) -> anyhow::Result<Value> {
        let body: Value = http.get(path, query).await?;
        match body.get("status").and_then(Value::as_str) {
            Some("ok") => Ok(body.get("response").cloned().unwrap_or(Value::Null)),
            Some("invalid-token") => bail!("authentication failed (check the API token)"),
            Some("2fa-required") => {
                bail!("the account needs two-factor authentication; use an API token")
            }
            _ => bail!(
                "{}",
                body.get("errorMessage")
                    .and_then(Value::as_str)
                    .unwrap_or("unexpected response")
            ),
        }
    }

    async fn fetch(&self) -> anyhow::Result<(TechnitiumData, Option<String>)> {
        let Some(http) = &self.http else {
            let mut paused = self.demo_paused.lock().expect("lock");
            if paused.is_some_and(|p| p <= Utc::now()) {
                *paused = None;
            }
            return Ok((demo_data(*paused), Some("15.5".into())));
        };
        let stats_q = [("type", "LastDay".to_owned()), ("utc", "true".to_owned())];
        let (stats, settings) = tokio::try_join!(
            Self::api(http, "/api/dashboard/stats/get", &stats_q),
            Self::api(http, "/api/settings/get", &[]),
        )?;
        Ok((
            parse(&stats, &settings),
            settings
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ))
    }
}

pub fn parse(stats: &Value, settings: &Value) -> TechnitiumData {
    let s = stats.get("stats").cloned().unwrap_or(Value::Null);
    let n = |k: &str| s.get(k).and_then(Value::as_u64).unwrap_or(0);
    let chart = stats.get("mainChartData");
    let labels: Vec<DateTime<Utc>> = chart
        .and_then(|c| c.get("labels"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|l| DateTime::parse_from_rfc3339(l.as_str()?).ok())
                .map(|d| d.with_timezone(&Utc))
                .collect()
        })
        .unwrap_or_default();
    let series = |label: &str| -> Vec<u64> {
        chart
            .and_then(|c| c.get("datasets"))
            .and_then(Value::as_array)
            .and_then(|sets| {
                sets.iter()
                    .find(|d| d.get("label").and_then(Value::as_str) == Some(label))
            })
            .and_then(|d| d.get("data"))
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|v| v.as_u64().unwrap_or(0)).collect())
            .unwrap_or_default()
    };
    let (total, blocked) = (series("Total"), series("Blocked"));
    let top = |k: &str| -> Vec<TopEntry> {
        stats
            .get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .take(10)
                    .filter_map(|e| {
                        Some(TopEntry {
                            name: e.get("name")?.as_str()?.to_owned(),
                            domain: e
                                .get("domain")
                                .and_then(Value::as_str)
                                .filter(|d| !d.is_empty())
                                .map(str::to_owned),
                            hits: e.get("hits").and_then(Value::as_u64).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let enabled = settings
        .get("enableBlocking")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let paused_until = settings
        .get("temporaryDisableBlockingTill")
        .and_then(Value::as_str)
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|d| d.with_timezone(&Utc))
        .filter(|d| !enabled && *d > Utc::now());
    TechnitiumData {
        total_queries: n("totalQueries"),
        blocked: n("totalBlocked"),
        cached: n("totalCached"),
        clients: n("totalClients"),
        block_list_zones: n("blockListZones"),
        chart: labels
            .iter()
            .enumerate()
            .map(|(i, at)| DnsPoint {
                at: *at,
                total: total.get(i).copied().unwrap_or(0),
                blocked: blocked.get(i).copied().unwrap_or(0),
            })
            .collect(),
        top_clients: top("topClients"),
        top_domains: top("topDomains"),
        top_blocked: top("topBlockedDomains"),
        blocking_enabled: enabled,
        paused_until,
    }
}

fn demo_data(paused_until: Option<DateTime<Utc>>) -> TechnitiumData {
    let now = Utc::now()
        .with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or_else(Utc::now)
        .with_minute(0)
        .unwrap_or_else(Utc::now);
    let chart: Vec<DnsPoint> = (0..24)
        .map(|i| {
            let at = now - Duration::hours(23 - i);
            let hour = at.hour() as f64;
            // Quiet at night, busy in the evening.
            let load = 0.35 + 0.65 * ((hour - 4.0) / 24.0 * std::f64::consts::TAU).sin().abs();
            let total = (1400.0 + 2600.0 * load + (i * 37 % 211) as f64) as u64;
            DnsPoint {
                at,
                total,
                blocked: total * (11 + (i as u64 * 7) % 6) / 100,
            }
        })
        .collect();
    let total_queries = chart.iter().map(|p| p.total).sum();
    let blocked = chart.iter().map(|p| p.blocked).sum();
    let e = |name: &str, domain: Option<&str>, hits| TopEntry {
        name: name.into(),
        domain: domain.map(str::to_owned),
        hits,
    };
    TechnitiumData {
        total_queries,
        blocked,
        cached: total_queries * 46 / 100,
        clients: 23,
        block_list_zones: 312_480,
        chart,
        top_clients: vec![
            e("10.0.20.50", Some("living-room-tv.lan"), 14_220),
            e("10.0.20.31", Some("janick-laptop.lan"), 9_874),
            e("10.0.20.120", Some("qbittorrent.lan"), 6_105),
            e("10.0.20.44", Some("phone-mara.lan"), 4_390),
            e("10.0.20.108", Some("jellyfin.lan"), 2_811),
        ],
        top_domains: vec![
            e("api.themoviedb.org", None, 5_620),
            e("time.apple.com", None, 3_998),
            e("connectivitycheck.gstatic.com", None, 3_410),
            e("github.com", None, 2_034),
            e("deb.debian.org", None, 1_877),
        ],
        top_blocked: vec![
            e("telemetry.example-tv.com", None, 3_812),
            e("ads.tracker.example", None, 2_240),
            e("metrics.smart-plug.example", None, 1_902),
            e("doubleclick.example", None, 1_118),
            e("beacon.analytics.example", None, 806),
        ],
        blocking_enabled: paused_until.is_none(),
        paused_until,
    }
}

#[async_trait]
impl ServiceModule for TechnitiumModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Technitium
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
            data: ModuleData::Technitium(Box::new(data)),
        })
    }

    fn actions(&self) -> Vec<ActionDescriptor> {
        vec![
            ActionDescriptor::new("pause_blocking", "Pause blocking", false),
            ActionDescriptor::new("resume_blocking", "Resume blocking", false),
        ]
    }

    async fn run_action(&self, action: &str, args: Value) -> Result<ActionResult, ModuleError> {
        match action {
            "pause_blocking" => {
                let minutes = args
                    .get("minutes")
                    .and_then(Value::as_u64)
                    .filter(|m| PAUSE_MINUTES.contains(m))
                    .ok_or_else(|| {
                        ModuleError::InvalidArgs("minutes must be 5, 30 or 60".into())
                    })?;
                match &self.http {
                    None => {
                        *self.demo_paused.lock().expect("lock") =
                            Some(Utc::now() + Duration::minutes(minutes as i64));
                    }
                    Some(http) => {
                        Self::api(
                            http,
                            "/api/settings/temporaryDisableBlocking",
                            &[("minutes", minutes.to_string())],
                        )
                        .await?;
                    }
                }
                Ok(ActionResult {
                    ok: true,
                    message: format!("Blocking paused for {minutes} min"),
                    task: None,
                })
            }
            "resume_blocking" => {
                match &self.http {
                    None => *self.demo_paused.lock().expect("lock") = None,
                    Some(http) => {
                        Self::api(
                            http,
                            "/api/settings/set",
                            &[("enableBlocking", "true".to_owned())],
                        )
                        .await?;
                    }
                }
                Ok(ActionResult {
                    ok: true,
                    message: "Blocking resumed".into(),
                    task: None,
                })
            }
            _ => Err(ModuleError::UnknownAction(action.into())),
        }
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((d, version)) => ConnectionTest {
                ok: true,
                message: format!("{} queries in the last 24 h", group(d.total_queries)),
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
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(url: &str) -> InstanceConfig {
        InstanceConfig {
            id: "technitium".into(),
            kind: ServiceKind::Technitium,
            name: "Technitium DNS".into(),
            base_url: Some(url.into()),
            settings: json!({}),
            secrets: [("api_key".to_owned(), "tok".to_owned())].into(),
        }
    }

    fn stats() -> Value {
        json!({"status": "ok", "response": {
            "stats": {"totalQueries": 925, "totalBlocked": 49, "totalCached": 481, "totalClients": 6, "blockListZones": 307447},
            "mainChartData": {
                "labelFormat": "HH:mm",
                "labels": ["2024-02-04T10:38:00.0000000Z", "2024-02-04T10:39:00.0000000Z"],
                "datasets": [
                    {"label": "Total", "data": [20, 30]},
                    {"label": "Blocked", "data": [2, 5]}
                ]
            },
            "topClients": [{"name": "192.168.10.5", "domain": "server1.home", "hits": 463, "rateLimited": false},
                           {"name": "192.168.10.13", "hits": 165, "rateLimited": false}],
            "topDomains": [{"name": "example.com", "hits": 10}],
            "topBlockedDomains": [{"name": "ads.example", "hits": 4}]
        }})
    }

    #[tokio::test]
    async fn polls_stats_and_blocking_state() {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/dashboard/stats/get"))
            .and(query_param("type", "LastDay"))
            .and(header("authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(stats()))
            .mount(&s)
            .await;
        let till = (Utc::now() + Duration::minutes(4)).to_rfc3339();
        Mock::given(method("GET"))
            .and(path("/api/settings/get"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "ok", "response": {
                "version": "15.5", "enableBlocking": false, "temporaryDisableBlockingTill": till,
                "proxyPassword": "never-read"
            }})))
            .mount(&s)
            .await;
        let m = TechnitiumModule::new(&cfg(&s.uri())).unwrap();
        let out = m.poll().await.unwrap();
        let ModuleData::Technitium(d) = out.data else {
            panic!()
        };
        assert_eq!((d.total_queries, d.blocked, d.clients), (925, 49, 6));
        assert_eq!(d.chart.len(), 2);
        assert_eq!((d.chart[1].total, d.chart[1].blocked), (30, 5));
        assert_eq!(d.top_clients[0].domain.as_deref(), Some("server1.home"));
        assert_eq!(d.top_clients[1].domain, None);
        assert!(d.paused_until.is_some());
        assert_eq!(out.health, Health::Warning);
        assert_eq!(out.version.as_deref(), Some("15.5"));
        assert_eq!(d.headline().0, "5 % blocked");
    }

    #[tokio::test]
    async fn invalid_token_is_an_auth_error() {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"status": "invalid-token", "errorMessage": "Invalid token"}),
                ),
            )
            .mount(&s)
            .await;
        let m = TechnitiumModule::new(&cfg(&s.uri())).unwrap();
        let err = m.poll().await.unwrap_err().to_string();
        assert!(err.contains("authentication failed"), "{err}");
    }

    #[tokio::test]
    async fn pause_and_resume_call_the_settings_api() {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/settings/temporaryDisableBlocking"))
            .and(query_param("minutes", "30"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"status": "ok", "response": {
                "temporaryDisableBlockingTill": "2026-10-07T14:30:00Z"}})),
            )
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/settings/set"))
            .and(query_param("enableBlocking", "true"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"status": "ok", "response": {}})),
            )
            .expect(1)
            .mount(&s)
            .await;
        let m = TechnitiumModule::new(&cfg(&s.uri())).unwrap();
        let r = m
            .run_action("pause_blocking", json!({"minutes": 30}))
            .await
            .unwrap();
        assert_eq!(r.message, "Blocking paused for 30 min");
        assert!(matches!(
            m.run_action("pause_blocking", json!({"minutes": 7})).await,
            Err(ModuleError::InvalidArgs(_))
        ));
        m.run_action("resume_blocking", json!({})).await.unwrap();
    }
}
