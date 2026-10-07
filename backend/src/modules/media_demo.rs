//! Demo data for the media modules, shared so the request tracker can follow a title from
//! Seerr through Radarr/Sonarr and qBittorrent to Jellyfin (matching the design's samples:
//! Glass Meridian, Saltwater Kings, The Long Orbit, Harbor Lights).

use std::sync::{LazyLock, Mutex};
use std::time::{Duration as StdDuration, Instant};

use chrono::{Duration, Utc};
use rand::Rng;
use serde_json::Value;

use super::arr::{
    ArrData, ArrFlavor, CalendarItem, Disk, HealthIssue, LibraryItem, MissingItem, QueueItem,
};
use super::bazarr::{BazarrData, SubtitleEvent, WantedSubtitle};
use super::jellyfin::{
    ItemCounts, JellyfinData, JellyfinUser, Library, RecentItem, Stream, Transcode,
};
use super::navidrome::{NavidromeData, NowPlaying};
use super::prowlarr::{Indexer, ProwlarrData};
use super::qbittorrent::{QbittorrentData, Torrent, TorrentState, count, sort_torrents};
use super::seerr::{MediaRequest, RequestCounts, SearchResult, SeerrData};
use super::{ActionResult, ModuleError};

const GB: u64 = 1024 * 1024 * 1024;
const MB: u64 = 1024 * 1024;

pub const HASH_GLASS: &str = "8c212779b4abde7c6bc608063a0d008b7e40ce32";
pub const HASH_SALTWATER: &str = "54eddd830a5b58480a6143d616a97e3a6c23c439";
pub const HASH_ORBIT: &str = "3f1e1c0a2b4d6e8f90a1b2c3d4e5f60718293a4b";
pub const HASH_COPPER: &str = "9a8b7c6d5e4f30211a2b3c4d5e6f708192a3b4c5";

pub struct DemoMedia {
    state: Mutex<State>,
}

struct State {
    started: Instant,
    torrents: Vec<Torrent>,
    alt_speed: bool,
    requests: Vec<MediaRequest>,
    scanning_until: Option<Instant>,
}

pub static DEMO: LazyLock<DemoMedia> = LazyLock::new(DemoMedia::new);

#[allow(clippy::too_many_arguments)]
fn torrent(
    hash: &str,
    name: &str,
    raw: &str,
    progress: f64,
    size: u64,
    dl: u64,
    up: u64,
    category: &str,
    seeds: u64,
) -> Torrent {
    Torrent {
        hash: hash.into(),
        name: name.into(),
        state: TorrentState::parse(raw),
        raw_state: raw.into(),
        progress,
        size,
        downloaded: (size as f64 * progress) as u64,
        dl_speed: dl,
        up_speed: up,
        eta_s: None,
        ratio: if progress >= 1.0 { 1.4 } else { 0.0 },
        category: category.into(),
        tags: vec![],
        added_at: Some(Utc::now() - Duration::hours(2)),
        num_seeds: seeds,
    }
}

#[allow(clippy::too_many_arguments)]
fn request(
    id: i64,
    media_type: &str,
    tmdb: i64,
    tvdb: Option<i64>,
    title: &str,
    year: i64,
    by: &str,
    hours_ago: i64,
    status: &str,
    media_status: &str,
    seasons: Vec<i64>,
) -> MediaRequest {
    MediaRequest {
        id,
        media_type: media_type.into(),
        tmdb_id: Some(tmdb),
        tvdb_id: tvdb,
        title: title.into(),
        year: Some(year),
        poster_url: None,
        requested_by: by.into(),
        requested_at: Some(Utc::now() - Duration::hours(hours_ago)),
        status: status.into(),
        media_status: media_status.into(),
        seasons,
        is_4k: false,
    }
}

impl DemoMedia {
    fn new() -> Self {
        let mut torrents = vec![
            torrent(
                HASH_GLASS,
                "Glass.Meridian.2025.2160p.WEB-DL.DDP5.1",
                "downloading",
                0.62,
                18 * GB + 400 * MB,
                24 * MB + 100 * 1024,
                0,
                "radarr",
                41,
            ),
            torrent(
                HASH_SALTWATER,
                "Saltwater.Kings.S01E03.1080p.WEB.h264",
                "stalledDL",
                0.0,
                2 * GB,
                0,
                0,
                "tv-sonarr",
                0,
            ),
            torrent(
                HASH_ORBIT,
                "The.Long.Orbit.S02E06.1080p.WEB.h264",
                "downloading",
                0.4,
                3 * GB,
                12 * MB,
                0,
                "tv-sonarr",
                25,
            ),
            torrent(
                HASH_COPPER,
                "Copper.Valley.S01E02.720p.HDTV.x264",
                "downloading",
                0.05,
                GB,
                2 * MB,
                0,
                "tv-sonarr",
                9,
            ),
            torrent(
                "e1e2e3e4e5e6e7e8e9e0e1e2e3e4e5e6e7e8e9e0",
                "Old.Broadcast.Archive.1998",
                "missingFiles",
                0.0,
                4 * GB,
                0,
                0,
                "",
                0,
            ),
        ];
        let seeding = [
            "Harbor.Lights.2024.1080p.BluRay.x264",
            "The.Long.Orbit.S02E01.1080p",
            "The.Long.Orbit.S02E02.1080p",
            "The.Long.Orbit.S02E03.1080p",
            "The.Long.Orbit.S02E04.1080p",
            "The.Long.Orbit.S02E05.1080p",
            "Paper.Lanterns.2022.2160p.WEB-DL",
            "Northbound.2021.1080p.BluRay",
            "Static.Bloom.OST.FLAC",
            "The.Tidelines.Midnight.Ferry.FLAC",
            "Copper.Valley.S01E01.720p",
            "Night.Shift.Diaries.S03.1080p",
            "The.Quiet.Fathom.2023.1080p",
            "Debian.13.0.0.amd64.netinst.iso",
            "Saltwater.Kings.S01E01.1080p",
            "Saltwater.Kings.S01E02.1080p",
            "Ubuntu.26.04.desktop.amd64.iso",
        ];
        for (i, name) in seeding.iter().enumerate() {
            let up = if i < 4 { 600 * 1024 } else { 0 };
            torrents.push(torrent(
                &format!("{:040x}", 0xabc000 + i),
                name,
                "uploading",
                1.0,
                (2 + i as u64 % 5) * GB,
                0,
                up,
                "",
                30,
            ));
        }
        let requests = vec![
            request(
                31,
                "movie",
                1003,
                None,
                "Static Bloom",
                2024,
                "leo",
                0,
                "pending",
                "pending",
                vec![],
            ),
            request(
                30,
                "tv",
                1004,
                Some(2003),
                "Copper Valley",
                2025,
                "mara",
                3,
                "pending",
                "pending",
                vec![2],
            ),
            request(
                28,
                "movie",
                1001,
                None,
                "Glass Meridian",
                2025,
                "leo",
                2,
                "approved",
                "processing",
                vec![],
            ),
            request(
                27,
                "tv",
                1005,
                Some(2001),
                "Saltwater Kings",
                2026,
                "mara",
                26,
                "approved",
                "processing",
                vec![1],
            ),
            request(
                22,
                "movie",
                1002,
                None,
                "Harbor Lights",
                2024,
                "janick",
                120,
                "approved",
                "available",
                vec![],
            ),
        ];
        Self {
            state: Mutex::new(State {
                started: Instant::now(),
                torrents,
                alt_speed: false,
                requests,
                scanning_until: None,
            }),
        }
    }

    pub fn note(&self, _msg: &str) {}

    pub fn qbittorrent(&self) -> QbittorrentData {
        let mut st = self.state.lock().expect("lock");
        let elapsed = st.started.elapsed().as_secs_f64();
        let alt = st.alt_speed;
        let mut rng = rand::thread_rng();
        for t in st
            .torrents
            .iter_mut()
            .filter(|t| t.state == TorrentState::Downloading)
        {
            let base = match t.hash.as_str() {
                HASH_GLASS => 24.1,
                HASH_ORBIT => 12.0,
                _ => 2.1,
            };
            let mbps = if alt { base * 0.2 } else { base } * rng.gen_range(0.85..1.15);
            t.dl_speed = (mbps * MB as f64) as u64;
            // Slow progress so the demo moves without finishing in minutes.
            let p = (t.progress + elapsed / 600_000.0).min(0.99);
            t.downloaded = (t.size as f64 * p) as u64;
            let left = t.size.saturating_sub(t.downloaded);
            t.eta_s = Some(left / t.dl_speed.max(1));
        }
        let mut torrents = st.torrents.clone();
        sort_torrents(&mut torrents);
        let dl = torrents.iter().map(|t| t.dl_speed).sum();
        let up = torrents.iter().map(|t| t.up_speed).sum();
        QbittorrentData {
            api_version: "2.11.3".into(),
            dl_speed: dl,
            up_speed: up,
            connection: "connected".into(),
            alt_speed: alt,
            free_space: Some(1_979_120_929_996),
            counts: count(&torrents),
            torrents,
            history: Vec::new(),
        }
    }

    pub fn qbittorrent_action(
        &self,
        action: &str,
        args: &Value,
    ) -> Result<ActionResult, ModuleError> {
        let mut st = self.state.lock().expect("lock");
        let selected = |t: &Torrent| match args.get("hashes") {
            Some(Value::String(s)) if s == "all" => true,
            Some(Value::Array(a)) => a.iter().any(|h| h.as_str() == Some(t.hash.as_str())),
            _ => false,
        };
        let msg = match action {
            "toggle_alt_speed" => {
                st.alt_speed = !st.alt_speed;
                if st.alt_speed {
                    "alternative speed limits on"
                } else {
                    "alternative speed limits off"
                }
            }
            "stop" => {
                for t in st.torrents.iter_mut().filter(|t| selected(t)) {
                    t.raw_state = if t.progress >= 1.0 {
                        "stoppedUP"
                    } else {
                        "stoppedDL"
                    }
                    .into();
                    t.state = TorrentState::Stopped;
                    t.dl_speed = 0;
                    t.up_speed = 0;
                }
                "stopped"
            }
            "start" => {
                for t in st
                    .torrents
                    .iter_mut()
                    .filter(|t| selected(t) && t.state == TorrentState::Stopped)
                {
                    let raw = if t.progress >= 1.0 {
                        "uploading"
                    } else {
                        "downloading"
                    };
                    t.raw_state = raw.into();
                    t.state = TorrentState::parse(raw);
                }
                "started"
            }
            "delete" => {
                if matches!(args.get("hashes"), Some(Value::String(_))) {
                    return Err(ModuleError::InvalidArgs(
                        "delete needs explicit hashes".into(),
                    ));
                }
                let before = st.torrents.len();
                st.torrents.retain(|t| !selected(t));
                if st.torrents.len() == before {
                    return Err(ModuleError::InvalidArgs("no matching torrents".into()));
                }
                "deleted"
            }
            other => return Err(ModuleError::UnknownAction(other.into())),
        };
        Ok(ActionResult {
            ok: true,
            message: msg.into(),
            task: None,
        })
    }

    pub fn arr(&self, flavor: ArrFlavor) -> (ArrData, Vec<LibraryItem>) {
        let (mut data, library) = self.arr_base(flavor);
        // Keep the queue in step with the simulated qBittorrent transfers.
        let st = self.state.lock().expect("lock");
        for q in &mut data.queue {
            let Some(t) = st.torrents.iter().find(|t| {
                q.download_id
                    .as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(&t.hash))
            }) else {
                continue;
            };
            q.size_left = (q.size as f64 * (1.0 - t.progress)).round() as u64;
            q.eta = t.eta_s.map(|s| Utc::now() + Duration::seconds(s as i64));
        }
        (data, library)
    }

    fn arr_base(&self, flavor: ArrFlavor) -> (ArrData, Vec<LibraryItem>) {
        let now = Utc::now();
        let lib = |id, title: &str, year, tmdb, tvdb, have, total, size_gb| LibraryItem {
            id,
            title: title.into(),
            year: Some(year),
            monitored: true,
            tmdb_id: tmdb,
            tvdb_id: tvdb,
            complete: have >= total,
            have,
            total,
            size: size_gb * GB,
            poster: None,
            status: None,
        };
        let q = |id,
                 title: &str,
                 sub: Option<&str>,
                 release: &str,
                 item,
                 hash: &str,
                 size: u64,
                 left: u64,
                 status: &str,
                 tracked: &str,
                 msg: Option<&str>| QueueItem {
            id,
            title: title.into(),
            subtitle: sub.map(str::to_owned),
            release: release.into(),
            item_id: Some(item),
            episode_id: None,
            size,
            size_left: left,
            status: status.into(),
            tracked_status: Some(tracked.into()),
            tracked_state: Some("downloading".into()),
            message: msg.map(str::to_owned),
            download_id: Some(hash.to_uppercase()),
            download_client: Some("qBittorrent".into()),
            protocol: Some("torrent".into()),
            eta: Some(now + Duration::minutes(5)),
        };
        let disks = vec![Disk {
            path: "/media".into(),
            free: 1_979_120_929_996,
            total: 3_958_241_859_992,
        }];
        match flavor {
            ArrFlavor::Radarr => {
                let library = vec![
                    lib(12, "Glass Meridian", 2025, Some(1001), None, 0, 1, 0),
                    lib(9, "Harbor Lights", 2024, Some(1002), None, 1, 1, 14),
                    lib(4, "Northbound", 2021, Some(1006), None, 1, 1, 9),
                    lib(6, "Paper Lanterns", 2022, Some(1007), None, 1, 1, 21),
                    lib(7, "The Quiet Fathom", 2023, Some(1008), None, 1, 1, 11),
                    lib(14, "Tin Horizon", 2026, Some(1009), None, 0, 1, 0),
                ];
                let data = ArrData {
                    flavor,
                    queue: vec![q(
                        101,
                        "Glass Meridian",
                        None,
                        "Glass.Meridian.2025.2160p.WEB-DL.DDP5.1",
                        12,
                        HASH_GLASS,
                        18 * GB + 400 * MB,
                        7 * GB,
                        "downloading",
                        "ok",
                        None,
                    )],
                    calendar: vec![CalendarItem {
                        id: 14,
                        title: "Tin Horizon".into(),
                        subtitle: Some("2026".into()),
                        at: now + Duration::days(4),
                        kind: "digital".into(),
                        has_file: false,
                        item_id: Some(14),
                    }],
                    missing: vec![MissingItem {
                        id: 14,
                        title: "Tin Horizon".into(),
                        subtitle: Some("2026".into()),
                        at: None,
                    }],
                    missing_total: 1,
                    health: vec![],
                    disks,
                    library_count: library.len() as u32,
                    library_size: library.iter().map(|l| l.size).sum(),
                };
                (data, library)
            }
            ArrFlavor::Sonarr => {
                let library = vec![
                    lib(3, "Saltwater Kings", 2026, None, Some(2001), 2, 3, 6),
                    lib(5, "The Long Orbit", 2024, None, Some(2002), 15, 16, 48),
                    lib(7, "Copper Valley", 2025, None, Some(2003), 1, 2, 2),
                    lib(8, "Night Shift Diaries", 2019, None, Some(2004), 30, 30, 61),
                ];
                let ep = |days: i64, title: &str, sub: &str, item| CalendarItem {
                    id: 900 + item,
                    title: title.into(),
                    subtitle: Some(sub.into()),
                    at: now + Duration::days(days) + Duration::hours(2),
                    kind: "airing".into(),
                    has_file: false,
                    item_id: Some(item),
                };
                let data = ArrData {
                    flavor,
                    queue: vec![
                        q(
                            201,
                            "Saltwater Kings",
                            Some("S01E03 · Low Tide"),
                            "Saltwater.Kings.S01E03.1080p.WEB.h264",
                            3,
                            HASH_SALTWATER,
                            2 * GB,
                            1640 * MB,
                            "downloading",
                            "warning",
                            Some("No seeders · 6 h"),
                        ),
                        q(
                            202,
                            "The Long Orbit",
                            Some("S02E06 · Perihelion"),
                            "The.Long.Orbit.S02E06.1080p.WEB.h264",
                            5,
                            HASH_ORBIT,
                            3 * GB,
                            1800 * MB,
                            "downloading",
                            "ok",
                            None,
                        ),
                        q(
                            203,
                            "Copper Valley",
                            Some("S01E02 · Ore"),
                            "Copper.Valley.S01E02.720p.HDTV.x264",
                            7,
                            HASH_COPPER,
                            GB,
                            970 * MB,
                            "downloading",
                            "ok",
                            None,
                        ),
                    ],
                    calendar: vec![
                        ep(0, "The Long Orbit", "S02E07 · Aphelion", 5),
                        ep(2, "Saltwater Kings", "S01E04", 3),
                        ep(6, "Copper Valley", "S01E03", 7),
                    ],
                    missing: (1..=12)
                        .map(|i| MissingItem {
                            id: 500 + i,
                            title: if i % 3 == 0 {
                                "Copper Valley"
                            } else {
                                "Night Shift Diaries"
                            }
                            .into(),
                            subtitle: Some(format!("S0{}E{:02}", 1 + i % 3, i)),
                            at: Some(now - Duration::days(i * 3)),
                        })
                        .collect(),
                    missing_total: 12,
                    health: vec![HealthIssue {
                        source: "IndexerLongTermStatusCheck".into(),
                        level: "warning".into(),
                        message: "Indexer unavailable · 6 h".into(),
                        wiki_url: None,
                    }],
                    disks,
                    library_count: library.len() as u32,
                    library_size: library.iter().map(|l| l.size).sum(),
                };
                (data, library)
            }
        }
    }

    pub fn jellyfin(&self, instance: &str) -> JellyfinData {
        let public = instance.contains("pub");
        let elapsed = self
            .state
            .lock()
            .expect("lock")
            .started
            .elapsed()
            .as_secs_f64();
        let streams = if public {
            vec![]
        } else {
            vec![
                Stream {
                    session_id: "s1".into(),
                    user: "mara".into(),
                    client: "Jellyfin Android TV".into(),
                    device: "Living room TV".into(),
                    title: "The Long Orbit".into(),
                    subtitle: Some("S02E05 · Drift".into()),
                    item_id: Some("e5".into()),
                    item_type: "Episode".into(),
                    position_s: Some((1872.0 + elapsed) % 3240.0),
                    runtime_s: Some(3240.0),
                    paused: false,
                    play_method: "Transcode".into(),
                    transcode: Some(Transcode {
                        reasons: vec!["AudioCodecNotSupported".into()],
                        video_direct: false,
                        audio_direct: false,
                        video_codec: Some("h264".into()),
                        audio_codec: Some("aac".into()),
                        bitrate: Some(12_400_000),
                        width: Some(1920),
                        height: Some(1080),
                        hardware: Some("vaapi".into()),
                    }),
                    source: Some("HEVC 1080p".into()),
                    bitrate: Some(12_400_000),
                    remote: false,
                },
                Stream {
                    session_id: "s2".into(),
                    user: "leo".into(),
                    client: "Jellyfin iOS".into(),
                    device: "iPad".into(),
                    title: "Harbor Lights".into(),
                    subtitle: Some("2024".into()),
                    item_id: Some("m2".into()),
                    item_type: "Movie".into(),
                    position_s: Some((640.0 + elapsed) % 6600.0),
                    runtime_s: Some(6600.0),
                    paused: false,
                    play_method: "DirectPlay".into(),
                    transcode: None,
                    source: Some("H264 1080p".into()),
                    bitrate: Some(25_600_000),
                    remote: true,
                },
            ]
        };
        let recent =
            |id: &str, name: &str, t: &str, series: Option<&str>, year, hours| RecentItem {
                id: id.into(),
                name: name.into(),
                item_type: t.into(),
                series_name: series.map(str::to_owned),
                year: Some(year),
                added_at: Some(Utc::now() - Duration::hours(hours)),
            };
        JellyfinData {
            server_name: if public {
                "Jellyfin (public)"
            } else {
                "Jellyfin"
            }
            .into(),
            streams,
            libraries: vec![
                Library {
                    id: "f137a2dd21bbc1b99aa5c0f6bf02a805".into(),
                    name: "Movies".into(),
                    collection_type: Some("movies".into()),
                    item_count: Some(412),
                },
                Library {
                    id: "a656b907eb3a73532e40e44b968d0225".into(),
                    name: "Shows".into(),
                    collection_type: Some("tvshows".into()),
                    item_count: Some(87),
                },
                Library {
                    id: "7e64e319657a9516ec78490da03edccb".into(),
                    name: "Music".into(),
                    collection_type: Some("music".into()),
                    item_count: Some(930),
                },
            ],
            counts: ItemCounts {
                movies: 412,
                series: 87,
                episodes: 3204,
                songs: 12_345,
                albums: 930,
            },
            recent: vec![
                recent("r1", "Harbor Lights", "Movie", None, 2024, 20),
                recent(
                    "r2",
                    "Perihelion",
                    "Episode",
                    Some("The Long Orbit"),
                    2024,
                    30,
                ),
                recent("r3", "Northbound", "Movie", None, 2021, 70),
                recent(
                    "r4",
                    "Night Watch",
                    "Episode",
                    Some("Night Shift Diaries"),
                    2019,
                    90,
                ),
                recent("r5", "Paper Lanterns", "Movie", None, 2022, 140),
            ],
            users: vec![
                JellyfinUser {
                    name: "janick".into(),
                    is_admin: true,
                    last_activity: Some(Utc::now() - Duration::minutes(40)),
                },
                JellyfinUser {
                    name: "mara".into(),
                    is_admin: false,
                    last_activity: Some(Utc::now()),
                },
                JellyfinUser {
                    name: "leo".into(),
                    is_admin: false,
                    last_activity: Some(Utc::now()),
                },
            ],
        }
    }

    pub fn seerr(&self) -> SeerrData {
        let st = self.state.lock().expect("lock");
        let pending: Vec<MediaRequest> = st
            .requests
            .iter()
            .filter(|r| r.status == "pending")
            .cloned()
            .collect();
        let n =
            |f: &dyn Fn(&MediaRequest) -> bool| st.requests.iter().filter(|r| f(r)).count() as u64;
        SeerrData {
            counts: RequestCounts {
                total: 14 + st.requests.len() as u64,
                pending: pending.len() as u64,
                approved: n(&|r| r.status == "approved"),
                processing: n(&|r| r.media_status == "processing"),
                available: 9 + n(&|r| r.media_status == "available"),
            },
            pending,
            recent: st.requests.clone(),
        }
    }

    pub fn seerr_action(&self, action: &str, args: &Value) -> Result<ActionResult, ModuleError> {
        let mut st = self.state.lock().expect("lock");
        match action {
            "approve" | "decline" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| ModuleError::InvalidArgs("id is required".into()))?;
                let r = st
                    .requests
                    .iter_mut()
                    .find(|r| r.id == id)
                    .ok_or_else(|| ModuleError::InvalidArgs("no such request".into()))?;
                r.status = if action == "approve" {
                    "approved"
                } else {
                    "declined"
                }
                .into();
                if action == "approve" {
                    r.media_status = "processing".into();
                }
                Ok(ActionResult {
                    ok: true,
                    message: format!("{} {action}d", r.title),
                    task: None,
                })
            }
            "request" => {
                let tmdb = args
                    .get("tmdb_id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| ModuleError::InvalidArgs("tmdb_id is required".into()))?;
                let media_type = args
                    .get("media_type")
                    .and_then(Value::as_str)
                    .unwrap_or("movie")
                    .to_owned();
                let title = Self::catalogue()
                    .into_iter()
                    .find(|s| s.tmdb_id == tmdb)
                    .map(|s| s.title)
                    .unwrap_or_else(|| format!("TMDB {tmdb}"));
                let id = st.requests.iter().map(|r| r.id).max().unwrap_or(0) + 1;
                st.requests.insert(
                    0,
                    request(
                        id,
                        &media_type,
                        tmdb,
                        None,
                        &title,
                        2026,
                        "demo",
                        0,
                        "approved",
                        "processing",
                        vec![],
                    ),
                );
                Ok(ActionResult {
                    ok: true,
                    message: format!("{title} requested"),
                    task: None,
                })
            }
            other => Err(ModuleError::UnknownAction(other.into())),
        }
    }

    fn catalogue() -> Vec<SearchResult> {
        let r =
            |t: &str, id, title: &str, year, status: Option<&str>, overview: &str| SearchResult {
                media_type: t.into(),
                tmdb_id: id,
                title: title.into(),
                year: Some(year),
                poster_url: None,
                overview: Some(overview.into()),
                media_status: status.map(str::to_owned),
            };
        vec![
            r(
                "movie",
                1001,
                "Glass Meridian",
                2025,
                Some("processing"),
                "A cartographer maps a city that rearranges itself every night.",
            ),
            r(
                "movie",
                1002,
                "Harbor Lights",
                2024,
                Some("available"),
                "Two lighthouse keepers trade letters across a strait.",
            ),
            r(
                "movie",
                1003,
                "Static Bloom",
                2024,
                Some("pending"),
                "A radio operator hears a garden growing.",
            ),
            r(
                "movie",
                1010,
                "Glasshouse Weather",
                2023,
                None,
                "A botanist forecasts storms inside a greenhouse.",
            ),
            r(
                "tv",
                1005,
                "Saltwater Kings",
                2026,
                Some("processing"),
                "Rival fishing families on a cold coast.",
            ),
            r(
                "tv",
                1011,
                "Harbor Patrol",
                2022,
                None,
                "Night shifts on the water.",
            ),
        ]
    }

    pub fn seerr_search(&self, q: &str) -> Vec<SearchResult> {
        let q = q.to_lowercase();
        Self::catalogue()
            .into_iter()
            .filter(|s| s.title.to_lowercase().contains(&q))
            .collect()
    }

    pub fn prowlarr(&self) -> ProwlarrData {
        let ix = |id, name: &str, privacy: &str, status: &str, ms, q, g, f| Indexer {
            id,
            name: name.into(),
            enabled: true,
            protocol: "torrent".into(),
            privacy: privacy.into(),
            status: status.into(),
            disabled_till: (status == "failing").then(|| Utc::now() + Duration::minutes(40)),
            avg_response_ms: Some(ms),
            queries_24h: q,
            grabs_24h: g,
            failures_24h: f,
        };
        ProwlarrData {
            indexers: vec![
                ix(1, "Atlas", "public", "ok", 380, 212, 6, 0),
                ix(2, "Beacon", "private", "ok", 512, 190, 4, 1),
                ix(3, "Corsair", "public", "failing", 5000, 140, 0, 37),
                ix(4, "Delta", "semiPrivate", "ok", 620, 133, 2, 0),
                ix(5, "Ember", "private", "ok", 290, 201, 9, 0),
                ix(6, "Fjord", "public", "ok", 880, 98, 1, 3),
            ],
            health: vec![],
        }
    }

    pub fn bazarr(&self) -> BazarrData {
        let ep = |title: &str, sub: &str, langs: &[&str], id| WantedSubtitle {
            kind: "episode".into(),
            title: title.into(),
            subtitle: Some(sub.into()),
            languages: langs.iter().map(|s| s.to_string()).collect(),
            item_id: id,
        };
        BazarrData {
            missing_episodes: 12,
            missing_movies: 3,
            wanted: vec![
                ep("Saltwater Kings", "1x02 · Breakwater", &["Danish"], 3),
                ep(
                    "The Long Orbit",
                    "2x05 · Drift",
                    &["Danish", "English (HI)"],
                    5,
                ),
                ep("Copper Valley", "1x01 · Seam", &["Danish"], 7),
                WantedSubtitle {
                    kind: "movie".into(),
                    title: "Northbound".into(),
                    subtitle: None,
                    languages: vec!["Danish".into()],
                    item_id: 4,
                },
            ],
            history: vec![SubtitleEvent {
                kind: "movie".into(),
                title: "Harbor Lights".into(),
                description:
                    "Danish subtitles downloaded from opensubtitles.com with a score of 96 %."
                        .into(),
                language: Some("Danish".into()),
                provider: Some("opensubtitles.com".into()),
                when: Some("2 hours ago".into()),
            }],
        }
    }

    pub fn navidrome(&self) -> NavidromeData {
        let scanning = self
            .state
            .lock()
            .expect("lock")
            .scanning_until
            .is_some_and(|u| u > Instant::now());
        NavidromeData {
            now_playing: vec![NowPlaying {
                user: "janick".into(),
                title: "Midnight Ferry".into(),
                artist: Some("The Tidelines".into()),
                album: Some("Low Water".into()),
                player: Some("Feishin".into()),
                minutes_ago: Some(0),
                duration_s: Some(254),
            }],
            scanning,
            songs: Some(12_345),
            folders: Some(830),
            last_scan: Some((Utc::now() - Duration::hours(5)).to_rfc3339()),
        }
    }

    pub fn navidrome_scan(&self) {
        self.state.lock().expect("lock").scanning_until =
            Some(Instant::now() + StdDuration::from_secs(8));
    }
}
