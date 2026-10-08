//! Home overview and the Health page.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::ApiResult;
use super::settings::load_settings;
use crate::auth::CurrentUser;
use crate::db::{UptimeStats, ts};
use crate::modules::proxmox::{GuestStatus, GuestType};
use crate::modules::{Health, ModuleData, ServiceKind};
use crate::services::{self, ServiceView};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(overview))
        .routes(routes!(health))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StatusSummary {
    /// Enabled services you can see.
    pub total: u32,
    pub healthy: u32,
    pub warning: u32,
    pub down: u32,
    pub unknown: u32,
    /// "All 18 services healthy" or "2 problems".
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionKind {
    /// Poll the service again (`POST /services/{id}/actions/refresh`).
    Retry,
    /// Start the stopped linked guest (`POST /proxmox/guests/{vmid}/power`).
    StartGuest,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QuickAction {
    pub kind: QuickActionKind,
    pub label: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Problem {
    pub service_id: Option<String>,
    pub vmid: Option<u32>,
    pub title: String,
    pub reason: Option<String>,
    pub health: Health,
    pub quick_action: Option<QuickAction>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NodeSummary {
    pub name: String,
    pub online: bool,
    /// 0.0–1.0.
    pub cpu_usage: Option<f64>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub guests_running: u32,
}

/// One card on Home.
#[derive(Debug, Serialize, ToSchema)]
pub struct Highlight {
    pub service: ServiceView,
    /// Main line, e.g. "3 nodes · 24 guests running" or "42 ms".
    pub headline: String,
    /// Second line, e.g. "CPU 18 % · RAM 58 %" or "99.9 % · 7 d".
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Overview {
    pub status: StatusSummary,
    pub problems: Vec<Problem>,
    /// Proxmox node strip (empty without a visible Proxmox service).
    pub nodes: Vec<NodeSummary>,
    /// Enabled, visible services in Home order, without hidden ones.
    pub highlights: Vec<Highlight>,
}

fn pct(v: f64) -> String {
    format!("{:.0} %", v * 100.0)
}

fn uptime_pct(up: i64, total: i64) -> Option<f64> {
    (total > 0).then(|| (up as f64 / total as f64 * 1000.0).round() / 10.0)
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Everything Home needs in one call.
#[utoipa::path(get, path = "/overview", tag = "overview",
    responses((status = 200, body = Overview)))]
async fn overview(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Overview>> {
    Ok(Json(build(&state, &user).await?))
}

/// The Home overview for one user (also used by `GET /widget`).
pub async fn build(state: &AppState, user: &CurrentUser) -> ApiResult<Overview> {
    let views = services::visible_views(state, user).await?;
    let stats = state.db.uptime_stats(crate::db::now()).await?;
    let settings = load_settings(state).await?;
    let rows = state.db.list_services().await?;
    let guests = services::guests_with_links(state, &rows)
        .filter(|(pve, _)| user.can_see(pve))
        .map(|(_, g)| g)
        .unwrap_or_default();

    let enabled: Vec<&ServiceView> = views.iter().filter(|v| v.enabled).collect();
    let count = |h: Health| enabled.iter().filter(|v| v.health == h).count() as u32;
    let (healthy, warning, down) = (
        count(Health::Healthy),
        count(Health::Warning),
        count(Health::Down),
    );
    let problems_n = warning + down;
    let total = enabled.len() as u32;
    let status = StatusSummary {
        total,
        healthy,
        warning,
        down,
        unknown: total - healthy - warning - down,
        message: if problems_n == 0 && total == 1 {
            "1 service healthy".to_owned()
        } else if problems_n == 0 {
            format!("All {} healthy", plural(total, "service", "services"))
        } else {
            plural(problems_n, "problem", "problems")
        },
    };

    let mut problems: Vec<Problem> = enabled
        .iter()
        .filter(|v| v.health.is_problem())
        .map(|v| {
            let stopped_guest = v
                .linked_guest
                .as_ref()
                .filter(|g| g.status == GuestStatus::Stopped);
            Problem {
                service_id: Some(v.id.clone()),
                vmid: v.linked_guest.as_ref().map(|g| g.vmid),
                title: v.name.clone(),
                reason: v.health_reason.clone(),
                health: v.health,
                quick_action: Some(match stopped_guest {
                    Some(g) => QuickAction {
                        kind: QuickActionKind::StartGuest,
                        label: format!(
                            "Start {} {}",
                            if g.guest_type == GuestType::Lxc {
                                "CT"
                            } else {
                                "VM"
                            },
                            g.vmid
                        ),
                    },
                    None => QuickAction {
                        kind: QuickActionKind::Retry,
                        label: "Retry".into(),
                    },
                }),
            }
        })
        .collect();
    problems.sort_by_key(|p| if p.health == Health::Down { 0 } else { 1 });

    let pve_data = rows
        .iter()
        .find(|r| r.enabled && r.kind == ServiceKind::Proxmox.as_str() && user.can_see(&r.id))
        .and_then(|r| state.cache.get(&r.id));
    let nodes = pve_data
        .as_ref()
        .and_then(services::proxmox_data)
        .map(|d| {
            d.nodes
                .iter()
                .map(|n| NodeSummary {
                    name: n.name.clone(),
                    online: n.online,
                    cpu_usage: n.cpu_usage,
                    mem_used: n.mem_used,
                    mem_total: n.mem_total,
                    guests_running: guests
                        .iter()
                        .filter(|g| g.node == n.name && g.status == GuestStatus::Running)
                        .count() as u32,
                })
                .collect()
        })
        .unwrap_or_default();

    let order: HashMap<&str, usize> = settings
        .home_order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let mut cards: Vec<&ServiceView> = enabled
        .into_iter()
        .filter(|v| !settings.home_hidden.contains(&v.id))
        .collect();
    cards.sort_by_key(|v| {
        (
            order.get(v.id.as_str()).copied().unwrap_or(usize::MAX),
            v.sort_order,
        )
    });
    let highlights = cards
        .into_iter()
        .map(|v| highlight(state, v, stats.get(&format!("svc:{}", v.id))))
        .collect();

    Ok(Overview {
        status,
        problems,
        nodes,
        highlights,
    })
}

fn highlight(state: &AppState, v: &ServiceView, stats: Option<&UptimeStats>) -> Highlight {
    let snapshot = state.cache.get(&v.id);
    if let Some((headline, detail)) = snapshot.as_ref().and_then(|s| s.data.headline()) {
        return Highlight {
            service: v.clone(),
            headline,
            detail,
        };
    }
    let (headline, detail) = match snapshot.as_ref().map(|s| &s.data) {
        Some(ModuleData::Proxmox(d)) => {
            let running = d
                .guests
                .iter()
                .filter(|g| g.status == GuestStatus::Running)
                .count();
            let online: Vec<_> = d.nodes.iter().filter(|n| n.online).collect();
            let cpu =
                online.iter().filter_map(|n| n.cpu_usage).sum::<f64>() / online.len().max(1) as f64;
            let (used, total) = online.iter().fold((0u64, 0u64), |(u, t), n| {
                (u + n.mem_used.unwrap_or(0), t + n.mem_total.unwrap_or(0))
            });
            (
                format!(
                    "{} · {} running",
                    plural(d.cluster.nodes_total, "node", "nodes"),
                    plural(running as u32, "guest", "guests")
                ),
                Some(format!(
                    "CPU {} · RAM {}",
                    pct(cpu),
                    pct(if total > 0 {
                        used as f64 / total as f64
                    } else {
                        0.0
                    })
                )),
            )
        }
        _ => (
            match (v.health, v.response_ms) {
                (Health::Down, _) => v.health_reason.clone().unwrap_or_else(|| "Down".into()),
                (_, Some(ms)) => format!("{ms} ms"),
                _ => "Waiting for first check".into(),
            },
            stats
                .and_then(|s| uptime_pct(s.up_7d, s.total_7d))
                .map(|p| format!("{p} % · 7 d")),
        ),
    };
    Highlight {
        service: v.clone(),
        headline,
        detail,
    }
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthEntryKind {
    Service,
    Guest,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TimelineBucket {
    pub start: DateTime<Utc>,
    /// Worst state seen in the hour; `unknown` without samples.
    pub health: Health,
    /// Uptime in the hour, percent.
    pub uptime: Option<f64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthEntry {
    /// `svc:<id>` or `guest:<vmid>`.
    pub id: String,
    pub kind: HealthEntryKind,
    pub service_id: Option<String>,
    pub vmid: Option<u32>,
    pub guest_type: Option<GuestType>,
    pub name: String,
    pub health: Health,
    pub health_reason: Option<String>,
    pub response_ms: Option<u64>,
    /// Percent, one decimal.
    pub uptime_24h: Option<f64>,
    pub uptime_7d: Option<f64>,
    /// 24 hourly buckets, oldest first.
    pub timeline: Vec<TimelineBucket>,
    /// e.g. "disk at 92 %".
    pub warnings: Vec<String>,
    /// Guests: latest backup in the last 35 days.
    pub backup: Option<crate::modules::proxmox::backups::GuestBackup>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthReport {
    pub generated_at: DateTime<Utc>,
    pub entries: Vec<HealthEntry>,
}

fn timeline(stats: Option<&UptimeStats>, now: i64) -> Vec<TimelineBucket> {
    let current = now / 3600 * 3600;
    (0..24)
        .rev()
        .map(|i| {
            let start = current - i * 3600;
            let bucket = stats.and_then(|s| s.hours.get(&start));
            TimelineBucket {
                start: ts(start),
                health: match bucket.map(|b| b.0) {
                    Some(2) => Health::Healthy,
                    Some(1) => Health::Warning,
                    Some(_) => Health::Down,
                    None => Health::Unknown,
                },
                uptime: bucket.and_then(|b| uptime_pct(b.1, b.2)),
            }
        })
        .collect()
}

/// Every service and guest with status, response time, uptime and a 24 h timeline.
#[utoipa::path(get, path = "/health", tag = "overview",
    responses((status = 200, body = HealthReport)))]
async fn health(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<HealthReport>> {
    let now = crate::db::now();
    let stats = state.db.uptime_stats(now).await?;
    let views = services::visible_views(&state, &user).await?;
    let rows = state.db.list_services().await?;
    let mut entries: Vec<HealthEntry> = views
        .iter()
        .map(|v| {
            let s = stats.get(&format!("svc:{}", v.id));
            HealthEntry {
                id: format!("svc:{}", v.id),
                kind: HealthEntryKind::Service,
                service_id: Some(v.id.clone()),
                vmid: v.linked_guest.as_ref().map(|g| g.vmid),
                guest_type: None,
                name: v.name.clone(),
                health: v.health,
                health_reason: v.health_reason.clone(),
                response_ms: v.response_ms,
                uptime_24h: s.and_then(|s| uptime_pct(s.up_24h, s.total_24h)),
                uptime_7d: s.and_then(|s| uptime_pct(s.up_7d, s.total_7d)),
                timeline: timeline(s, now),
                warnings: Vec::new(),
                backup: None,
            }
        })
        .collect();
    if let Some((pve, guests)) = services::guests_with_links(&state, &rows)
        && user.can_see(&pve)
    {
        let mut backups = super::proxmox::guest_backups(&state, &pve).await;
        for g in guests.iter().filter(|g| !g.template) {
            let s = stats.get(&format!("guest:{}", g.vmid));
            let mut warnings = Vec::new();
            let backup = backups.remove(&g.vmid);
            if backup.as_ref().is_some_and(|b| !b.ok) {
                warnings.push("last backup failed".into());
            }
            if let (Some(used), Some(total)) = (g.disk_used, g.disk_total.filter(|t| *t > 0)) {
                let usage = used as f64 / total as f64;
                if usage >= 0.8 {
                    warnings.push(format!("disk at {}", pct(usage)));
                }
            }
            entries.push(HealthEntry {
                id: format!("guest:{}", g.vmid),
                kind: HealthEntryKind::Guest,
                service_id: g.linked_services.first().cloned(),
                vmid: Some(g.vmid),
                guest_type: Some(g.guest_type),
                name: g.name.clone(),
                health: g.status.health(),
                health_reason: (g.status != GuestStatus::Running)
                    .then(|| format!("{:?}", g.status).to_lowercase()),
                response_ms: None,
                uptime_24h: s.and_then(|s| uptime_pct(s.up_24h, s.total_24h)),
                uptime_7d: s.and_then(|s| uptime_pct(s.up_7d, s.total_7d)),
                timeline: timeline(s, now),
                warnings,
                backup,
            });
        }
    }
    Ok(Json(HealthReport {
        generated_at: Utc::now(),
        entries,
    }))
}
