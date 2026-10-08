//! qBittorrent: transfer speeds, torrents, stop/start, delete, alternative speed limits.
//!
//! WebAPI checked against the qBittorrent wiki ("WebUI API (qBittorrent 5.0)"). Since WebAPI
//! 2.11.0 (qBittorrent 5) `pause`/`resume` are `stop`/`start` and paused states are
//! `stoppedUP`/`stoppedDL`; the module detects the version and uses the right names.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use super::http::{ServiceHttp, fmt_bytes, fmt_rate, validate_http};
use super::media_demo::DEMO;
use super::{
    ActionDescriptor, ActionResult, ConnectionTest, Health, InstanceConfig, ModuleData,
    ModuleError, PollOutcome, ServiceKind, ServiceModule, describe_error,
};

/// Ten minutes of speed history for the mini chart.
const HISTORY_SECS: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TorrentState {
    Downloading,
    Seeding,
    Stopped,
    Queued,
    Stalled,
    Checking,
    Metadata,
    Errored,
    Other,
}

impl TorrentState {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "downloading" | "forcedDL" => Self::Downloading,
            "uploading" | "forcedUP" | "stalledUP" => Self::Seeding,
            "pausedUP" | "pausedDL" | "stoppedUP" | "stoppedDL" => Self::Stopped,
            "queuedUP" | "queuedDL" => Self::Queued,
            "stalledDL" => Self::Stalled,
            "checkingUP" | "checkingDL" | "checkingResumeData" | "allocating" | "moving" => {
                Self::Checking
            }
            "metaDL" | "forcedMetaDL" => Self::Metadata,
            "error" | "missingFiles" => Self::Errored,
            _ => Self::Other,
        }
    }

    pub fn is_downloading(self) -> bool {
        matches!(self, Self::Downloading | Self::Stalled | Self::Metadata)
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Torrent {
    pub hash: String,
    pub name: String,
    pub state: TorrentState,
    /// qBittorrent's own state name.
    pub raw_state: String,
    /// 0.0–1.0.
    pub progress: f64,
    /// Bytes.
    pub size: u64,
    pub downloaded: u64,
    /// Bytes per second.
    pub dl_speed: u64,
    pub up_speed: u64,
    /// Seconds; `None` when unknown or done.
    pub eta_s: Option<u64>,
    pub ratio: f64,
    pub category: String,
    pub tags: Vec<String>,
    pub added_at: Option<DateTime<Utc>>,
    pub num_seeds: u64,
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct TorrentCounts {
    pub all: u32,
    pub downloading: u32,
    pub seeding: u32,
    pub stopped: u32,
    pub errored: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpeedPoint {
    pub at: DateTime<Utc>,
    pub dl: u64,
    pub up: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QbittorrentData {
    /// WebAPI version, e.g. `2.11.3`.
    pub api_version: String,
    pub dl_speed: u64,
    pub up_speed: u64,
    /// `connected`, `firewalled` or `disconnected`.
    pub connection: String,
    /// Alternative ("turtle") speed limits on.
    pub alt_speed: bool,
    /// Free space in the default save path, bytes.
    pub free_space: Option<u64>,
    pub counts: TorrentCounts,
    pub torrents: Vec<Torrent>,
    /// Last 10 minutes of transfer speeds.
    pub history: Vec<SpeedPoint>,
}

impl QbittorrentData {
    pub fn headline(&self) -> (String, Option<String>) {
        let downloading = self.counts.downloading;
        (
            if downloading > 0 {
                format!("{downloading} downloading")
            } else {
                format!("{} seeding", self.counts.seeding)
            },
            Some(format!(
                "↓ {} · ↑ {}{}",
                fmt_rate(self.dl_speed),
                fmt_rate(self.up_speed),
                self.free_space
                    .map(|f| format!(" · {} free", fmt_bytes(f)))
                    .unwrap_or_default()
            )),
        )
    }
}

pub fn count(torrents: &[Torrent]) -> TorrentCounts {
    let n = |f: &dyn Fn(&Torrent) -> bool| torrents.iter().filter(|t| f(t)).count() as u32;
    TorrentCounts {
        all: torrents.len() as u32,
        downloading: n(&|t| t.state.is_downloading()),
        seeding: n(&|t| t.state == TorrentState::Seeding),
        stopped: n(&|t| t.state == TorrentState::Stopped),
        errored: n(&|t| t.state == TorrentState::Errored),
    }
}

pub fn health_of(data: &QbittorrentData) -> (Health, Option<String>) {
    if data.connection == "disconnected" {
        return (Health::Warning, Some("no network connection".into()));
    }
    if data.counts.errored > 0 {
        return (
            Health::Warning,
            Some(format!(
                "{} torrent{} errored",
                data.counts.errored,
                if data.counts.errored == 1 { "" } else { "s" }
            )),
        );
    }
    (Health::Healthy, None)
}

#[derive(Deserialize)]
struct MainData {
    #[serde(default)]
    server_state: ServerState,
    #[serde(default)]
    torrents: HashMap<String, RawTorrent>,
}

#[derive(Deserialize, Default)]
struct ServerState {
    #[serde(default)]
    dl_info_speed: u64,
    #[serde(default)]
    up_info_speed: u64,
    #[serde(default)]
    connection_status: String,
    #[serde(default)]
    use_alt_speed_limits: bool,
    free_space_on_disk: Option<u64>,
}

#[derive(Deserialize, Default)]
struct RawTorrent {
    #[serde(default)]
    name: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    progress: f64,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    downloaded: u64,
    #[serde(default)]
    dlspeed: u64,
    #[serde(default)]
    upspeed: u64,
    #[serde(default)]
    eta: i64,
    #[serde(default)]
    ratio: f64,
    #[serde(default)]
    category: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    added_on: i64,
    #[serde(default)]
    num_seeds: u64,
}

/// qBittorrent reports 8640000 s (100 days) as "infinite".
const ETA_INFINITE: i64 = 8_640_000;

fn convert(hash: String, t: RawTorrent) -> Torrent {
    Torrent {
        state: TorrentState::parse(&t.state),
        raw_state: t.state,
        name: t.name,
        progress: t.progress,
        size: t.size,
        downloaded: t.downloaded,
        dl_speed: t.dlspeed,
        up_speed: t.upspeed,
        eta_s: (t.eta > 0 && t.eta < ETA_INFINITE && t.progress < 1.0).then_some(t.eta as u64),
        ratio: t.ratio,
        category: t.category,
        tags: t
            .tags
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        added_at: DateTime::from_timestamp(t.added_on, 0),
        num_seeds: t.num_seeds,
        hash,
    }
}

/// Order: downloading first (by progress), then the rest by name.
pub fn sort_torrents(torrents: &mut [Torrent]) {
    torrents.sort_by(|a, b| {
        b.state
            .is_downloading()
            .cmp(&a.state.is_downloading())
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// The session cookie from a login response, as `name=value`. qBittorrent before 5.2 calls
/// it `SID`; 5.2 and later `QBT_SID_<WebUI port>`.
fn session_cookie(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|c| c.split(';').next())
        .map(str::trim)
        .find(|kv| {
            let name = kv.split('=').next().unwrap_or_default();
            name == "SID" || name.starts_with("QBT_SID_")
        })
        .map(str::to_owned)
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &[])
}

pub struct QbittorrentModule {
    http: Option<ServiceHttp>,
    username: Option<String>,
    password: Option<String>,
    /// qBittorrent 5.2+ API key (`Authorization: Bearer qbt_…`); replaces the login.
    api_key: Option<String>,
    /// Session cookie as `name=value`: `SID` before 5.2, `QBT_SID_<port>` since.
    sid: tokio::sync::Mutex<Option<String>>,
    api_version: tokio::sync::Mutex<Option<String>>,
    history: Mutex<VecDeque<SpeedPoint>>,
}

impl QbittorrentModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        Ok(Self {
            http: Some(ServiceHttp::new(cfg, &[])?),
            username: cfg.setting_str("username").map(str::to_owned),
            password: cfg.secret("password").map(str::to_owned),
            api_key: cfg
                .secret("api_key")
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(str::to_owned),
            sid: tokio::sync::Mutex::new(None),
            api_version: tokio::sync::Mutex::new(None),
            history: Mutex::new(VecDeque::new()),
        })
    }

    pub fn demo() -> Self {
        Self {
            http: None,
            username: None,
            password: None,
            api_key: None,
            sid: tokio::sync::Mutex::new(None),
            api_version: tokio::sync::Mutex::new(Some("2.11.3".into())),
            history: Mutex::new(VecDeque::new()),
        }
    }

    fn http(&self) -> &ServiceHttp {
        self.http.as_ref().expect("http module")
    }

    async fn login(&self) -> anyhow::Result<()> {
        if self.api_key.is_some() {
            return Ok(());
        }
        let (Some(user), Some(pass)) = (&self.username, &self.password) else {
            return Ok(()); // auth bypassed for this client (whitelist / localhost)
        };
        let http = self.http();
        let response = http
            .request(Method::POST, "/api/v2/auth/login")
            .header("Referer", http.base())
            .form(&[("username", user.as_str()), ("password", pass.as_str())])
            .send()
            .await?;
        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!("login blocked: too many failed attempts (IP banned by qBittorrent)");
        }
        let sid = session_cookie(response.headers());
        let body = response.text().await.unwrap_or_default();
        match sid {
            Some(sid) => {
                *self.sid.lock().await = Some(sid);
                Ok(())
            }
            None if body.trim() == "Fails." || status == reqwest::StatusCode::UNAUTHORIZED => {
                anyhow::bail!("authentication failed (check username and password)")
            }
            None => anyhow::bail!(
                "login failed: qBittorrent answered HTTP {} without a session cookie{}",
                status.as_u16(),
                if body.trim().is_empty() {
                    String::new()
                } else {
                    format!(" ({})", body.trim())
                }
            ),
        }
    }

    /// Send with the session cookie; log in (again) once on 403.
    async fn send(
        &self,
        method: Method,
        path: &str,
        form: &[(&str, String)],
    ) -> anyhow::Result<reqwest::Response> {
        for attempt in 0..2 {
            if attempt == 0 && self.sid.lock().await.is_none() {
                self.login().await?;
            }
            let mut req = self
                .http()
                .request(method.clone(), path)
                .header("Referer", self.http().base());
            if let Some(key) = &self.api_key {
                req = req.bearer_auth(key);
            } else if let Some(sid) = self.sid.lock().await.as_deref() {
                req = req.header(reqwest::header::COOKIE, sid);
            }
            if method == Method::POST {
                req = req.form(form);
            } else {
                req = req.query(form);
            }
            let response = req.send().await?;
            if self.api_key.is_some() && response.status() == reqwest::StatusCode::UNAUTHORIZED {
                anyhow::bail!("API key rejected (Options → WebUI → API key)");
            }
            if response.status() == reqwest::StatusCode::FORBIDDEN
                && attempt == 0
                && self.username.is_some()
            {
                *self.sid.lock().await = None;
                self.login().await?;
                continue;
            }
            return ServiceHttp::send_checked(response);
        }
        anyhow::bail!("authentication failed")
    }

    async fn api_version(&self) -> anyhow::Result<String> {
        if let Some(v) = self.api_version.lock().await.clone() {
            return Ok(v);
        }
        let v = self
            .send(Method::GET, "/api/v2/app/webapiVersion", &[])
            .await?
            .text()
            .await?
            .trim()
            .to_owned();
        *self.api_version.lock().await = Some(v.clone());
        Ok(v)
    }

    /// `stop`/`start` on WebAPI ≥ 2.11, `pause`/`resume` before.
    async fn verbs(&self) -> anyhow::Result<(&'static str, &'static str)> {
        let v = self.api_version().await?;
        let mut parts = v.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        let (major, minor) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
        Ok(if (major, minor) >= (2, 11) {
            ("stop", "start")
        } else {
            ("pause", "resume")
        })
    }

    fn record(&self, dl: u64, up: u64) -> Vec<SpeedPoint> {
        let mut h = self.history.lock().expect("lock");
        let now = Utc::now();
        h.push_back(SpeedPoint { at: now, dl, up });
        while h
            .front()
            .is_some_and(|p| (now - p.at).num_seconds() > HISTORY_SECS)
        {
            h.pop_front();
        }
        h.iter().cloned().collect()
    }

    async fn fetch(&self) -> anyhow::Result<QbittorrentData> {
        if self.http.is_none() {
            let mut data = DEMO.qbittorrent();
            data.history = self.record(data.dl_speed, data.up_speed);
            return Ok(data);
        }
        let api_version = self.api_version().await?;
        let main: MainData = self
            .send(Method::GET, "/api/v2/sync/maindata", &[])
            .await?
            .json()
            .await?;
        let mut torrents: Vec<Torrent> = main
            .torrents
            .into_iter()
            .map(|(h, t)| convert(h, t))
            .collect();
        sort_torrents(&mut torrents);
        let s = main.server_state;
        Ok(QbittorrentData {
            api_version,
            dl_speed: s.dl_info_speed,
            up_speed: s.up_info_speed,
            connection: s.connection_status,
            alt_speed: s.use_alt_speed_limits,
            free_space: s.free_space_on_disk,
            counts: count(&torrents),
            torrents,
            history: self.record(s.dl_info_speed, s.up_info_speed),
        })
    }
}

fn hashes(args: &Value) -> Result<String, ModuleError> {
    match args.get("hashes") {
        Some(Value::String(s)) if s == "all" => Ok("all".into()),
        Some(Value::Array(a)) if !a.is_empty() => {
            let list: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
            if list.len() != a.len()
                || list
                    .iter()
                    .any(|h| !h.chars().all(|c| c.is_ascii_hexdigit()))
            {
                return Err(ModuleError::InvalidArgs(
                    "hashes must be hex strings".into(),
                ));
            }
            Ok(list.join("|"))
        }
        _ => Err(ModuleError::InvalidArgs(
            "hashes: a list of torrent hashes or \"all\"".into(),
        )),
    }
}

#[async_trait]
impl ServiceModule for QbittorrentModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Qbittorrent
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let data = self.fetch().await?;
        let (health, health_reason) = health_of(&data);
        Ok(PollOutcome {
            health,
            health_reason,
            response_ms: Some(started.elapsed().as_millis() as u64),
            version: Some(format!("WebAPI {}", data.api_version)),
            data: ModuleData::Qbittorrent(Box::new(data)),
        })
    }

    fn actions(&self) -> Vec<ActionDescriptor> {
        vec![
            ActionDescriptor::new("toggle_alt_speed", "Toggle alternative speed limits", false),
            ActionDescriptor::new("stop", "Stop (pause) torrents", false),
            ActionDescriptor::new("start", "Start (resume) torrents", false),
            ActionDescriptor::new("delete", "Delete torrents (optionally with files)", true),
        ]
    }

    async fn run_action(&self, action: &str, args: Value) -> Result<ActionResult, ModuleError> {
        if self.http.is_none() {
            return DEMO.qbittorrent_action(action, &args);
        }
        let ok = |m: String| {
            Ok(ActionResult {
                ok: true,
                message: m,
                task: None,
            })
        };
        match action {
            "toggle_alt_speed" => {
                self.send(Method::POST, "/api/v2/transfer/toggleSpeedLimitsMode", &[])
                    .await?;
                ok("alternative speed limits toggled".into())
            }
            "stop" | "start" => {
                let h = hashes(&args)?;
                let (stop, start) = self.verbs().await?;
                let verb = if action == "stop" { stop } else { start };
                self.send(
                    Method::POST,
                    &format!("/api/v2/torrents/{verb}"),
                    &[("hashes", h)],
                )
                .await?;
                ok(format!("{action} sent"))
            }
            "delete" => {
                let h = hashes(&args)?;
                if h == "all" {
                    return Err(ModuleError::InvalidArgs(
                        "delete needs explicit hashes".into(),
                    ));
                }
                let files = args
                    .get("delete_files")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.send(
                    Method::POST,
                    "/api/v2/torrents/delete",
                    &[("hashes", h), ("deleteFiles", files.to_string())],
                )
                .await?;
                ok(if files {
                    "deleted with files".into()
                } else {
                    "deleted, files kept".into()
                })
            }
            other => Err(ModuleError::UnknownAction(other.into())),
        }
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok(d) => ConnectionTest {
                ok: true,
                message: format!("{} torrents", d.counts.all),
                version: Some(format!("WebAPI {}", d.api_version)),
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
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(uri: &str) -> InstanceConfig {
        InstanceConfig {
            id: "qbittorrent".into(),
            kind: ServiceKind::Qbittorrent,
            name: "qBittorrent".into(),
            base_url: Some(uri.into()),
            settings: json!({"username": "admin"}),
            secrets: [("password".to_owned(), "pw".to_owned())].into(),
        }
    }

    async fn server(version: &str) -> MockServer {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .and(body_string_contains("username=admin"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "SID=abc123; path=/")
                    .set_body_string("Ok."),
            )
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/app/webapiVersion"))
            .and(header("cookie", "SID=abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_string(version))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/sync/maindata"))
            .and(header("cookie", "SID=abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "server_state": {"dl_info_speed": 40055603, "up_info_speed": 2516582, "connection_status": "connected",
                                 "use_alt_speed_limits": false, "free_space_on_disk": 1979120929996u64},
                "torrents": {
                    "aa11": {"name": "Glass.Meridian.2025.2160p", "state": "downloading", "progress": 0.62, "size": 19756849561u64,
                             "downloaded": 12240656793u64, "dlspeed": 25270000, "upspeed": 0, "eta": 300, "ratio": 0.0,
                             "category": "radarr", "tags": "", "added_on": 1791360000, "num_seeds": 12},
                    "bb22": {"name": "Old.Show.S01", "state": "stoppedUP", "progress": 1.0, "size": 100, "eta": 8640000},
                    "cc33": {"name": "Broken", "state": "error", "progress": 0.1, "size": 100}
                }
            })))
            .mount(&s)
            .await;
        s
    }

    #[tokio::test]
    async fn polls_maindata_with_session_cookie() {
        let s = server("2.11.3").await;
        let m = QbittorrentModule::new(&cfg(&s.uri())).unwrap();
        let out = m.poll().await.unwrap();
        let ModuleData::Qbittorrent(d) = out.data else {
            panic!()
        };
        assert_eq!(d.counts.all, 3);
        assert_eq!(
            (d.counts.downloading, d.counts.stopped, d.counts.errored),
            (1, 1, 1)
        );
        assert_eq!(d.torrents[0].name, "Glass.Meridian.2025.2160p");
        assert_eq!(d.torrents[0].eta_s, Some(300));
        assert_eq!(out.health, Health::Warning);
        assert_eq!(d.headline().0, "1 downloading");
        assert!(d.headline().1.unwrap().contains("38.2 MB/s"));
    }

    /// qBittorrent 5.2+: the cookie is `QBT_SID_<port>` and a successful login has no body.
    #[tokio::test]
    async fn logs_in_with_the_5_2_cookie_name() {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "QBT_SID_8080=xyz; HttpOnly; path=/"),
            )
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/app/webapiVersion"))
            .and(header("cookie", "QBT_SID_8080=xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_string("2.12.0"))
            .expect(1)
            .mount(&s)
            .await;
        let m = QbittorrentModule::new(&cfg(&s.uri())).unwrap();
        assert_eq!(m.api_version().await.unwrap(), "2.12.0");
    }

    #[tokio::test]
    async fn wrong_password_and_missing_cookie_are_explained() {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&s)
            .await;
        let m = QbittorrentModule::new(&cfg(&s.uri())).unwrap();
        let err = m.api_version().await.unwrap_err().to_string();
        assert!(err.contains("check username and password"), "{err}");

        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&s)
            .await;
        let m = QbittorrentModule::new(&cfg(&s.uri())).unwrap();
        let err = m.api_version().await.unwrap_err().to_string();
        assert!(err.contains("HTTP 200 without a session cookie"), "{err}");
    }

    /// qBittorrent 5.2+ API key: bearer header, no login call.
    #[tokio::test]
    async fn api_key_skips_login() {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/app/webapiVersion"))
            .and(header("authorization", "Bearer qbt_key"))
            .respond_with(ResponseTemplate::new(200).set_body_string("2.12.0"))
            .mount(&s)
            .await;
        let mut c = cfg(&s.uri());
        c.settings = json!({});
        c.secrets = [("api_key".to_owned(), "qbt_key".to_owned())].into();
        let m = QbittorrentModule::new(&c).unwrap();
        assert_eq!(m.api_version().await.unwrap(), "2.12.0");

        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&s)
            .await;
        c.base_url = Some(s.uri());
        let m = QbittorrentModule::new(&c).unwrap();
        let err = m.api_version().await.unwrap_err().to_string();
        assert!(err.contains("API key rejected"), "{err}");
    }

    #[tokio::test]
    async fn uses_stop_on_v5_and_pause_before() {
        for (version, verb) in [("2.11.3", "stop"), ("2.9.3", "pause")] {
            let s = server(version).await;
            Mock::given(method("POST"))
                .and(path(format!("/api/v2/torrents/{verb}")))
                .and(body_string_contains("hashes=aa11"))
                .respond_with(ResponseTemplate::new(200))
                .expect(1)
                .mount(&s)
                .await;
            let m = QbittorrentModule::new(&cfg(&s.uri())).unwrap();
            m.run_action("stop", json!({"hashes": ["aa11"]}))
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn delete_passes_delete_files() {
        let s = server("2.11.3").await;
        Mock::given(method("POST"))
            .and(path("/api/v2/torrents/delete"))
            .and(body_string_contains("deleteFiles=true"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&s)
            .await;
        let m = QbittorrentModule::new(&cfg(&s.uri())).unwrap();
        m.run_action("delete", json!({"hashes": ["aa11"], "delete_files": true}))
            .await
            .unwrap();
        assert!(
            m.run_action("delete", json!({"hashes": "all"}))
                .await
                .is_err()
        );
        assert!(
            m.run_action("stop", json!({"hashes": ["../x"]}))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn bad_password() {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_string("Fails."))
            .mount(&s)
            .await;
        let m = QbittorrentModule::new(&cfg(&s.uri())).unwrap();
        let err = m.poll().await.unwrap_err();
        assert!(err.to_string().contains("authentication failed"), "{err}");
    }
}
