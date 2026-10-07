//! Navidrome via the Subsonic API: now playing, library scan and scan status.
//!
//! Subsonic auth: `u`, `t` = md5(password + salt), `s` = salt, `v`, `c`, `f=json`.
//! Endpoints: `ping`, `getNowPlaying`, `getScanStatus`, `startScan`.

use std::time::Instant;

use async_trait::async_trait;
use md5::{Digest, Md5};
use rand::RngCore;
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
pub struct NowPlaying {
    pub user: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub player: Option<String>,
    pub minutes_ago: Option<u64>,
    pub duration_s: Option<u64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NavidromeData {
    pub now_playing: Vec<NowPlaying>,
    pub scanning: bool,
    /// Songs in the library (from the scan status).
    pub songs: Option<u64>,
    pub folders: Option<u64>,
    pub last_scan: Option<String>,
}

impl NavidromeData {
    pub fn headline(&self) -> (String, Option<String>) {
        let n = self.now_playing.len();
        (
            if self.scanning {
                "Scanning".into()
            } else if n > 0 {
                format!("{n} playing")
            } else {
                "Idle".into()
            },
            self.songs
                .map(|s| format!("{} songs", super::http::fmt_count(s))),
        )
    }
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["password"])?;
    cfg.setting_str("username").ok_or("username is required")?;
    Ok(())
}

pub struct NavidromeModule {
    http: Option<ServiceHttp>,
    username: String,
    password: String,
}

impl NavidromeModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        Ok(Self {
            http: Some(ServiceHttp::new(cfg, &[])?),
            username: cfg.setting_str("username").unwrap_or_default().to_owned(),
            password: cfg.secret("password").unwrap_or_default().to_owned(),
        })
    }

    pub fn demo() -> Self {
        Self {
            http: None,
            username: String::new(),
            password: String::new(),
        }
    }

    fn auth(&self) -> Vec<(&'static str, String)> {
        let mut salt = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let salt = hex::encode(salt);
        let token = hex::encode(Md5::digest(format!("{}{salt}", self.password).as_bytes()));
        vec![
            ("u", self.username.clone()),
            ("t", token),
            ("s", salt),
            ("v", "1.16.1".into()),
            ("c", "quorum".into()),
            ("f", "json".into()),
        ]
    }

    /// Call a Subsonic method and return the `subsonic-response` object.
    async fn call(&self, method: &str, extra: &[(&'static str, String)]) -> anyhow::Result<Value> {
        let http = self.http.as_ref().expect("http");
        let mut q = self.auth();
        q.extend_from_slice(extra);
        let v: Value = http.get(&format!("/rest/{method}"), &q).await?;
        let r = v.get("subsonic-response").cloned().unwrap_or(Value::Null);
        if r.get("status").and_then(Value::as_str) != Some("ok") {
            let msg = r
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("request failed");
            let code = r
                .pointer("/error/code")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if code == 40 || code == 41 {
                anyhow::bail!("authentication failed (check username and password)");
            }
            anyhow::bail!("Navidrome: {msg}");
        }
        Ok(r)
    }

    async fn fetch(&self) -> anyhow::Result<(NavidromeData, Option<String>)> {
        if self.http.is_none() {
            return Ok((DEMO.navidrome(), Some("0.58.0".into())));
        }
        let (np, scan) = tokio::try_join!(
            self.call("getNowPlaying", &[]),
            self.call("getScanStatus", &[])
        )?;
        let version = np
            .get("serverVersion")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok((parse(&np, &scan), version))
    }
}

pub fn parse(np: &Value, scan: &Value) -> NavidromeData {
    let entries = match np.pointer("/nowPlaying/entry") {
        Some(Value::Array(a)) => a.clone(),
        Some(v @ Value::Object(_)) => vec![v.clone()],
        _ => Vec::new(),
    };
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let st = scan.get("scanStatus").cloned().unwrap_or(Value::Null);
    NavidromeData {
        now_playing: entries
            .iter()
            .map(|e| NowPlaying {
                user: s(e, "username").unwrap_or_default(),
                title: s(e, "title").unwrap_or_default(),
                artist: s(e, "artist"),
                album: s(e, "album"),
                player: s(e, "playerName"),
                minutes_ago: e.get("minutesAgo").and_then(Value::as_u64),
                duration_s: e.get("duration").and_then(Value::as_u64),
            })
            .collect(),
        scanning: st.get("scanning").and_then(Value::as_bool).unwrap_or(false),
        songs: st.get("count").and_then(Value::as_u64),
        folders: st.get("folderCount").and_then(Value::as_u64),
        last_scan: s(&st, "lastScan"),
    }
}

#[async_trait]
impl ServiceModule for NavidromeModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Navidrome
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let (data, version) = self.fetch().await?;
        Ok(PollOutcome {
            health: if data.scanning {
                Health::InProgress
            } else {
                Health::Healthy
            },
            health_reason: data.scanning.then(|| "scanning library".into()),
            response_ms: Some(started.elapsed().as_millis() as u64),
            version,
            data: ModuleData::Navidrome(Box::new(data)),
        })
    }

    fn actions(&self) -> Vec<ActionDescriptor> {
        vec![
            ActionDescriptor::new("scan", "Scan library", false),
            ActionDescriptor::new("full_scan", "Full rescan", false),
        ]
    }

    async fn run_action(&self, action: &str, _args: Value) -> Result<ActionResult, ModuleError> {
        let full = match action {
            "scan" => false,
            "full_scan" => true,
            other => return Err(ModuleError::UnknownAction(other.into())),
        };
        if self.http.is_some() {
            self.call("startScan", &[("fullScan", full.to_string())])
                .await?;
        } else {
            DEMO.navidrome_scan();
        }
        Ok(ActionResult {
            ok: true,
            message: "scan started".into(),
            task: None,
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        if self.http.is_none() {
            return ConnectionTest {
                ok: true,
                message: "pong (demo)".into(),
                ..Default::default()
            };
        }
        match self.call("ping", &[]).await {
            Ok(r) => ConnectionTest {
                ok: true,
                message: "Subsonic API reachable".into(),
                version: r
                    .get("serverVersion")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
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
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn parses_single_entry_object() {
        let d = parse(
            &json!({"nowPlaying": {"entry": {"username": "janick", "title": "Midnight Ferry", "artist": "The Tidelines", "minutesAgo": 0}}}),
            &json!({"scanStatus": {"scanning": false, "count": 12345, "folderCount": 830}}),
        );
        assert_eq!(d.now_playing[0].title, "Midnight Ferry");
        assert_eq!(
            d.headline(),
            ("1 playing".into(), Some("12,345 songs".into()))
        );
    }

    #[tokio::test]
    async fn token_auth_and_errors() {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/ping"))
            .and(query_param("u", "janick"))
            .and(query_param("f", "json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"subsonic-response":
                {"status": "failed", "error": {"code": 40, "message": "Wrong username or password"}}})))
            .mount(&s)
            .await;
        let cfg = InstanceConfig {
            id: "navidrome".into(),
            kind: ServiceKind::Navidrome,
            name: "Navidrome".into(),
            base_url: Some(s.uri()),
            settings: json!({"username": "janick"}),
            secrets: [("password".to_owned(), "pw".to_owned())].into(),
        };
        let m = NavidromeModule::new(&cfg).unwrap();
        let t = m.test_connection().await;
        assert!(!t.ok);
        assert!(t.message.contains("authentication failed"));
        let q = m.auth();
        let salt = &q.iter().find(|(k, _)| *k == "s").unwrap().1;
        let token = &q.iter().find(|(k, _)| *k == "t").unwrap().1;
        assert_eq!(
            *token,
            hex::encode(Md5::digest(format!("pw{salt}").as_bytes()))
        );
    }
}
