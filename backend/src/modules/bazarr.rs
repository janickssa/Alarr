//! Bazarr: missing subtitle counts and lists, search (one or all), history.
//!
//! Checked against the Bazarr source (`bazarr/api`): `GET /api/badges`, `GET /api/system/status`,
//! `GET /api/episodes/wanted`, `GET /api/movies/wanted`, `GET /api/episodes/history`,
//! `GET /api/movies/history`, `PATCH /api/series?action=search-missing|search-wanted`,
//! `PATCH /api/movies?action=search-missing|search-wanted`. Auth header `X-API-KEY`.

use std::time::Instant;

use async_trait::async_trait;
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use super::http::{ServiceHttp, validate_http};
use super::media_demo::DEMO;
use super::{
    ActionDescriptor, ActionResult, ConnectionTest, Health, InstanceConfig, ModuleData,
    ModuleError, PollOutcome, ServiceKind, ServiceModule, describe_error,
};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WantedSubtitle {
    /// `episode` or `movie`.
    pub kind: String,
    pub title: String,
    /// `S01E03 · Episode title` for episodes.
    pub subtitle: Option<String>,
    /// Missing languages, e.g. `["Danish", "English (HI)"]`.
    pub languages: Vec<String>,
    /// Sonarr series id or Radarr movie id (for `search`).
    pub item_id: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SubtitleEvent {
    pub kind: String,
    pub title: String,
    pub description: String,
    pub language: Option<String>,
    pub provider: Option<String>,
    /// As Bazarr formats it (e.g. "2 hours ago").
    pub when: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BazarrData {
    pub missing_episodes: u64,
    pub missing_movies: u64,
    pub wanted: Vec<WantedSubtitle>,
    pub history: Vec<SubtitleEvent>,
}

impl BazarrData {
    pub fn headline(&self) -> (String, Option<String>) {
        (
            format!("{} missing", self.missing_episodes + self.missing_movies),
            Some(format!(
                "{} episodes · {} movies",
                self.missing_episodes, self.missing_movies
            )),
        )
    }
}

fn str_of(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn languages(v: &Value) -> Vec<String> {
    v.get("missing_subtitles")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|l| {
                    let name = str_of(l, "name")?;
                    let hi = l.get("hi").and_then(Value::as_bool).unwrap_or(false);
                    let forced = l.get("forced").and_then(Value::as_bool).unwrap_or(false);
                    Some(match (hi, forced) {
                        (true, _) => format!("{name} (HI)"),
                        (_, true) => format!("{name} (forced)"),
                        _ => name,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_wanted(episodes: &Value, movies: &Value) -> Vec<WantedSubtitle> {
    let data = |v: &Value| {
        v.get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let mut out: Vec<WantedSubtitle> = data(episodes)
        .iter()
        .map(|e| WantedSubtitle {
            kind: "episode".into(),
            title: str_of(e, "seriesTitle").unwrap_or_default(),
            subtitle: match (str_of(e, "episode_number"), str_of(e, "episodeTitle")) {
                (Some(n), Some(t)) => Some(format!("{n} · {t}")),
                (n, t) => n.or(t),
            },
            languages: languages(e),
            item_id: e
                .get("sonarrSeriesId")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        })
        .collect();
    out.extend(data(movies).iter().map(|m| {
        WantedSubtitle {
            kind: "movie".into(),
            title: str_of(m, "title").unwrap_or_default(),
            subtitle: None,
            languages: languages(m),
            item_id: m
                .get("radarrId")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        }
    }));
    out
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["api_key"])
}

pub struct BazarrModule {
    http: Option<ServiceHttp>,
}

impl BazarrModule {
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

    async fn fetch(&self) -> anyhow::Result<(BazarrData, Option<String>)> {
        let Some(http) = &self.http else {
            return Ok((DEMO.bazarr(), Some("1.5.3".into())));
        };
        let page = [("start", "0".to_owned()), ("length", "25".to_owned())];
        let short = [("start", "0".to_owned()), ("length", "10".to_owned())];
        let (status, badges, eps, movies, eh, mh) = tokio::try_join!(
            http.get::<Value>("/api/system/status", &[]),
            http.get::<Value>("/api/badges", &[]),
            http.get::<Value>("/api/episodes/wanted", &page),
            http.get::<Value>("/api/movies/wanted", &page),
            http.get::<Value>("/api/episodes/history", &short),
            http.get::<Value>("/api/movies/history", &short),
        )?;
        let history = |v: &Value, kind: &str| -> Vec<SubtitleEvent> {
            v.get("data")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|h| SubtitleEvent {
                            kind: kind.into(),
                            title: str_of(h, "seriesTitle")
                                .or_else(|| str_of(h, "title"))
                                .unwrap_or_default(),
                            description: str_of(h, "description").unwrap_or_default(),
                            language: h.get("language").and_then(|l| str_of(l, "name")),
                            provider: str_of(h, "provider"),
                            when: str_of(h, "timestamp"),
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut events = history(&eh, "episode");
        events.extend(history(&mh, "movie"));
        let version = status
            .get("data")
            .unwrap_or(&status)
            .get("bazarr_version")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok((
            BazarrData {
                missing_episodes: badges.get("episodes").and_then(Value::as_u64).unwrap_or(0),
                missing_movies: badges.get("movies").and_then(Value::as_u64).unwrap_or(0),
                wanted: parse_wanted(&eps, &movies),
                history: events,
            },
            version,
        ))
    }
}

#[async_trait]
impl ServiceModule for BazarrModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Bazarr
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let (data, version) = self.fetch().await?;
        Ok(PollOutcome {
            health: Health::Healthy,
            health_reason: None,
            response_ms: Some(started.elapsed().as_millis() as u64),
            version,
            data: ModuleData::Bazarr(Box::new(data)),
        })
    }

    fn actions(&self) -> Vec<ActionDescriptor> {
        vec![
            ActionDescriptor::new("search_all", "Search all missing subtitles", false),
            ActionDescriptor::new(
                "search",
                "Search one series or movie (args: kind, id)",
                false,
            ),
        ]
    }

    async fn run_action(&self, action: &str, args: Value) -> Result<ActionResult, ModuleError> {
        let calls: Vec<(&str, Vec<(&str, String)>)> = match action {
            "search_all" => vec![
                ("/api/series", vec![("action", "search-wanted".into())]),
                ("/api/movies", vec![("action", "search-wanted".into())]),
            ],
            "search" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| ModuleError::InvalidArgs("id is required".into()))?;
                match args.get("kind").and_then(Value::as_str) {
                    Some("episode" | "series") => vec![(
                        "/api/series",
                        vec![
                            ("action", "search-missing".into()),
                            ("seriesid", id.to_string()),
                        ],
                    )],
                    Some("movie") => vec![(
                        "/api/movies",
                        vec![
                            ("action", "search-missing".into()),
                            ("radarrid", id.to_string()),
                        ],
                    )],
                    _ => return Err(ModuleError::InvalidArgs("kind: episode or movie".into())),
                }
            }
            other => return Err(ModuleError::UnknownAction(other.into())),
        };
        if let Some(http) = &self.http {
            for (path, q) in calls {
                http.call(Method::PATCH, path, &q).await?;
            }
        }
        Ok(ActionResult {
            ok: true,
            message: "subtitle search started".into(),
            task: None,
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((d, version)) => ConnectionTest {
                ok: true,
                message: format!(
                    "{} missing subtitles",
                    d.missing_episodes + d.missing_movies
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
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn wanted_languages() {
        let w = parse_wanted(
            &json!({"data": [{"seriesTitle": "Saltwater Kings", "episode_number": "1x03", "episodeTitle": "Low Tide",
                "missing_subtitles": [{"name": "Danish", "hi": false, "forced": false}, {"name": "English", "hi": true}],
                "sonarrSeriesId": 3}], "total": 1}),
            &json!({"data": [{"title": "Harbor Lights", "missing_subtitles": [{"name": "Danish"}], "radarrId": 9}]}),
        );
        assert_eq!(w[0].languages, vec!["Danish", "English (HI)"]);
        assert_eq!(w[0].subtitle.as_deref(), Some("1x03 · Low Tide"));
        assert_eq!((w[1].kind.as_str(), w[1].item_id), ("movie", 9));
    }

    #[tokio::test]
    async fn search_series_uses_patch() {
        let s = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/series"))
            .and(header("x-api-key", "k"))
            .and(query_param("action", "search-missing"))
            .and(query_param("seriesid", "3"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        let cfg = InstanceConfig {
            id: "bazarr".into(),
            kind: ServiceKind::Bazarr,
            name: "Bazarr".into(),
            base_url: Some(s.uri()),
            settings: json!({}),
            secrets: [("api_key".to_owned(), "k".to_owned())].into(),
        };
        let m = BazarrModule::new(&cfg).unwrap();
        m.run_action("search", json!({"kind": "episode", "id": 3}))
            .await
            .unwrap();
    }
}
