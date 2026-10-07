//! Sonarr and Radarr (shared code; the v3 APIs have the same shape).
//!
//! Checked against the committed OpenAPI specs (`Sonarr.Api.V3/openapi.json`,
//! `Radarr.Api.V3/openapi.json`): queue, calendar, wanted/missing, health, diskspace,
//! system/status, series/movie and command.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use super::http::{ServiceHttp, fmt_bytes, validate_http};
use super::media_demo::DEMO;
use super::{
    ActionDescriptor, ActionResult, ConnectionTest, Health, InstanceConfig, ModuleData,
    ModuleError, PollOutcome, ServiceKind, ServiceModule, describe_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArrFlavor {
    Sonarr,
    Radarr,
}

impl ArrFlavor {
    fn item(self) -> &'static str {
        match self {
            Self::Sonarr => "series",
            Self::Radarr => "movie",
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QueueItem {
    pub id: i64,
    /// Series or movie title.
    pub title: String,
    /// Episode label (`S02E05 · Title`) for Sonarr.
    pub subtitle: Option<String>,
    /// Release name.
    pub release: String,
    /// Series or movie id in Sonarr/Radarr.
    pub item_id: Option<i64>,
    pub episode_id: Option<i64>,
    pub size: u64,
    pub size_left: u64,
    /// `downloading`, `queued`, `paused`, `completed`, `delay`, `warning`, `failed`, …
    pub status: String,
    /// `ok`, `warning` or `error`.
    pub tracked_status: Option<String>,
    /// `downloading`, `importPending`, `importing`, `imported`, `failedPending`, …
    pub tracked_state: Option<String>,
    /// Warning or error text (first status message or error message).
    pub message: Option<String>,
    /// Download client id; the torrent hash for qBittorrent.
    pub download_id: Option<String>,
    pub download_client: Option<String>,
    pub protocol: Option<String>,
    pub eta: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CalendarItem {
    pub id: i64,
    pub title: String,
    pub subtitle: Option<String>,
    pub at: DateTime<Utc>,
    /// Radarr: `cinema`, `digital` or `physical`; Sonarr: `airing`.
    pub kind: String,
    pub has_file: bool,
    pub item_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MissingItem {
    /// Episode id (Sonarr) or movie id (Radarr): used by the `search_item` action.
    pub id: i64,
    pub title: String,
    pub subtitle: Option<String>,
    pub at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HealthIssue {
    pub source: String,
    /// `notice`, `warning` or `error`.
    pub level: String,
    pub message: String,
    pub wiki_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Disk {
    pub path: String,
    pub free: u64,
    pub total: u64,
}

/// One series or movie (library grid, search, request tracker matching).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LibraryItem {
    pub id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub monitored: bool,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<i64>,
    /// Movies: file present. Series: all aired episodes present.
    pub complete: bool,
    /// Series: episodes with a file / episodes aired. Movies: 0/1.
    pub have: i64,
    pub total: i64,
    pub size: u64,
    /// Poster path on the service (`/MediaCover/…`), proxied by `/services/{id}/image`.
    pub poster: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ArrData {
    pub flavor: ArrFlavor,
    pub queue: Vec<QueueItem>,
    /// From yesterday to 14 days ahead.
    pub calendar: Vec<CalendarItem>,
    /// Newest 20 wanted (missing) items.
    pub missing: Vec<MissingItem>,
    pub missing_total: i64,
    pub health: Vec<HealthIssue>,
    pub disks: Vec<Disk>,
    pub library_count: u32,
    pub library_size: u64,
}

impl ArrData {
    pub fn headline(&self) -> (String, Option<String>) {
        let detail = match self.health.first() {
            Some(h) => h.message.clone(),
            None if self.missing_total > 0 => format!("{} missing", self.missing_total),
            None => format!(
                "{} {}",
                self.library_count,
                match (self.flavor, self.library_count) {
                    (ArrFlavor::Sonarr, _) => "series",
                    (ArrFlavor::Radarr, 1) => "movie",
                    (ArrFlavor::Radarr, _) => "movies",
                }
            ),
        };
        (format!("{} in queue", self.queue.len()), Some(detail))
    }
}

pub fn health_of(issues: &[HealthIssue], queue: &[QueueItem]) -> (Health, Option<String>) {
    let serious: Vec<&HealthIssue> = issues.iter().filter(|h| h.level != "notice").collect();
    if let Some(first) = serious.first() {
        let more = if serious.len() > 1 {
            format!(" · {} more", serious.len() - 1)
        } else {
            String::new()
        };
        return (Health::Warning, Some(format!("{}{more}", first.message)));
    }
    let stuck = queue
        .iter()
        .filter(|q| matches!(q.tracked_status.as_deref(), Some("warning" | "error")))
        .count();
    if stuck > 0 {
        return (
            Health::Warning,
            Some(format!(
                "{stuck} queue item{} need attention",
                if stuck == 1 { "" } else { "s" }
            )),
        );
    }
    (Health::Healthy, None)
}

fn ep_label(season: Option<i64>, episode: Option<i64>, title: Option<&str>) -> Option<String> {
    let code = format!("S{:02}E{:02}", season?, episode?);
    Some(match title.filter(|t| !t.is_empty() && *t != "TBA") {
        Some(t) => format!("{code} · {t}"),
        None => code,
    })
}

fn parse_dt(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_owned)
}

fn i(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(Value::as_i64)
}

fn u(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0).max(0.0) as u64
}

fn poster(v: &Value) -> Option<String> {
    v.get("images")?
        .as_array()?
        .iter()
        .find(|img| img.get("coverType").and_then(Value::as_str) == Some("poster"))
        .and_then(|img| s(img, "url"))
        .map(|url| url.split('?').next().unwrap_or(&url).to_owned())
        .filter(|url| url.starts_with("/MediaCover/"))
}

pub fn parse_queue(flavor: ArrFlavor, records: &[Value]) -> Vec<QueueItem> {
    records
        .iter()
        .map(|r| {
            let (title, subtitle, item_id) = match flavor {
                ArrFlavor::Sonarr => {
                    let ep = r.get("episode").cloned().unwrap_or(Value::Null);
                    (
                        r.get("series")
                            .and_then(|x| s(x, "title"))
                            .unwrap_or_else(|| s(r, "title").unwrap_or_default()),
                        ep_label(
                            i(&ep, "seasonNumber"),
                            i(&ep, "episodeNumber"),
                            ep.get("title").and_then(Value::as_str),
                        ),
                        i(r, "seriesId"),
                    )
                }
                ArrFlavor::Radarr => (
                    r.get("movie")
                        .and_then(|x| s(x, "title"))
                        .unwrap_or_else(|| s(r, "title").unwrap_or_default()),
                    None,
                    i(r, "movieId"),
                ),
            };
            let message = r
                .get("statusMessages")
                .and_then(Value::as_array)
                .and_then(|msgs| {
                    msgs.iter()
                        .flat_map(|m| {
                            m.get("messages")
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default()
                        })
                        .filter_map(|m| m.as_str().map(str::to_owned))
                        .next()
                })
                .or_else(|| s(r, "errorMessage"));
            QueueItem {
                id: i(r, "id").unwrap_or_default(),
                title,
                subtitle,
                release: s(r, "title").unwrap_or_default(),
                item_id,
                episode_id: i(r, "episodeId"),
                size: u(r, "size"),
                size_left: u(r, "sizeleft"),
                status: s(r, "status").unwrap_or_default(),
                tracked_status: s(r, "trackedDownloadStatus"),
                tracked_state: s(r, "trackedDownloadState"),
                message,
                download_id: s(r, "downloadId"),
                download_client: s(r, "downloadClient"),
                protocol: s(r, "protocol"),
                eta: r.get("estimatedCompletionTime").and_then(parse_dt),
            }
        })
        .collect()
}

pub fn parse_calendar(
    flavor: ArrFlavor,
    items: &[Value],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Vec<CalendarItem> {
    let mut out = Vec::new();
    for v in items {
        match flavor {
            ArrFlavor::Sonarr => {
                if let Some(at) = v.get("airDateUtc").and_then(parse_dt) {
                    out.push(CalendarItem {
                        id: i(v, "id").unwrap_or_default(),
                        title: v
                            .get("series")
                            .and_then(|x| s(x, "title"))
                            .unwrap_or_default(),
                        subtitle: ep_label(
                            i(v, "seasonNumber"),
                            i(v, "episodeNumber"),
                            v.get("title").and_then(Value::as_str),
                        ),
                        at,
                        kind: "airing".into(),
                        has_file: v.get("hasFile").and_then(Value::as_bool).unwrap_or(false),
                        item_id: i(v, "seriesId"),
                    });
                }
            }
            ArrFlavor::Radarr => {
                for (key, kind) in [
                    ("inCinemas", "cinema"),
                    ("digitalRelease", "digital"),
                    ("physicalRelease", "physical"),
                ] {
                    if let Some(at) = v
                        .get(key)
                        .and_then(parse_dt)
                        .filter(|at| *at >= from && *at <= to)
                    {
                        out.push(CalendarItem {
                            id: i(v, "id").unwrap_or_default(),
                            title: s(v, "title").unwrap_or_default(),
                            subtitle: i(v, "year").map(|y| y.to_string()),
                            at,
                            kind: kind.into(),
                            has_file: v.get("hasFile").and_then(Value::as_bool).unwrap_or(false),
                            item_id: i(v, "id"),
                        });
                    }
                }
            }
        }
    }
    out.sort_by_key(|c| c.at);
    out
}

pub fn parse_missing(flavor: ArrFlavor, records: &[Value]) -> Vec<MissingItem> {
    records
        .iter()
        .map(|r| match flavor {
            ArrFlavor::Sonarr => MissingItem {
                id: i(r, "id").unwrap_or_default(),
                title: r
                    .get("series")
                    .and_then(|x| s(x, "title"))
                    .unwrap_or_default(),
                subtitle: ep_label(
                    i(r, "seasonNumber"),
                    i(r, "episodeNumber"),
                    r.get("title").and_then(Value::as_str),
                ),
                at: r.get("airDateUtc").and_then(parse_dt),
            },
            ArrFlavor::Radarr => MissingItem {
                id: i(r, "id").unwrap_or_default(),
                title: s(r, "title").unwrap_or_default(),
                subtitle: i(r, "year").map(|y| y.to_string()),
                at: r
                    .get("digitalRelease")
                    .and_then(parse_dt)
                    .or_else(|| r.get("inCinemas").and_then(parse_dt)),
            },
        })
        .collect()
}

pub fn parse_library(flavor: ArrFlavor, items: &[Value]) -> Vec<LibraryItem> {
    let mut out: Vec<LibraryItem> = items
        .iter()
        .map(|v| {
            let stats = v.get("statistics").cloned().unwrap_or(Value::Null);
            let (have, total, complete, size) = match flavor {
                ArrFlavor::Sonarr => {
                    let have = i(&stats, "episodeFileCount").unwrap_or(0);
                    let total = i(&stats, "episodeCount").unwrap_or(0);
                    (
                        have,
                        total,
                        total > 0 && have >= total,
                        u(&stats, "sizeOnDisk"),
                    )
                }
                ArrFlavor::Radarr => {
                    let has = v.get("hasFile").and_then(Value::as_bool).unwrap_or(false);
                    (has as i64, 1, has, u(v, "sizeOnDisk"))
                }
            };
            LibraryItem {
                id: i(v, "id").unwrap_or_default(),
                title: s(v, "title").unwrap_or_default(),
                year: i(v, "year").filter(|y| *y > 0),
                monitored: v.get("monitored").and_then(Value::as_bool).unwrap_or(false),
                tmdb_id: i(v, "tmdbId").filter(|x| *x > 0),
                tvdb_id: i(v, "tvdbId").filter(|x| *x > 0),
                complete,
                have,
                total,
                size,
                poster: poster(v),
                status: s(v, "status"),
            }
        })
        .collect();
    out.sort_by_key(|l| l.title.to_lowercase());
    out
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["api_key"])
}

pub struct ArrModule {
    flavor: ArrFlavor,
    http: Option<ServiceHttp>,
    library: Mutex<Arc<Vec<LibraryItem>>>,
}

impl ArrModule {
    pub fn new(cfg: &InstanceConfig, flavor: ArrFlavor) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        let key = cfg.secret("api_key").unwrap_or_default().to_owned();
        Ok(Self {
            flavor,
            http: Some(ServiceHttp::new(cfg, &[("x-api-key", key)])?),
            library: Mutex::new(Arc::default()),
        })
    }

    pub fn demo(flavor: ArrFlavor) -> Self {
        Self {
            flavor,
            http: None,
            library: Mutex::new(Arc::default()),
        }
    }

    pub fn flavor(&self) -> ArrFlavor {
        self.flavor
    }

    /// The library from the last poll.
    pub fn library(&self) -> Arc<Vec<LibraryItem>> {
        self.library.lock().expect("lock").clone()
    }

    async fn fetch(&self) -> anyhow::Result<(ArrData, String)> {
        let Some(http) = &self.http else {
            let (data, library) = DEMO.arr(self.flavor);
            *self.library.lock().expect("lock") = Arc::new(library);
            return Ok((
                data,
                if self.flavor == ArrFlavor::Sonarr {
                    "4.0.15"
                } else {
                    "5.26.2"
                }
                .into(),
            ));
        };
        let now = Utc::now();
        let (from, to) = (now - Duration::days(1), now + Duration::days(14));
        let item = self.flavor.item();
        let mut queue_q = vec![("page", "1".to_owned()), ("pageSize", "100".to_owned())];
        let mut cal_q = vec![("start", from.to_rfc3339()), ("end", to.to_rfc3339())];
        let mut missing_q = vec![
            ("page", "1".to_owned()),
            ("pageSize", "20".to_owned()),
            ("monitored", "true".to_owned()),
        ];
        match self.flavor {
            ArrFlavor::Sonarr => {
                queue_q.extend([
                    ("includeSeries", "true".into()),
                    ("includeEpisode", "true".into()),
                ]);
                cal_q.push(("includeSeries", "true".into()));
                missing_q.extend([
                    ("includeSeries", "true".into()),
                    ("sortKey", "episodes.airDateUtc".into()),
                    ("sortDirection", "descending".into()),
                ]);
            }
            ArrFlavor::Radarr => queue_q.push(("includeMovie", "true".into())),
        }
        let library_path = format!("/api/v3/{item}");
        let (status, queue, calendar, missing, health, disks, library) = tokio::try_join!(
            http.get::<Value>("/api/v3/system/status", &[]),
            http.get::<Value>("/api/v3/queue", &queue_q),
            http.get::<Vec<Value>>("/api/v3/calendar", &cal_q),
            http.get::<Value>("/api/v3/wanted/missing", &missing_q),
            http.get::<Vec<Value>>("/api/v3/health", &[]),
            http.get::<Vec<Value>>("/api/v3/diskspace", &[]),
            http.get::<Vec<Value>>(&library_path, &[]),
        )?;
        let records = |v: &Value| {
            v.get("records")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let library = parse_library(self.flavor, &library);
        let data = ArrData {
            flavor: self.flavor,
            queue: parse_queue(self.flavor, &records(&queue)),
            calendar: parse_calendar(self.flavor, &calendar, from, to),
            missing: parse_missing(self.flavor, &records(&missing)),
            missing_total: i(&missing, "totalRecords").unwrap_or(0),
            health: health
                .iter()
                .map(|h| HealthIssue {
                    source: s(h, "source").unwrap_or_default(),
                    level: s(h, "type").unwrap_or_default(),
                    message: s(h, "message").unwrap_or_default(),
                    wiki_url: s(h, "wikiUrl"),
                })
                .filter(|h| h.level != "ok")
                .collect(),
            disks: disks
                .iter()
                .map(|d| Disk {
                    path: s(d, "path").unwrap_or_default(),
                    free: u(d, "freeSpace"),
                    total: u(d, "totalSpace"),
                })
                .collect(),
            library_count: library.len() as u32,
            library_size: library.iter().map(|l| l.size).sum(),
        };
        *self.library.lock().expect("lock") = Arc::new(library);
        Ok((data, s(&status, "version").unwrap_or_default()))
    }

    async fn command(&self, body: Value) -> Result<ActionResult, ModuleError> {
        let name = body["name"].as_str().unwrap_or_default().to_owned();
        match &self.http {
            Some(http) => {
                http.post_json("/api/v3/command", &body).await?;
            }
            None => DEMO.note(&format!("{name} queued")),
        }
        Ok(ActionResult {
            ok: true,
            message: format!("{name} queued"),
            task: None,
        })
    }
}

#[async_trait]
impl ServiceModule for ArrModule {
    fn kind(&self) -> ServiceKind {
        match self.flavor {
            ArrFlavor::Sonarr => ServiceKind::Sonarr,
            ArrFlavor::Radarr => ServiceKind::Radarr,
        }
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let (data, version) = self.fetch().await?;
        let (health, health_reason) = health_of(&data.health, &data.queue);
        Ok(PollOutcome {
            health,
            health_reason,
            response_ms: Some(started.elapsed().as_millis() as u64),
            version: Some(version).filter(|v| !v.is_empty()),
            data: ModuleData::Arr(Box::new(data)),
        })
    }

    fn actions(&self) -> Vec<ActionDescriptor> {
        vec![
            ActionDescriptor::new("rescan", "Rescan disk", false),
            ActionDescriptor::new("refresh", "Refresh metadata", false),
            ActionDescriptor::new("rss_sync", "RSS sync", false),
            ActionDescriptor::new("search_missing", "Search all missing", false),
            ActionDescriptor::new("search_item", "Search one missing item (args: id)", false),
        ]
    }

    async fn run_action(&self, action: &str, args: Value) -> Result<ActionResult, ModuleError> {
        let sonarr = self.flavor == ArrFlavor::Sonarr;
        let body = match action {
            "rescan" => json!({"name": if sonarr { "RescanSeries" } else { "RescanMovie" }}),
            "refresh" => json!({"name": if sonarr { "RefreshSeries" } else { "RefreshMovie" }}),
            "rss_sync" => json!({"name": "RssSync"}),
            "search_missing" => {
                json!({"name": if sonarr { "MissingEpisodeSearch" } else { "MissingMoviesSearch" }})
            }
            "search_item" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| ModuleError::InvalidArgs("id is required".into()))?;
                if sonarr {
                    json!({"name": "EpisodeSearch", "episodeIds": [id]})
                } else {
                    json!({"name": "MoviesSearch", "movieIds": [id]})
                }
            }
            other => return Err(ModuleError::UnknownAction(other.into())),
        };
        self.command(body).await
    }

    fn queries(&self) -> Vec<&'static str> {
        vec!["library"]
    }

    async fn query(
        &self,
        name: &str,
        _args: &HashMap<String, String>,
    ) -> Result<Value, ModuleError> {
        match name {
            "library" => {
                if self.library().is_empty() {
                    self.fetch().await?;
                }
                Ok(serde_json::to_value(&*self.library()).map_err(anyhow::Error::from)?)
            }
            other => Err(ModuleError::UnknownAction(other.into())),
        }
    }

    async fn image(&self, path: &str) -> Result<(bytes::Bytes, String), ModuleError> {
        if !path.starts_with("/MediaCover/") || path.contains("..") {
            return Err(ModuleError::InvalidArgs("not a MediaCover path".into()));
        }
        match &self.http {
            Some(http) => Ok(http.bytes(path).await?),
            None => Err(ModuleError::UnknownAction("no images in demo mode".into())),
        }
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((d, version)) => ConnectionTest {
                ok: true,
                message: format!(
                    "{} {} · {} in queue · {} free",
                    d.library_count,
                    if self.flavor == ArrFlavor::Sonarr {
                        "series"
                    } else {
                        "movies"
                    },
                    d.queue.len(),
                    fmt_bytes(d.disks.iter().map(|x| x.free).max().unwrap_or(0))
                ),
                version: Some(version),
                ..Default::default()
            },
            Err(e) => ConnectionTest {
                ok: false,
                message: describe_error(&e),
                ..Default::default()
            },
        }
    }

    fn as_arr(&self) -> Option<&ArrModule> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(uri: &str, kind: ServiceKind) -> InstanceConfig {
        InstanceConfig {
            id: "sonarr".into(),
            kind,
            name: "Sonarr".into(),
            base_url: Some(uri.into()),
            settings: json!({}),
            secrets: [("api_key".to_owned(), "k3y".to_owned())].into(),
        }
    }

    async fn mock(s: &MockServer, p: &str, body: Value) {
        Mock::given(method("GET"))
            .and(path(p))
            .and(header("x-api-key", "k3y"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(s)
            .await;
    }

    #[tokio::test]
    async fn sonarr_poll() {
        let s = MockServer::start().await;
        let soon = (Utc::now() + Duration::days(2)).to_rfc3339();
        mock(
            &s,
            "/api/v3/system/status",
            json!({"version": "4.0.15.2941"}),
        )
        .await;
        mock(&s, "/api/v3/queue", json!({"totalRecords": 1, "records": [{
            "id": 7, "seriesId": 3, "episodeId": 55, "title": "Saltwater.Kings.S01E03.1080p", "size": 100.0, "sizeleft": 40.0,
            "status": "downloading", "trackedDownloadStatus": "warning", "trackedDownloadState": "downloading",
            "statusMessages": [{"title": "x", "messages": ["No seeders"]}], "downloadId": "ABC123", "downloadClient": "qBittorrent",
            "series": {"title": "Saltwater Kings"}, "episode": {"seasonNumber": 1, "episodeNumber": 3, "title": "Low Tide"}
        }]})).await;
        mock(
            &s,
            "/api/v3/calendar",
            json!([{"id": 9, "seriesId": 3, "seasonNumber": 1, "episodeNumber": 4, "title": "TBA",
            "airDateUtc": soon, "hasFile": false, "series": {"title": "Saltwater Kings"}}]),
        )
        .await;
        mock(&s, "/api/v3/wanted/missing", json!({"totalRecords": 12, "records": [{"id": 55, "seasonNumber": 1, "episodeNumber": 3,
            "title": "Low Tide", "airDateUtc": "2026-09-01T20:00:00Z", "series": {"title": "Saltwater Kings"}}]})).await;
        mock(&s, "/api/v3/health", json!([{"source": "IndexerStatusCheck", "type": "warning",
            "message": "Indexers unavailable due to failures for more than 6 hours: Prowlarr", "wikiUrl": "https://wiki"}])).await;
        mock(&s, "/api/v3/diskspace", json!([{"path": "/media", "freeSpace": 1979120929996u64, "totalSpace": 3958241859992u64}])).await;
        mock(&s, "/api/v3/series", json!([{"id": 3, "title": "Saltwater Kings", "year": 2026, "monitored": true, "tvdbId": 4242,
            "images": [{"coverType": "poster", "url": "/MediaCover/3/poster.jpg?lastWrite=1"}],
            "statistics": {"episodeFileCount": 2, "episodeCount": 3, "sizeOnDisk": 3000}}])).await;

        let m = ArrModule::new(&cfg(&s.uri(), ServiceKind::Sonarr), ArrFlavor::Sonarr).unwrap();
        let out = m.poll().await.unwrap();
        let ModuleData::Arr(d) = out.data else {
            panic!()
        };
        assert_eq!(out.health, Health::Warning);
        assert!(out.health_reason.unwrap().contains("Prowlarr"));
        assert_eq!(d.queue[0].subtitle.as_deref(), Some("S01E03 · Low Tide"));
        assert_eq!(d.queue[0].message.as_deref(), Some("No seeders"));
        assert_eq!(d.queue[0].download_id.as_deref(), Some("ABC123"));
        assert_eq!(d.calendar[0].subtitle.as_deref(), Some("S01E04"));
        assert_eq!(d.missing_total, 12);
        assert_eq!(d.headline().0, "1 in queue");
        let lib = m.library();
        assert_eq!(lib[0].poster.as_deref(), Some("/MediaCover/3/poster.jpg"));
        assert_eq!((lib[0].have, lib[0].total, lib[0].complete), (2, 3, false));
        assert!(m.image("/etc/passwd").await.is_err());
    }

    #[tokio::test]
    async fn radarr_commands() {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v3/command"))
            .and(header("x-api-key", "k3y"))
            .and(body_partial_json(
                json!({"name": "MoviesSearch", "movieIds": [12]}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": 1})))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v3/command"))
            .and(body_partial_json(json!({"name": "MissingMoviesSearch"})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": 2})))
            .expect(1)
            .mount(&s)
            .await;
        let m = ArrModule::new(&cfg(&s.uri(), ServiceKind::Radarr), ArrFlavor::Radarr).unwrap();
        m.run_action("search_item", json!({"id": 12}))
            .await
            .unwrap();
        m.run_action("search_missing", json!({})).await.unwrap();
        assert!(m.run_action("search_item", json!({})).await.is_err());
    }

    #[test]
    fn radarr_calendar_uses_release_dates_in_window() {
        let now = Utc::now();
        let items = vec![json!({"id": 1, "title": "Glass Meridian", "year": 2025,
            "inCinemas": "2020-01-01T00:00:00Z", "digitalRelease": (now + Duration::days(3)).to_rfc3339()})];
        let cal = parse_calendar(
            ArrFlavor::Radarr,
            &items,
            now - Duration::days(1),
            now + Duration::days(14),
        );
        assert_eq!(cal.len(), 1);
        assert_eq!(cal[0].kind, "digital");
    }
}
