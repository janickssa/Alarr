//! Media overview (now playing, request tracker, calendar, recently added) and media search.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::ApiResult;
use crate::auth::CurrentUser;
use crate::modules::arr::{ArrData, ArrFlavor, CalendarItem, LibraryItem, QueueItem};
use crate::modules::http::{fmt_bitrate, fmt_bytes};
use crate::modules::jellyfin::RecentItem;
use crate::modules::qbittorrent::{Torrent, TorrentState};
use crate::modules::seerr::MediaRequest;
use crate::modules::{ModuleData, ServiceKind, ServiceSnapshot};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(overview))
        .routes(routes!(search))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NowPlayingEntry {
    pub service_id: String,
    pub service_name: String,
    /// `video` or `music`.
    pub kind: String,
    pub user: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub device: Option<String>,
    /// 0.0–1.0.
    pub progress: Option<f64>,
    pub position_s: Option<f64>,
    pub runtime_s: Option<f64>,
    pub paused: bool,
    /// `DirectPlay`, `DirectStream`, `Transcode` (video).
    pub play_method: Option<String>,
    /// e.g. "1080p HEVC → H.264 · 12.4 Mbit/s".
    pub detail: Option<String>,
    /// e.g. "audio codec not supported".
    pub transcode_reason: Option<String>,
    pub remote: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Done,
    Current,
    Stuck,
    Todo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrackerStep {
    /// Requested, Approved, Grabbed, Downloading, Imported, Available.
    pub label: String,
    pub state: StepState,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrackedRequest {
    pub seerr_id: String,
    pub request: MediaRequest,
    pub steps: Vec<TrackerStep>,
    /// Sonarr/Radarr service and item, when matched.
    pub arr_service_id: Option<String>,
    pub arr_item_id: Option<i64>,
    /// qBittorrent service and torrent hash, when matched.
    pub torrent_service_id: Option<String>,
    pub torrent_hash: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarEntry {
    pub service_id: String,
    pub flavor: ArrFlavor,
    pub item: CalendarItem,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecentEntry {
    pub service_id: String,
    pub item: RecentItem,
}

#[derive(Debug, Default, Serialize, ToSchema)]
pub struct MediaCounts {
    pub streams: u32,
    pub transcodes: u32,
    pub downloading: u32,
    pub pending_requests: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MediaOverview {
    pub counts: MediaCounts,
    pub now_playing: Vec<NowPlayingEntry>,
    pub requests: Vec<TrackedRequest>,
    pub calendar: Vec<CalendarEntry>,
    pub recently_added: Vec<RecentEntry>,
}

fn reason_text(reason: &str) -> String {
    // "AudioCodecNotSupported" -> "audio codec not supported"
    let mut out = String::new();
    for (i, c) in reason.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Visible, enabled snapshots of a kind, with the service's name.
fn snapshots(
    all: &[(String, String, ServiceSnapshot)],
    kind: ServiceKind,
) -> impl Iterator<Item = &(String, String, ServiceSnapshot)> {
    all.iter().filter(move |(_, _, s)| s.kind == kind)
}

struct Arr<'a> {
    id: &'a str,
    data: &'a ArrData,
    library: std::sync::Arc<Vec<LibraryItem>>,
}

fn human_eta(at: Option<DateTime<Utc>>) -> Option<String> {
    let secs = (at? - Utc::now()).num_seconds();
    (secs > 0).then(|| {
        if secs < 3600 {
            format!("ETA {} min", (secs / 60).max(1))
        } else {
            format!("ETA {} h", secs / 3600)
        }
    })
}

fn track(
    seerr_id: &str,
    r: MediaRequest,
    arrs: &[Arr<'_>],
    torrents: &[(String, &Torrent)],
) -> TrackedRequest {
    let flavor = if r.media_type == "tv" {
        ArrFlavor::Sonarr
    } else {
        ArrFlavor::Radarr
    };
    let matched = arrs
        .iter()
        .filter(|a| a.data.flavor == flavor)
        .find_map(|a| {
            let item = a.library.iter().find(|l| match flavor {
                ArrFlavor::Radarr => l.tmdb_id.is_some() && l.tmdb_id == r.tmdb_id,
                ArrFlavor::Sonarr => l.tvdb_id.is_some() && l.tvdb_id == r.tvdb_id,
            })?;
            Some((a, item.clone()))
        });
    let queue: Vec<&QueueItem> = matched
        .as_ref()
        .map(|(a, item)| {
            a.data
                .queue
                .iter()
                .filter(|q| q.item_id == Some(item.id))
                .collect()
        })
        .unwrap_or_default();
    let torrent = queue.iter().find_map(|q| {
        let hash = q.download_id.as_deref()?.to_lowercase();
        torrents
            .iter()
            .find(|(_, t)| t.hash == hash)
            .map(|(sid, t)| (sid.clone(), *t))
    });
    let available = matches!(r.media_status.as_str(), "available" | "partially_available");
    let imported = matched.as_ref().is_some_and(|(_, i)| i.have > 0) && queue.is_empty();
    let approved = r.status == "approved";
    let age_h = r
        .requested_at
        .map(|t| (Utc::now() - t).num_hours())
        .unwrap_or(0);

    let mut steps: Vec<TrackerStep> = [
        "Requested",
        "Approved",
        "Grabbed",
        "Downloading",
        "Imported",
        "Available",
    ]
    .iter()
    .map(|l| TrackerStep {
        label: (*l).into(),
        state: StepState::Todo,
        detail: None,
    })
    .collect();
    steps[0].detail = Some("Seerr".into());
    let downloading = torrent
        .as_ref()
        .is_some_and(|(_, t)| t.progress > 0.0 && t.state != TorrentState::Errored);
    let problem = queue
        .iter()
        .find(|q| matches!(q.tracked_status.as_deref(), Some("warning" | "error")))
        .and_then(|q| q.message.clone().or_else(|| Some("needs attention".into())));
    let done = [
        true,
        approved,
        approved
            && (!queue.is_empty() && (downloading || torrent.is_none()) || imported || available),
        imported || available || torrent.as_ref().is_some_and(|(_, t)| t.progress >= 1.0),
        imported || available,
        available,
    ];
    let current = done.iter().position(|d| !d).unwrap_or(steps.len());
    for (i, step) in steps.iter_mut().enumerate() {
        step.state = if done[i] && i < current {
            StepState::Done
        } else if i == current {
            StepState::Current
        } else {
            StepState::Todo
        };
    }
    if current < steps.len() {
        let step = &mut steps[current];
        match current {
            1 => step.detail = Some("waiting for approval".into()),
            2 => {
                if let Some(msg) = problem.clone() {
                    step.state = StepState::Stuck;
                    step.detail = Some(msg);
                } else if queue.is_empty() && age_h >= 6 {
                    step.state = StepState::Stuck;
                    step.detail = Some(format!("no release found · {age_h} h"));
                } else {
                    step.detail = Some("searching".into());
                }
            }
            3 => {
                if let Some((_, t)) = &torrent {
                    let pct = (t.progress * 100.0).round();
                    let eta = t.eta_s.map(|s| {
                        if s < 3600 {
                            format!(" · ETA {} min", (s / 60).max(1))
                        } else {
                            format!(" · ETA {} h", s / 3600)
                        }
                    });
                    step.detail = Some(format!("{pct} %{}", eta.unwrap_or_default()));
                    if t.state == TorrentState::Stalled || problem.is_some() {
                        step.state = StepState::Stuck;
                        step.detail = Some(
                            problem
                                .clone()
                                .unwrap_or_else(|| format!("{pct} % · stalled")),
                        );
                    }
                } else if let Some(q) = queue.first() {
                    let pct = (q.size_left * 100)
                        .checked_div(q.size)
                        .map_or(0, |left| 100 - left);
                    step.detail = Some(format!(
                        "{pct} %{}",
                        human_eta(q.eta)
                            .map(|e| format!(" · {e}"))
                            .unwrap_or_default()
                    ));
                }
            }
            4 => step.detail = Some("importing".into()),
            _ => step.detail = Some("waiting for Jellyfin".into()),
        }
    }
    if let Some((_, item)) = &matched
        && steps[2].state == StepState::Done
        && steps[2].detail.is_none()
    {
        steps[2].detail = queue
            .first()
            .and_then(|q| q.download_client.clone())
            .or(Some(item.title.clone()))
            .filter(|_| !queue.is_empty());
    }
    if let Some(sz) = torrent.as_ref().map(|(_, t)| t.size).filter(|s| *s > 0)
        && steps[3].state == StepState::Done
    {
        steps[3].detail = Some(fmt_bytes(sz));
    }
    TrackedRequest {
        seerr_id: seerr_id.to_owned(),
        arr_service_id: matched.as_ref().map(|(a, _)| a.id.to_owned()),
        arr_item_id: matched.as_ref().map(|(_, i)| i.id),
        torrent_service_id: torrent.as_ref().map(|(s, _)| s.clone()),
        torrent_hash: torrent.as_ref().map(|(_, t)| t.hash.clone()),
        request: r,
        steps,
    }
}

/// (service id, name, snapshot) for every enabled service the user can see.
async fn visible(
    state: &AppState,
    user: &CurrentUser,
) -> anyhow::Result<Vec<(String, String, ServiceSnapshot)>> {
    Ok(state
        .db
        .list_services()
        .await?
        .into_iter()
        .filter(|r| r.enabled && user.can_see(&r.id))
        .filter_map(|r| state.cache.get(&r.id).map(|s| (r.id, r.name, s)))
        .collect())
}

/// Everything the Media page needs: now playing across Jellyfin instances and Navidrome,
/// the request tracker (Seerr → Sonarr/Radarr → qBittorrent → Jellyfin), the combined
/// calendar and recently added items.
#[utoipa::path(get, path = "/media/overview", tag = "media", operation_id = "media_overview",
    responses((status = 200, body = MediaOverview)))]
async fn overview(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<MediaOverview>> {
    let all = visible(&state, &user).await?;
    let mut counts = MediaCounts::default();
    let mut now_playing = Vec::new();
    let mut recent: Vec<RecentEntry> = Vec::new();
    for (id, name, snap) in snapshots(&all, ServiceKind::Jellyfin) {
        state.supervisor.watch(id);
        let ModuleData::Jellyfin(d) = &snap.data else {
            continue;
        };
        for s in &d.streams {
            counts.streams += 1;
            let transcoding = s.play_method == "Transcode";
            counts.transcodes += transcoding as u32;
            let detail = match (&s.transcode, transcoding) {
                (Some(t), true) => Some(format!(
                    "{} → {}{}",
                    s.source.clone().unwrap_or_default(),
                    t.video_codec
                        .clone()
                        .unwrap_or_default()
                        .to_uppercase()
                        .replace("H264", "H.264"),
                    s.bitrate
                        .map(|b| format!(" · {}", fmt_bitrate(b)))
                        .unwrap_or_default()
                )),
                _ => s.source.clone().map(|src| {
                    format!(
                        "{src} · {}{}",
                        s.play_method,
                        s.bitrate
                            .map(|b| format!(" · {}", fmt_bitrate(b)))
                            .unwrap_or_default()
                    )
                }),
            };
            now_playing.push(NowPlayingEntry {
                service_id: id.clone(),
                service_name: name.clone(),
                kind: if s.item_type == "Audio" {
                    "music"
                } else {
                    "video"
                }
                .into(),
                user: s.user.clone(),
                title: s.title.clone(),
                subtitle: s.subtitle.clone(),
                device: Some(s.device.clone()).filter(|d| !d.is_empty()),
                progress: match (s.position_s, s.runtime_s) {
                    (Some(p), Some(r)) if r > 0.0 => Some((p / r).clamp(0.0, 1.0)),
                    _ => None,
                },
                position_s: s.position_s,
                runtime_s: s.runtime_s,
                paused: s.paused,
                play_method: Some(s.play_method.clone()),
                detail,
                transcode_reason: s
                    .transcode
                    .as_ref()
                    .and_then(|t| t.reasons.first())
                    .map(|r| reason_text(r)),
                remote: s.remote,
            });
        }
        recent.extend(d.recent.iter().map(|r| RecentEntry {
            service_id: id.clone(),
            item: r.clone(),
        }));
    }
    for (id, name, snap) in snapshots(&all, ServiceKind::Navidrome) {
        let ModuleData::Navidrome(d) = &snap.data else {
            continue;
        };
        for n in &d.now_playing {
            counts.streams += 1;
            now_playing.push(NowPlayingEntry {
                service_id: id.clone(),
                service_name: name.clone(),
                kind: "music".into(),
                user: n.user.clone(),
                title: n.title.clone(),
                subtitle: match (&n.artist, &n.album) {
                    (Some(a), Some(b)) => Some(format!("{a} · {b}")),
                    (a, b) => a.clone().or(b.clone()),
                },
                device: n.player.clone(),
                progress: None,
                position_s: None,
                runtime_s: n.duration_s.map(|d| d as f64),
                paused: false,
                play_method: None,
                detail: None,
                transcode_reason: None,
                remote: false,
            });
        }
    }
    let mut seen = HashSet::new();
    recent.sort_by_key(|r| std::cmp::Reverse(r.item.added_at));
    recent.retain(|r| seen.insert((r.item.name.clone(), r.item.series_name.clone())));
    recent.truncate(12);

    let torrents: Vec<(String, &Torrent)> = snapshots(&all, ServiceKind::Qbittorrent)
        .filter_map(|(id, _, s)| match &s.data {
            ModuleData::Qbittorrent(d) => Some(d.torrents.iter().map(move |t| (id.clone(), t))),
            _ => None,
        })
        .flatten()
        .collect();
    counts.downloading = torrents
        .iter()
        .filter(|(_, t)| t.state.is_downloading())
        .count() as u32;

    let arrs: Vec<Arr<'_>> = all
        .iter()
        .filter(|(_, _, s)| matches!(s.kind, ServiceKind::Sonarr | ServiceKind::Radarr))
        .filter_map(|(id, _, s)| {
            let ModuleData::Arr(d) = &s.data else {
                return None;
            };
            let library = state
                .supervisor
                .module(id)
                .and_then(|m| m.as_arr().map(|a| a.library()))
                .unwrap_or_default();
            Some(Arr {
                id,
                data: d,
                library,
            })
        })
        .collect();

    let mut calendar: Vec<CalendarEntry> = arrs
        .iter()
        .flat_map(|a| {
            a.data.calendar.iter().map(move |c| CalendarEntry {
                service_id: a.id.to_owned(),
                flavor: a.data.flavor,
                item: c.clone(),
            })
        })
        .filter(|c| c.item.at >= Utc::now() - Duration::hours(12))
        .collect();
    calendar.sort_by_key(|c| c.item.at);
    calendar.truncate(30);

    let mut requests = Vec::new();
    for (id, _, snap) in snapshots(&all, ServiceKind::Seerr) {
        let ModuleData::Seerr(d) = &snap.data else {
            continue;
        };
        counts.pending_requests += d.counts.pending as u32;
        let mut ids = HashSet::new();
        for r in d.pending.iter().chain(d.recent.iter()) {
            if r.status == "declined" || !ids.insert(r.id) {
                continue;
            }
            requests.push(track(id, r.clone(), &arrs, &torrents));
        }
    }
    requests.sort_by_key(|t| std::cmp::Reverse(t.request.requested_at));
    requests.truncate(12);

    Ok(Json(MediaOverview {
        counts,
        now_playing,
        requests,
        calendar,
        recently_added: recent,
    }))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MediaSearchResult {
    /// `movie`, `series` or `torrent`.
    pub kind: String,
    pub service_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    /// Movie/series id, or torrent hash.
    pub id: String,
}

/// Search movies (Radarr), series (Sonarr) and torrents (qBittorrent) by title.
#[utoipa::path(get, path = "/media/search", tag = "media", operation_id = "media_search",
    params(SearchQuery),
    responses((status = 200, body = Vec<MediaSearchResult>)))]
async fn search(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<Vec<MediaSearchResult>>> {
    let needle = q.q.trim().to_lowercase();
    if needle.len() < 2 {
        return Ok(Json(Vec::new()));
    }
    let all = visible(&state, &user).await?;
    let mut out = Vec::new();
    for (id, _, snap) in &all {
        match &snap.data {
            ModuleData::Arr(d) => {
                let Some(m) = state.supervisor.module(id) else {
                    continue;
                };
                let Some(arr) = m.as_arr() else { continue };
                for l in arr
                    .library()
                    .iter()
                    .filter(|l| l.title.to_lowercase().contains(&needle))
                    .take(8)
                {
                    out.push(MediaSearchResult {
                        kind: if d.flavor == ArrFlavor::Sonarr {
                            "series"
                        } else {
                            "movie"
                        }
                        .into(),
                        service_id: id.clone(),
                        title: l.title.clone(),
                        subtitle: Some(match (d.flavor, l.year) {
                            (ArrFlavor::Sonarr, _) => format!("{}/{} episodes", l.have, l.total),
                            (ArrFlavor::Radarr, Some(y)) => {
                                format!("{y} · {}", if l.complete { "on disk" } else { "missing" })
                            }
                            (ArrFlavor::Radarr, None) => {
                                if l.complete {
                                    "on disk".into()
                                } else {
                                    "missing".into()
                                }
                            }
                        }),
                        id: l.id.to_string(),
                    });
                }
            }
            ModuleData::Qbittorrent(d) => {
                for t in d
                    .torrents
                    .iter()
                    .filter(|t| t.name.to_lowercase().contains(&needle))
                    .take(8)
                {
                    out.push(MediaSearchResult {
                        kind: "torrent".into(),
                        service_id: id.clone(),
                        title: t.name.clone(),
                        subtitle: Some(format!(
                            "{} % · {}",
                            (t.progress * 100.0).round(),
                            fmt_bytes(t.size)
                        )),
                        id: t.hash.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    #[test]
    fn reason() {
        assert_eq!(
            super::reason_text("AudioCodecNotSupported"),
            "audio codec not supported"
        );
    }
}
