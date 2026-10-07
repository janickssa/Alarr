//! Prowlarr: indexers with status, response time and 24 h failures; test all indexers.
//!
//! Checked against `Prowlarr.Api.V1/openapi.json`: `GET /api/v1/indexer`, `/indexerstatus`,
//! `/indexerstats`, `/health`, `/system/status`, `POST /api/v1/indexer/testall`.

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use super::arr::HealthIssue;
use super::http::{ServiceHttp, validate_http};
use super::media_demo::DEMO;
use super::{
    ActionDescriptor, ActionResult, ConnectionTest, Health, InstanceConfig, ModuleData,
    ModuleError, PollOutcome, ServiceKind, ServiceModule, describe_error,
};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Indexer {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    /// `torrent` or `usenet`.
    pub protocol: String,
    /// `public`, `semiPrivate` or `private`.
    pub privacy: String,
    /// `ok`, `failing` or `disabled`.
    pub status: String,
    pub disabled_till: Option<DateTime<Utc>>,
    pub avg_response_ms: Option<u64>,
    pub queries_24h: u64,
    pub grabs_24h: u64,
    pub failures_24h: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProwlarrData {
    pub indexers: Vec<Indexer>,
    pub health: Vec<HealthIssue>,
}

impl ProwlarrData {
    pub fn failing(&self) -> usize {
        self.indexers
            .iter()
            .filter(|i| i.status == "failing")
            .count()
    }

    pub fn headline(&self) -> (String, Option<String>) {
        let enabled = self.indexers.iter().filter(|i| i.enabled).count();
        let f = self.failing();
        (
            format!("{enabled} indexer{}", if enabled == 1 { "" } else { "s" }),
            Some(if f > 0 {
                format!("{f} failing")
            } else {
                "all working".into()
            }),
        )
    }
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["api_key"])
}

pub struct ProwlarrModule {
    http: Option<ServiceHttp>,
}

impl ProwlarrModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        let key = cfg.secret("api_key").unwrap_or_default().to_owned();
        Ok(Self {
            http: Some(ServiceHttp::new(cfg, &[("x-api-key", key)])?),
        })
    }

    pub fn demo() -> Self {
        Self { http: None }
    }

    async fn fetch(&self) -> anyhow::Result<(ProwlarrData, Option<String>)> {
        let Some(http) = &self.http else {
            return Ok((DEMO.prowlarr(), Some("2.0.5".into())));
        };
        let now = Utc::now();
        let stats_q = [
            ("startDate", (now - Duration::hours(24)).to_rfc3339()),
            ("endDate", now.to_rfc3339()),
        ];
        let (status, indexers, statuses, stats, health) = tokio::try_join!(
            http.get::<Value>("/api/v1/system/status", &[]),
            http.get::<Vec<Value>>("/api/v1/indexer", &[]),
            http.get::<Vec<Value>>("/api/v1/indexerstatus", &[]),
            http.get::<Value>("/api/v1/indexerstats", &stats_q),
            http.get::<Vec<Value>>("/api/v1/health", &[]),
        )?;
        Ok((
            parse(&indexers, &statuses, &stats, &health, now),
            status
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ))
    }
}

pub fn parse(
    indexers: &[Value],
    statuses: &[Value],
    stats: &Value,
    health: &[Value],
    now: DateTime<Utc>,
) -> ProwlarrData {
    let disabled: HashMap<i64, DateTime<Utc>> = statuses
        .iter()
        .filter_map(|s| {
            let till = s.get("disabledTill")?.as_str()?;
            let till = DateTime::parse_from_rfc3339(till).ok()?.with_timezone(&Utc);
            Some((s.get("indexerId")?.as_i64()?, till))
        })
        .filter(|(_, till)| *till > now)
        .collect();
    let per: HashMap<i64, &Value> = stats
        .get("indexers")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| Some((s.get("indexerId")?.as_i64()?, s)))
                .collect()
        })
        .unwrap_or_default();
    let n = |v: Option<&&Value>, k: &str| {
        v.and_then(|s| s.get(k))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let mut out: Vec<Indexer> = indexers
        .iter()
        .map(|i| {
            let id = i.get("id").and_then(Value::as_i64).unwrap_or_default();
            let enabled = i.get("enable").and_then(Value::as_bool).unwrap_or(false);
            let st = per.get(&id);
            Indexer {
                id,
                name: i
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                enabled,
                protocol: i
                    .get("protocol")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                privacy: i
                    .get("privacy")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                status: if !enabled {
                    "disabled".into()
                } else if disabled.contains_key(&id) {
                    "failing".into()
                } else {
                    "ok".into()
                },
                disabled_till: disabled.get(&id).copied(),
                avg_response_ms: st
                    .and_then(|s| s.get("averageResponseTime"))
                    .and_then(Value::as_u64),
                queries_24h: n(st, "numberOfQueries"),
                grabs_24h: n(st, "numberOfGrabs"),
                failures_24h: n(st, "numberOfFailedQueries") + n(st, "numberOfFailedGrabs"),
            }
        })
        .collect();
    out.sort_by_key(|i| i.name.to_lowercase());
    ProwlarrData {
        indexers: out,
        health: health
            .iter()
            .map(|h| HealthIssue {
                source: h
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                level: h
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                message: h
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                wiki_url: h.get("wikiUrl").and_then(Value::as_str).map(str::to_owned),
            })
            .filter(|h| h.level != "ok")
            .collect(),
    }
}

#[async_trait]
impl ServiceModule for ProwlarrModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Prowlarr
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let (data, version) = self.fetch().await?;
        let failing = data.failing();
        let (health, reason) = if failing > 0 {
            (
                Health::Warning,
                Some(format!(
                    "{failing} indexer{} failing",
                    if failing == 1 { "" } else { "s" }
                )),
            )
        } else if let Some(h) = data.health.iter().find(|h| h.level != "notice") {
            (Health::Warning, Some(h.message.clone()))
        } else {
            (Health::Healthy, None)
        };
        Ok(PollOutcome {
            health,
            health_reason: reason,
            response_ms: Some(started.elapsed().as_millis() as u64),
            version,
            data: ModuleData::Prowlarr(Box::new(data)),
        })
    }

    fn actions(&self) -> Vec<ActionDescriptor> {
        vec![ActionDescriptor::new(
            "test_all",
            "Test all indexers",
            false,
        )]
    }

    async fn run_action(&self, action: &str, _args: Value) -> Result<ActionResult, ModuleError> {
        if action != "test_all" {
            return Err(ModuleError::UnknownAction(action.into()));
        }
        let Some(http) = &self.http else {
            return Ok(ActionResult {
                ok: true,
                message: "5 of 6 indexers passed".into(),
                task: None,
            });
        };
        let results = http
            .post_json("/api/v1/indexer/testall", &serde_json::json!({}))
            .await?;
        let list = results.as_array().cloned().unwrap_or_default();
        let passed = list
            .iter()
            .filter(|r| r.get("isValid").and_then(Value::as_bool).unwrap_or(false))
            .count();
        Ok(ActionResult {
            ok: passed == list.len(),
            message: format!("{passed} of {} indexers passed", list.len()),
            task: None,
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((d, version)) => ConnectionTest {
                ok: true,
                message: format!("{} indexers", d.indexers.len()),
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

    #[test]
    fn indexer_status_and_stats() {
        let now = Utc::now();
        let d = parse(
            &[
                json!({"id": 1, "name": "Nyx", "enable": true, "protocol": "torrent", "privacy": "private"}),
                json!({"id": 2, "name": "Atlas", "enable": true, "protocol": "torrent", "privacy": "public"}),
                json!({"id": 3, "name": "Off", "enable": false}),
            ],
            &[
                json!({"indexerId": 2, "disabledTill": (now + Duration::hours(1)).to_rfc3339()}),
                json!({"indexerId": 1, "disabledTill": (now - Duration::hours(1)).to_rfc3339()}),
            ],
            &json!({"indexers": [{"indexerId": 1, "averageResponseTime": 412, "numberOfQueries": 90, "numberOfGrabs": 4,
                                   "numberOfFailedQueries": 2, "numberOfFailedGrabs": 1}]}),
            &[json!({"source": "x", "type": "ok", "message": "fine"})],
            now,
        );
        let by = |n: &str| d.indexers.iter().find(|i| i.name == n).unwrap().clone();
        assert_eq!(by("Atlas").status, "failing");
        assert_eq!(by("Nyx").status, "ok");
        assert_eq!(by("Off").status, "disabled");
        assert_eq!(
            (by("Nyx").avg_response_ms, by("Nyx").failures_24h),
            (Some(412), 3)
        );
        assert!(d.health.is_empty());
        assert_eq!(
            d.headline(),
            ("2 indexers".into(), Some("1 failing".into()))
        );
    }
}
