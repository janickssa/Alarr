//! `GET /widget`: one small read with everything the iPhone widgets show (problems, nodes,
//! downloads, streams). Widgets refresh on their own timeline, so this never puts a service
//! into fast polling.

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::ApiResult;
use super::overview::{NodeSummary, Problem, StatusSummary};
use crate::auth::CurrentUser;
use crate::modules::qbittorrent::TorrentState;
use crate::modules::{ModuleData, ServiceKind};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(widget))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WidgetDownload {
    pub name: String,
    /// 0.0–1.0.
    pub progress: f64,
    /// Bytes per second.
    pub dl_speed: u64,
    pub eta_s: Option<u64>,
}

#[derive(Debug, Default, Serialize, ToSchema)]
pub struct WidgetDownloads {
    /// Bytes per second, across qBittorrent instances.
    pub dl_speed: u64,
    pub up_speed: u64,
    pub downloading: u32,
    pub errored: u32,
    /// Busiest downloads first (at most 4).
    pub items: Vec<WidgetDownload>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WidgetStream {
    pub user: String,
    pub title: String,
    pub subtitle: Option<String>,
    /// 0.0–1.0.
    pub progress: Option<f64>,
    pub paused: bool,
    pub transcoding: bool,
    pub service_name: String,
}

#[derive(Debug, Default, Serialize, ToSchema)]
pub struct WidgetStreams {
    pub count: u32,
    pub transcodes: u32,
    /// At most 4.
    pub items: Vec<WidgetStream>,
}

/// One number per visible service for the small widget ("Sonarr: 3 in queue").
#[derive(Debug, Serialize, ToSchema)]
pub struct WidgetService {
    pub id: String,
    pub name: String,
    /// Phosphor icon name.
    pub icon: String,
    pub health: crate::modules::Health,
    pub headline: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WidgetSummary {
    pub generated_at: DateTime<Utc>,
    pub status: StatusSummary,
    /// Worst first.
    pub problems: Vec<Problem>,
    pub nodes: Vec<NodeSummary>,
    /// `null` without a visible qBittorrent.
    pub downloads: Option<WidgetDownloads>,
    /// `null` without a visible Jellyfin or Navidrome.
    pub streams: Option<WidgetStreams>,
    pub services: Vec<WidgetService>,
}

/// Everything the iPhone widgets show, in one call.
#[utoipa::path(get, path = "/widget", tag = "overview", operation_id = "widget",
    responses((status = 200, body = WidgetSummary)))]
async fn widget(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<WidgetSummary>> {
    let ov = super::overview::build(&state, &user).await?;
    let rows = state.db.list_services().await?;
    let mut downloads: Option<WidgetDownloads> = None;
    let mut streams: Option<WidgetStreams> = None;
    let mut all_torrents = Vec::new();
    for r in rows.iter().filter(|r| r.enabled && user.can_see(&r.id)) {
        let Some(snap) = state.cache.get(&r.id) else {
            continue;
        };
        match (&snap.kind, &snap.data) {
            (ServiceKind::Qbittorrent, ModuleData::Qbittorrent(d)) => {
                let w = downloads.get_or_insert_with(Default::default);
                w.dl_speed += d.dl_speed;
                w.up_speed += d.up_speed;
                w.downloading += d.counts.downloading;
                w.errored += d.counts.errored;
                all_torrents.extend(
                    d.torrents
                        .iter()
                        .filter(|t| t.state == TorrentState::Downloading)
                        .map(|t| WidgetDownload {
                            name: t.name.clone(),
                            progress: t.progress,
                            dl_speed: t.dl_speed,
                            eta_s: t.eta_s,
                        }),
                );
            }
            (ServiceKind::Jellyfin, ModuleData::Jellyfin(d)) => {
                let w = streams.get_or_insert_with(Default::default);
                for s in &d.streams {
                    w.count += 1;
                    let transcoding = s.play_method == "Transcode";
                    w.transcodes += transcoding as u32;
                    w.items.push(WidgetStream {
                        user: s.user.clone(),
                        title: s.title.clone(),
                        subtitle: s.subtitle.clone(),
                        progress: match (s.position_s, s.runtime_s) {
                            (Some(p), Some(rt)) if rt > 0.0 => Some((p / rt).clamp(0.0, 1.0)),
                            _ => None,
                        },
                        paused: s.paused,
                        transcoding,
                        service_name: r.name.clone(),
                    });
                }
            }
            (ServiceKind::Navidrome, ModuleData::Navidrome(d)) => {
                let w = streams.get_or_insert_with(Default::default);
                for n in &d.now_playing {
                    w.count += 1;
                    w.items.push(WidgetStream {
                        user: n.user.clone(),
                        title: n.title.clone(),
                        subtitle: n.artist.clone(),
                        progress: None,
                        paused: false,
                        transcoding: false,
                        service_name: r.name.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    if let Some(w) = downloads.as_mut() {
        all_torrents.sort_by_key(|t| std::cmp::Reverse(t.dl_speed));
        all_torrents.truncate(4);
        w.items = all_torrents;
    }
    if let Some(w) = streams.as_mut() {
        w.items.sort_by_key(|s| s.paused);
        w.items.truncate(4);
    }
    let services = ov
        .highlights
        .iter()
        .map(|h| WidgetService {
            id: h.service.id.clone(),
            name: h.service.name.clone(),
            icon: h.service.icon.clone(),
            health: h.service.health,
            headline: h.headline.clone(),
        })
        .collect();
    Ok(Json(WidgetSummary {
        generated_at: Utc::now(),
        status: ov.status,
        problems: ov.problems,
        nodes: ov.nodes,
        downloads,
        streams,
        services,
    }))
}
