//! Seerr (Overseerr/Jellyseerr API): pending requests with approve/decline, recent requests,
//! new requests from search.
//!
//! Checked against `seerr-api.yml` (seerr-team/seerr): `GET /api/v1/status`, `GET /request`,
//! `GET /request/count`, `POST /request/{id}/{approve|decline}`, `POST /request`, `GET /search`,
//! `GET /movie/{id}`, `GET /tv/{id}`. Request status: 1 pending, 2 approved, 3 declined.
//! Media status: 1 unknown, 2 pending, 3 processing, 4 partially available, 5 available, 6 deleted.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;

use super::http::{ServiceHttp, validate_http};
use super::media_demo::DEMO;
use super::{
    ActionDescriptor, ActionResult, ConnectionTest, Health, InstanceConfig, ModuleData,
    ModuleError, PollOutcome, ServiceKind, ServiceModule, describe_error,
};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MediaRequest {
    pub id: i64,
    /// `movie` or `tv`.
    pub media_type: String,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<i64>,
    pub title: String,
    pub year: Option<i64>,
    /// Poster URL (TMDB image CDN), when known.
    pub poster_url: Option<String>,
    pub requested_by: String,
    pub requested_at: Option<DateTime<Utc>>,
    /// `pending`, `approved` or `declined`.
    pub status: String,
    /// `unknown`, `pending`, `processing`, `partially_available`, `available`, `deleted`.
    pub media_status: String,
    pub seasons: Vec<i64>,
    pub is_4k: bool,
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct RequestCounts {
    pub total: u64,
    pub pending: u64,
    pub approved: u64,
    pub processing: u64,
    pub available: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SeerrData {
    pub counts: RequestCounts,
    pub pending: Vec<MediaRequest>,
    /// Latest 20 requests of any status.
    pub recent: Vec<MediaRequest>,
}

impl SeerrData {
    pub fn headline(&self) -> (String, Option<String>) {
        (
            format!("{} pending", self.counts.pending),
            Some(format!(
                "{} requests · {} processing",
                self.counts.total, self.counts.processing
            )),
        )
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchResult {
    pub media_type: String,
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub poster_url: Option<String>,
    pub overview: Option<String>,
    /// Media status if already known to Seerr.
    pub media_status: Option<String>,
}

/// Seerr's `MediaRequestStatus` (`server/constants/media.ts`).
pub fn request_status(n: i64) -> &'static str {
    match n {
        1 => "pending",
        2 => "approved",
        3 => "declined",
        // Sending the request to Sonarr/Radarr failed.
        4 => "failed",
        // Approved and since fulfilled.
        5 => "completed",
        _ => "unknown",
    }
}

/// Seerr's `MediaStatus`.
pub fn media_status(n: i64) -> &'static str {
    match n {
        2 => "pending",
        3 => "processing",
        4 => "partially_available",
        5 => "available",
        6 => "blocklisted",
        7 => "deleted",
        _ => "unknown",
    }
}

fn poster(path: Option<&str>) -> Option<String> {
    path.filter(|p| p.starts_with('/'))
        .map(|p| format!("https://image.tmdb.org/t/p/w342{p}"))
}

fn year(date: Option<&str>) -> Option<i64> {
    date.and_then(|d| d.get(..4)).and_then(|y| y.parse().ok())
}

#[derive(Clone)]
struct Details {
    title: String,
    year: Option<i64>,
    poster: Option<String>,
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["api_key"])
}

pub struct SeerrModule {
    http: Option<ServiceHttp>,
    titles: Mutex<HashMap<(String, i64), Details>>,
}

impl SeerrModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        let key = cfg.secret("api_key").unwrap_or_default().to_owned();
        Ok(Self {
            http: Some(ServiceHttp::new(cfg, &[("x-api-key", key)])?),
            titles: Mutex::new(HashMap::new()),
        })
    }

    pub fn demo() -> Self {
        Self {
            http: None,
            titles: Mutex::new(HashMap::new()),
        }
    }

    /// Title and poster for a request's media (cached; Seerr's request objects only carry ids).
    async fn details(&self, http: &ServiceHttp, media_type: &str, tmdb_id: i64) -> Option<Details> {
        let key = (media_type.to_owned(), tmdb_id);
        if let Some(d) = self.titles.lock().expect("lock").get(&key) {
            return Some(d.clone());
        }
        let path = if media_type == "tv" {
            format!("/api/v1/tv/{tmdb_id}")
        } else {
            format!("/api/v1/movie/{tmdb_id}")
        };
        let v: Value = http.get(&path, &[]).await.ok()?;
        let d = Details {
            title: v
                .get("title")
                .or_else(|| v.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("Unknown")
                .to_owned(),
            year: year(
                v.get("releaseDate")
                    .or_else(|| v.get("firstAirDate"))
                    .and_then(Value::as_str),
            ),
            poster: poster(v.get("posterPath").and_then(Value::as_str)),
        };
        self.titles.lock().expect("lock").insert(key, d.clone());
        Some(d)
    }

    async fn convert(&self, http: &ServiceHttp, r: &Value) -> MediaRequest {
        let media = r.get("media").cloned().unwrap_or(Value::Null);
        let media_type = r
            .get("type")
            .or_else(|| media.get("mediaType"))
            .and_then(Value::as_str)
            .unwrap_or("movie")
            .to_owned();
        let tmdb_id = media.get("tmdbId").and_then(Value::as_i64);
        let details = match tmdb_id {
            Some(id) => self.details(http, &media_type, id).await,
            None => None,
        };
        MediaRequest {
            id: r.get("id").and_then(Value::as_i64).unwrap_or_default(),
            tvdb_id: media.get("tvdbId").and_then(Value::as_i64),
            title: details
                .as_ref()
                .map(|d| d.title.clone())
                .unwrap_or_else(|| format!("TMDB {}", tmdb_id.unwrap_or(0))),
            year: details.as_ref().and_then(|d| d.year),
            poster_url: details.and_then(|d| d.poster),
            requested_by: r
                .get("requestedBy")
                .and_then(|u| {
                    u.get("displayName")
                        .or_else(|| u.get("username"))
                        .or_else(|| u.get("email"))
                })
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned(),
            requested_at: r
                .get("createdAt")
                .and_then(Value::as_str)
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc)),
            status: request_status(r.get("status").and_then(Value::as_i64).unwrap_or(0)).into(),
            media_status: media_status(media.get("status").and_then(Value::as_i64).unwrap_or(1))
                .into(),
            seasons: r
                .get("seasons")
                .and_then(Value::as_array)
                .map(|s| {
                    s.iter()
                        .filter_map(|x| x.get("seasonNumber").and_then(Value::as_i64))
                        .collect()
                })
                .unwrap_or_default(),
            is_4k: r.get("is4k").and_then(Value::as_bool).unwrap_or(false),
            media_type,
            tmdb_id,
        }
    }

    async fn fetch(&self) -> anyhow::Result<(SeerrData, Option<String>)> {
        let Some(http) = &self.http else {
            return Ok((DEMO.seerr(), Some("2.7.3".into())));
        };
        let pending_q = [
            ("filter", "pending".to_owned()),
            ("take", "50".to_owned()),
            ("sort", "added".to_owned()),
        ];
        let recent_q = [
            ("filter", "all".to_owned()),
            ("take", "20".to_owned()),
            ("sort", "added".to_owned()),
        ];
        let (status, counts, pending, recent) = tokio::try_join!(
            http.get::<Value>("/api/v1/status", &[]),
            http.get::<Value>("/api/v1/request/count", &[]),
            http.get::<Value>("/api/v1/request", &pending_q),
            http.get::<Value>("/api/v1/request", &recent_q),
        )?;
        let results = |v: &Value| {
            v.get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let mut p = Vec::new();
        for r in results(&pending) {
            p.push(self.convert(http, &r).await);
        }
        let mut all = Vec::new();
        for r in results(&recent) {
            all.push(self.convert(http, &r).await);
        }
        let c = |k: &str| counts.get(k).and_then(Value::as_u64).unwrap_or(0);
        Ok((
            SeerrData {
                counts: RequestCounts {
                    total: c("total"),
                    pending: c("pending"),
                    approved: c("approved"),
                    processing: c("processing"),
                    available: c("available"),
                },
                pending: p,
                recent: all,
            },
            status
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ))
    }
}

fn id_arg(args: &Value) -> Result<i64, ModuleError> {
    args.get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| ModuleError::InvalidArgs("id is required".into()))
}

#[async_trait]
impl ServiceModule for SeerrModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Seerr
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let (data, version) = self.fetch().await?;
        Ok(PollOutcome {
            health: Health::Healthy,
            health_reason: None,
            response_ms: Some(started.elapsed().as_millis() as u64),
            version,
            data: ModuleData::Seerr(Box::new(data)),
        })
    }

    fn actions(&self) -> Vec<ActionDescriptor> {
        vec![
            ActionDescriptor::new("approve", "Approve a request (args: id)", false),
            ActionDescriptor::new("decline", "Decline a request (args: id)", false),
            ActionDescriptor::new(
                "request",
                "Request media (args: media_type, tmdb_id, seasons)",
                false,
            ),
        ]
    }

    async fn run_action(&self, action: &str, args: Value) -> Result<ActionResult, ModuleError> {
        let Some(http) = &self.http else {
            return DEMO.seerr_action(action, &args);
        };
        let ok = |m: String| {
            Ok(ActionResult {
                ok: true,
                message: m,
                task: None,
            })
        };
        match action {
            "approve" | "decline" => {
                let id = id_arg(&args)?;
                http.call(Method::POST, &format!("/api/v1/request/{id}/{action}"), &[])
                    .await?;
                ok(format!("request {id} {action}d"))
            }
            "request" => {
                let media_type = args
                    .get("media_type")
                    .and_then(Value::as_str)
                    .unwrap_or("movie");
                if !matches!(media_type, "movie" | "tv") {
                    return Err(ModuleError::InvalidArgs("media_type: movie or tv".into()));
                }
                let tmdb = args
                    .get("tmdb_id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| ModuleError::InvalidArgs("tmdb_id is required".into()))?;
                let mut body = json!({"mediaType": media_type, "mediaId": tmdb});
                if media_type == "tv" {
                    body["seasons"] = args.get("seasons").cloned().unwrap_or(json!("all"));
                }
                http.post_json("/api/v1/request", &body).await?;
                ok("request created".into())
            }
            other => Err(ModuleError::UnknownAction(other.into())),
        }
    }

    fn queries(&self) -> Vec<&'static str> {
        vec!["search"]
    }

    async fn query(
        &self,
        name: &str,
        args: &HashMap<String, String>,
    ) -> Result<Value, ModuleError> {
        if name != "search" {
            return Err(ModuleError::UnknownAction(name.into()));
        }
        let q = args
            .get("q")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ModuleError::InvalidArgs("q is required".into()))?;
        let results: Vec<SearchResult> = match &self.http {
            None => DEMO.seerr_search(q),
            Some(http) => {
                let v: Value = http
                    .get(
                        "/api/v1/search",
                        &[("query", q.to_owned()), ("page", "1".into())],
                    )
                    .await?;
                v.get("results")
                    .and_then(Value::as_array)
                    .map(|rs| {
                        rs.iter()
                            .filter(|r| {
                                matches!(
                                    r.get("mediaType").and_then(Value::as_str),
                                    Some("movie" | "tv")
                                )
                            })
                            .take(20)
                            .map(|r| SearchResult {
                                media_type: r["mediaType"].as_str().unwrap_or_default().to_owned(),
                                tmdb_id: r.get("id").and_then(Value::as_i64).unwrap_or_default(),
                                title: r
                                    .get("title")
                                    .or_else(|| r.get("name"))
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                year: year(
                                    r.get("releaseDate")
                                        .or_else(|| r.get("firstAirDate"))
                                        .and_then(Value::as_str),
                                ),
                                poster_url: poster(r.get("posterPath").and_then(Value::as_str)),
                                overview: r
                                    .get("overview")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned),
                                media_status: r
                                    .pointer("/mediaInfo/status")
                                    .and_then(Value::as_i64)
                                    .map(|s| media_status(s).to_owned()),
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
        };
        Ok(serde_json::to_value(results).map_err(anyhow::Error::from)?)
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((d, version)) => ConnectionTest {
                ok: true,
                message: format!("{} requests · {} pending", d.counts.total, d.counts.pending),
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
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn pending_requests_get_titles_and_approve() {
        let s = MockServer::start().await;
        let get = |p: &str, body: Value| {
            Mock::given(method("GET"))
                .and(path(p.to_owned()))
                .and(header("x-api-key", "k"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
        };
        get("/api/v1/status", json!({"version": "2.7.3"}))
            .mount(&s)
            .await;
        get(
            "/api/v1/request/count",
            json!({"total": 14, "pending": 1, "processing": 2}),
        )
        .mount(&s)
        .await;
        let req = json!({"id": 21, "status": 1, "type": "movie", "createdAt": "2026-10-07T07:00:00.000Z",
            "media": {"tmdbId": 555, "status": 2}, "requestedBy": {"displayName": "leo"}});
        Mock::given(method("GET"))
            .and(path("/api/v1/request"))
            .and(query_param("filter", "pending"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"pageInfo": {}, "results": [req.clone()]})),
            )
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/request"))
            .and(query_param("filter", "all"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"pageInfo": {}, "results": [req]})),
            )
            .mount(&s)
            .await;
        get(
            "/api/v1/movie/555",
            json!({"title": "Static Bloom", "releaseDate": "2024-05-01", "posterPath": "/abc.jpg"}),
        )
        .expect(1)
        .mount(&s)
        .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/request/21/approve"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&s)
            .await;

        let cfg = InstanceConfig {
            id: "seerr".into(),
            kind: ServiceKind::Seerr,
            name: "Seerr".into(),
            base_url: Some(s.uri()),
            settings: json!({}),
            secrets: [("api_key".to_owned(), "k".to_owned())].into(),
        };
        let m = SeerrModule::new(&cfg).unwrap();
        let out = m.poll().await.unwrap();
        let ModuleData::Seerr(d) = out.data else {
            panic!()
        };
        let r = &d.pending[0];
        assert_eq!(
            (
                r.title.as_str(),
                r.year,
                r.status.as_str(),
                r.media_status.as_str()
            ),
            ("Static Bloom", Some(2024), "pending", "pending")
        );
        assert_eq!(
            r.poster_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w342/abc.jpg")
        );
        assert_eq!(d.headline().0, "1 pending");
        m.run_action("approve", json!({"id": 21})).await.unwrap();
        assert!(
            m.run_action("request", json!({"media_type": "game", "tmdb_id": 1}))
                .await
                .is_err()
        );
    }
}
