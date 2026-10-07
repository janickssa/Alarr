//! Jellyfin: active streams (with transcode reasons), libraries with scans, recently added
//! and users. Multiple instances are separate services (e.g. `jellyfin` and `jellyfin-pub`).
//!
//! Checked against the Jellyfin server source (`Jellyfin.Api/Controllers`): `GET /Sessions`,
//! `GET /Library/VirtualFolders`, `GET /Items/Counts`, `GET /Items`, `GET /Users`,
//! `GET /System/Info`, `POST /Library/Refresh`, `POST /Items/{id}/Refresh`.

use std::net::IpAddr;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use super::http::{ServiceHttp, fmt_bitrate, validate_http};
use super::media_demo::DEMO;
use super::{
    ActionDescriptor, ActionResult, ConnectionTest, Health, InstanceConfig, ModuleData,
    ModuleError, PollOutcome, ServiceKind, ServiceModule, describe_error,
};

/// Jellyfin ticks are 100 ns.
const TICKS_PER_SECOND: f64 = 10_000_000.0;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Transcode {
    /// e.g. `AudioCodecNotSupported`.
    pub reasons: Vec<String>,
    pub video_direct: bool,
    pub audio_direct: bool,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    /// Bits per second.
    pub bitrate: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub hardware: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Stream {
    pub session_id: String,
    pub user: String,
    pub client: String,
    pub device: String,
    /// Movie title or series name.
    pub title: String,
    /// `S02E05 · Episode title` for episodes.
    pub subtitle: Option<String>,
    pub item_id: Option<String>,
    /// `Movie`, `Episode`, `Audio`, …
    pub item_type: String,
    pub position_s: Option<f64>,
    pub runtime_s: Option<f64>,
    pub paused: bool,
    /// `DirectPlay`, `DirectStream` or `Transcode`.
    pub play_method: String,
    pub transcode: Option<Transcode>,
    /// Source video codec and height, e.g. `HEVC 1080p`.
    pub source: Option<String>,
    /// Bits per second (transcode bitrate, or the source bitrate when playing directly).
    pub bitrate: Option<u64>,
    /// The client is not on a private network.
    pub remote: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Library {
    /// Folder item id, used by `scan_library`.
    pub id: String,
    pub name: String,
    /// `movies`, `tvshows`, `music`, …
    pub collection_type: Option<String>,
    pub item_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RecentItem {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub series_name: Option<String>,
    pub year: Option<i64>,
    pub added_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct JellyfinUser {
    pub name: String,
    pub is_admin: bool,
    pub last_activity: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct ItemCounts {
    pub movies: u64,
    pub series: u64,
    pub episodes: u64,
    pub songs: u64,
    pub albums: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct JellyfinData {
    pub server_name: String,
    pub streams: Vec<Stream>,
    pub libraries: Vec<Library>,
    pub counts: ItemCounts,
    pub recent: Vec<RecentItem>,
    pub users: Vec<JellyfinUser>,
}

impl JellyfinData {
    pub fn transcodes(&self) -> usize {
        self.streams
            .iter()
            .filter(|s| s.play_method == "Transcode")
            .count()
    }

    pub fn headline(&self) -> (String, Option<String>) {
        let n = self.streams.len();
        if n == 0 {
            return (
                "No streams".into(),
                Some(format!(
                    "{} movies · {} series",
                    self.counts.movies, self.counts.series
                )),
            );
        }
        let total: u64 = self.streams.iter().filter_map(|s| s.bitrate).sum();
        let t = self.transcodes();
        (
            format!("{n} stream{}", if n == 1 { "" } else { "s" }),
            Some(format!(
                "{t} transcode{}{}",
                if t == 1 { "" } else { "s" },
                if total > 0 {
                    format!(" · {}", fmt_bitrate(total))
                } else {
                    String::new()
                }
            )),
        )
    }
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|x| !x.is_empty())
        .map(str::to_owned)
}

fn dt(v: &Value, k: &str) -> Option<DateTime<Utc>> {
    v.get(k)
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}

fn is_remote(ip: Option<&str>) -> bool {
    let Some(ip) = ip.and_then(|s| s.parse::<IpAddr>().ok()) else {
        return false;
    };
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.octets()[0] == 100)
        }
        IpAddr::V6(v6) => !(v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00),
    }
}

/// TranscodeReasons is a flags enum; Jellyfin serialises it as an array of names
/// (older versions: a comma-separated string).
fn reasons(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(s)) => s
            .split(',')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

pub fn parse_sessions(sessions: &[Value]) -> Vec<Stream> {
    sessions
        .iter()
        .filter_map(|v| {
            let item = v.get("NowPlayingItem")?;
            let play = v.get("PlayState").cloned().unwrap_or(Value::Null);
            let item_type = s(item, "Type").unwrap_or_default();
            let (title, subtitle) = match item_type.as_str() {
                "Episode" => (
                    s(item, "SeriesName").unwrap_or_default(),
                    Some(format!(
                        "S{:02}E{:02}{}",
                        item.get("ParentIndexNumber")
                            .and_then(Value::as_i64)
                            .unwrap_or(0),
                        item.get("IndexNumber").and_then(Value::as_i64).unwrap_or(0),
                        s(item, "Name")
                            .map(|n| format!(" · {n}"))
                            .unwrap_or_default()
                    )),
                ),
                "Audio" => (s(item, "Name").unwrap_or_default(), s(item, "AlbumArtist")),
                _ => (
                    s(item, "Name").unwrap_or_default(),
                    item.get("ProductionYear")
                        .and_then(Value::as_i64)
                        .map(|y| y.to_string()),
                ),
            };
            let video = item
                .get("MediaStreams")
                .and_then(Value::as_array)
                .and_then(|ms| ms.iter().find(|m| s(m, "Type").as_deref() == Some("Video")));
            let source = video.map(|m| {
                format!(
                    "{}{}",
                    s(m, "Codec").unwrap_or_default().to_uppercase(),
                    m.get("Height")
                        .and_then(Value::as_u64)
                        .map(|h| format!(" {h}p"))
                        .unwrap_or_default()
                )
            });
            let tc = v.get("TranscodingInfo").filter(|t| t.is_object());
            let play_method = s(&play, "PlayMethod").unwrap_or_else(|| "DirectPlay".into());
            let transcode = tc.map(|t| Transcode {
                reasons: reasons(t.get("TranscodeReasons")),
                video_direct: t
                    .get("IsVideoDirect")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                audio_direct: t
                    .get("IsAudioDirect")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                video_codec: s(t, "VideoCodec"),
                audio_codec: s(t, "AudioCodec"),
                bitrate: t.get("Bitrate").and_then(Value::as_u64),
                width: t.get("Width").and_then(Value::as_u64).map(|x| x as u32),
                height: t.get("Height").and_then(Value::as_u64).map(|x| x as u32),
                hardware: s(t, "HardwareAccelerationType"),
            });
            let source_bitrate = video.and_then(|m| m.get("BitRate").and_then(Value::as_u64));
            Some(Stream {
                session_id: s(v, "Id").unwrap_or_default(),
                user: s(v, "UserName").unwrap_or_else(|| "unknown".into()),
                client: s(v, "Client").unwrap_or_default(),
                device: s(v, "DeviceName").unwrap_or_default(),
                title,
                subtitle,
                item_id: s(item, "Id"),
                position_s: play
                    .get("PositionTicks")
                    .and_then(Value::as_f64)
                    .map(|t| t / TICKS_PER_SECOND),
                runtime_s: item
                    .get("RunTimeTicks")
                    .and_then(Value::as_f64)
                    .map(|t| t / TICKS_PER_SECOND),
                paused: play
                    .get("IsPaused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                bitrate: transcode
                    .as_ref()
                    .and_then(|t| t.bitrate)
                    .or(source_bitrate),
                play_method: if transcode
                    .as_ref()
                    .is_some_and(|t| !t.video_direct || !t.audio_direct)
                {
                    "Transcode".into()
                } else {
                    play_method
                },
                transcode,
                source,
                remote: is_remote(v.get("RemoteEndPoint").and_then(Value::as_str)),
                item_type,
            })
        })
        .collect()
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["api_key"])
}

pub struct JellyfinModule {
    http: Option<ServiceHttp>,
    instance: String,
}

impl JellyfinModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        let key = cfg.secret("api_key").unwrap_or_default();
        let auth = format!(
            "MediaBrowser Client=\"Quorum\", Device=\"quorum-server\", DeviceId=\"quorum\", Version=\"{}\", Token=\"{key}\"",
            env!("CARGO_PKG_VERSION")
        );
        Ok(Self {
            http: Some(ServiceHttp::new(cfg, &[("authorization", auth)])?),
            instance: cfg.id.clone(),
        })
    }

    pub fn demo(instance: &str) -> Self {
        Self {
            http: None,
            instance: instance.to_owned(),
        }
    }

    async fn fetch(&self) -> anyhow::Result<(JellyfinData, Option<String>)> {
        let Some(http) = &self.http else {
            return Ok((DEMO.jellyfin(&self.instance), Some("10.11.0".into())));
        };
        let recent_q = [
            ("sortBy", "DateCreated".to_owned()),
            ("sortOrder", "Descending".to_owned()),
            ("recursive", "true".to_owned()),
            ("includeItemTypes", "Movie,Episode,MusicAlbum".to_owned()),
            ("limit", "16".to_owned()),
            ("fields", "DateCreated".to_owned()),
        ];
        let sessions_q = [("activeWithinSeconds", "960".to_owned())];
        let (info, sessions, folders, counts, recent, users) = tokio::try_join!(
            http.get::<Value>("/System/Info", &[]),
            http.get::<Vec<Value>>("/Sessions", &sessions_q),
            http.get::<Vec<Value>>("/Library/VirtualFolders", &[]),
            http.get::<Value>("/Items/Counts", &[]),
            http.get::<Value>("/Items", &recent_q),
            http.get::<Vec<Value>>("/Users", &[]),
        )?;
        let mut libraries: Vec<Library> = folders
            .iter()
            .filter_map(|f| {
                Some(Library {
                    id: s(f, "ItemId")?,
                    name: s(f, "Name").unwrap_or_default(),
                    collection_type: s(f, "CollectionType"),
                    item_count: None,
                })
            })
            .collect();
        let totals = futures_util::future::join_all(libraries.iter().map(|l| {
            let q = [
                ("parentId", l.id.clone()),
                ("recursive", "true".to_owned()),
                ("limit", "0".to_owned()),
                (
                    "includeItemTypes",
                    "Movie,Series,MusicAlbum,Book,Video".to_owned(),
                ),
            ];
            async move { http.get::<Value>("/Items", &q).await }
        }))
        .await;
        for (l, t) in libraries.iter_mut().zip(totals) {
            l.item_count = t
                .ok()
                .and_then(|v| v.get("TotalRecordCount").and_then(Value::as_u64));
        }
        let c = |k: &str| counts.get(k).and_then(Value::as_u64).unwrap_or(0);
        let data = JellyfinData {
            server_name: s(&info, "ServerName").unwrap_or_else(|| "Jellyfin".into()),
            streams: parse_sessions(&sessions),
            libraries,
            counts: ItemCounts {
                movies: c("MovieCount"),
                series: c("SeriesCount"),
                episodes: c("EpisodeCount"),
                songs: c("SongCount"),
                albums: c("AlbumCount"),
            },
            recent: recent
                .get("Items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|i| RecentItem {
                            id: s(i, "Id").unwrap_or_default(),
                            name: s(i, "Name").unwrap_or_default(),
                            item_type: s(i, "Type").unwrap_or_default(),
                            series_name: s(i, "SeriesName"),
                            year: i.get("ProductionYear").and_then(Value::as_i64),
                            added_at: dt(i, "DateCreated"),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            users: users
                .iter()
                .map(|u| JellyfinUser {
                    name: s(u, "Name").unwrap_or_default(),
                    is_admin: u
                        .pointer("/Policy/IsAdministrator")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    last_activity: dt(u, "LastActivityDate"),
                })
                .collect(),
        };
        Ok((data, s(&info, "Version")))
    }
}

#[async_trait]
impl ServiceModule for JellyfinModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Jellyfin
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let (data, version) = self.fetch().await?;
        Ok(PollOutcome {
            health: Health::Healthy,
            health_reason: None,
            response_ms: Some(started.elapsed().as_millis() as u64),
            version,
            data: ModuleData::Jellyfin(Box::new(data)),
        })
    }

    fn actions(&self) -> Vec<ActionDescriptor> {
        vec![
            ActionDescriptor::new("scan_all", "Scan all libraries", false),
            ActionDescriptor::new("scan_library", "Scan one library (args: id)", false),
        ]
    }

    async fn run_action(&self, action: &str, args: Value) -> Result<ActionResult, ModuleError> {
        let ok = |m: &str| {
            Ok(ActionResult {
                ok: true,
                message: m.into(),
                task: None,
            })
        };
        match action {
            "scan_all" => {
                if let Some(http) = &self.http {
                    http.call(Method::POST, "/Library/Refresh", &[]).await?;
                }
                ok("library scan started")
            }
            "scan_library" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
                    .ok_or_else(|| ModuleError::InvalidArgs("id: library item id".into()))?;
                if let Some(http) = &self.http {
                    http.call(
                        Method::POST,
                        &format!("/Items/{id}/Refresh"),
                        &[
                            ("metadataRefreshMode", "Default".into()),
                            ("imageRefreshMode", "Default".into()),
                        ],
                    )
                    .await?;
                }
                ok("library scan started")
            }
            other => Err(ModuleError::UnknownAction(other.into())),
        }
    }

    async fn image(&self, path: &str) -> Result<(bytes::Bytes, String), ModuleError> {
        let ok_path = path.starts_with("/Items/")
            && path.contains("/Images/")
            && path
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-'));
        if !ok_path {
            return Err(ModuleError::InvalidArgs("not an item image path".into()));
        }
        match &self.http {
            Some(http) => Ok(http
                .bytes(&format!("{path}?maxHeight=360&quality=85"))
                .await?),
            None => Err(ModuleError::UnknownAction("no images in demo mode".into())),
        }
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((d, version)) => ConnectionTest {
                ok: true,
                message: format!(
                    "{} · {} libraries · {} streams",
                    d.server_name,
                    d.libraries.len(),
                    d.streams.len()
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

    #[test]
    fn sessions_with_transcode() {
        let sessions = vec![
            json!({"Id": "s1", "UserName": "mara", "Client": "Jellyfin Android TV", "DeviceName": "Living room TV",
                "RemoteEndPoint": "10.0.20.50",
                "NowPlayingItem": {"Id": "e1", "Type": "Episode", "Name": "Drift", "SeriesName": "The Long Orbit",
                    "ParentIndexNumber": 2, "IndexNumber": 5, "RunTimeTicks": 32400000000u64,
                    "MediaStreams": [{"Type": "Video", "Codec": "hevc", "Height": 1080, "BitRate": 9000000}]},
                "PlayState": {"PositionTicks": 18720000000u64, "IsPaused": false, "PlayMethod": "Transcode"},
                "TranscodingInfo": {"IsVideoDirect": false, "IsAudioDirect": false, "VideoCodec": "h264", "AudioCodec": "aac",
                    "Bitrate": 12400000, "TranscodeReasons": ["AudioCodecNotSupported"]}}),
            json!({"Id": "s2", "UserName": "leo", "RemoteEndPoint": "203.0.113.9",
                "NowPlayingItem": {"Id": "m1", "Type": "Movie", "Name": "Harbor Lights", "ProductionYear": 2024,
                    "MediaStreams": [{"Type": "Video", "Codec": "h264", "Height": 2160, "BitRate": 25600000}]},
                "PlayState": {"PlayMethod": "DirectPlay"}}),
            json!({"Id": "idle", "UserName": "x"}),
        ];
        let streams = parse_sessions(&sessions);
        assert_eq!(streams.len(), 2);
        let a = &streams[0];
        assert_eq!(a.subtitle.as_deref(), Some("S02E05 · Drift"));
        assert_eq!(a.play_method, "Transcode");
        assert_eq!(
            a.transcode.as_ref().unwrap().reasons,
            vec!["AudioCodecNotSupported"]
        );
        assert_eq!(a.source.as_deref(), Some("HEVC 1080p"));
        assert_eq!(a.position_s, Some(1872.0));
        assert!(!a.remote);
        assert!(streams[1].remote);
        assert_eq!(streams[1].bitrate, Some(25_600_000));
        let data = JellyfinData {
            server_name: "x".into(),
            streams,
            libraries: vec![],
            counts: ItemCounts::default(),
            recent: vec![],
            users: vec![],
        };
        assert_eq!(
            data.headline(),
            ("2 streams".into(), Some("1 transcode · 38 Mbit/s".into()))
        );
    }

    #[tokio::test]
    async fn scan_library_validates_id() {
        let m = JellyfinModule::demo("jellyfin");
        assert!(
            m.run_action("scan_library", json!({"id": "../x"}))
                .await
                .is_err()
        );
        assert!(
            m.run_action(
                "scan_library",
                json!({"id": "f137a2dd21bbc1b99aa5c0f6bf02a805"})
            )
            .await
            .is_ok()
        );
        assert!(m.image("/etc/passwd").await.is_err());
    }
}
