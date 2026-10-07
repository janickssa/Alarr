//! Proxmox cluster, nodes, storage, guests, graphs, tasks and power actions.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::{ApiError, ApiResult, ErrorBody};
use crate::auth::CurrentUser;
use crate::db::{ActivityResult, ServiceRow};
use crate::modules::proxmox::api::safe_segment;
use crate::modules::proxmox::{
    ClusterInfo, Guest, GuestType, Node, PowerAction, ProxmoxData, RrdPoint, RrdRange, StoragePool,
    Task, TaskState, upid_node,
};
use crate::modules::{Health, ServiceModule};
use crate::services;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(cluster))
        .routes(routes!(nodes))
        .routes(routes!(node_rrd))
        .routes(routes!(storage))
        .routes(routes!(guests))
        .routes(routes!(guest))
        .routes(routes!(guest_rrd))
        .routes(routes!(power))
        .routes(routes!(task))
}

/// The Proxmox service, its module and current data, checked against the user's access.
struct Pve {
    id: String,
    rows: Vec<ServiceRow>,
    module: Arc<dyn ServiceModule>,
    data: Box<ProxmoxData>,
    health: Health,
    health_reason: Option<String>,
    updated_at: DateTime<Utc>,
}

async fn pve(state: &AppState, user: &CurrentUser) -> ApiResult<Pve> {
    let rows = state.db.list_services().await?;
    let row = services::proxmox_instance(&rows)
        .ok_or_else(|| ApiError::NotFound("Proxmox is not configured".into()))?;
    if !user.can_see(&row.id) {
        return Err(ApiError::Forbidden);
    }
    let id = row.id.clone();
    state.supervisor.watch(&id);
    let module = state.supervisor.module(&id).ok_or_else(|| {
        ApiError::Upstream("Proxmox module is not running (check its configuration)".into())
    })?;
    let snapshot = state
        .cache
        .get(&id)
        .ok_or_else(|| ApiError::Upstream("no Proxmox data yet".into()))?;
    let data = services::proxmox_data(&snapshot).cloned().ok_or_else(|| {
        ApiError::Upstream(
            snapshot
                .health_reason
                .clone()
                .unwrap_or_else(|| "no Proxmox data".into()),
        )
    })?;
    let mut data = Box::new(data);
    let links = services::links(&rows, &data.guests);
    for g in &mut data.guests {
        g.linked_services = links
            .guest_to_services
            .get(&g.vmid)
            .cloned()
            .unwrap_or_default();
    }
    Ok(Pve {
        id,
        rows,
        module,
        data,
        health: snapshot.health,
        health_reason: snapshot.health_reason,
        updated_at: snapshot.updated_at,
    })
}

fn api(p: &Pve) -> ApiResult<&dyn crate::modules::proxmox::api::ProxmoxApi> {
    p.module
        .as_proxmox()
        .map(|m| m.api())
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("not a Proxmox module")))
}

fn upstream(e: anyhow::Error) -> ApiError {
    ApiError::Upstream(crate::modules::describe_error(&e))
}

fn find_guest(p: &Pve, vmid: u32) -> ApiResult<&Guest> {
    p.data
        .guests
        .iter()
        .find(|g| g.vmid == vmid)
        .ok_or_else(|| ApiError::NotFound(format!("no guest {vmid}")))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GuestCounts {
    pub total: u32,
    pub running: u32,
    pub stopped: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ClusterOverview {
    pub service_id: String,
    pub health: Health,
    pub health_reason: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub cluster: ClusterInfo,
    pub nodes: Vec<Node>,
    pub storage: Vec<StoragePool>,
    pub guests: GuestCounts,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct RangeQuery {
    /// `hour` (default), `day` or `week`.
    #[serde(default)]
    pub range: RrdRange,
}

/// Cluster overview: quorum, nodes, storage pools and guest counts.
#[utoipa::path(get, path = "/proxmox/cluster", tag = "proxmox",
    responses((status = 200, body = ClusterOverview), (status = 404, body = ErrorBody), (status = 502, body = ErrorBody)))]
async fn cluster(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<ClusterOverview>> {
    let p = pve(&state, &user).await?;
    let running = p
        .data
        .guests
        .iter()
        .filter(|g| g.status == crate::modules::proxmox::GuestStatus::Running)
        .count() as u32;
    let total = p.data.guests.iter().filter(|g| !g.template).count() as u32;
    Ok(Json(ClusterOverview {
        service_id: p.id,
        health: p.health,
        health_reason: p.health_reason,
        updated_at: p.updated_at,
        cluster: p.data.cluster.clone(),
        nodes: p.data.nodes.clone(),
        storage: p.data.storage.clone(),
        guests: GuestCounts {
            total,
            running,
            stopped: total.saturating_sub(running),
        },
    }))
}

/// Nodes with CPU, memory, disk, load and uptime.
#[utoipa::path(get, path = "/proxmox/nodes", tag = "proxmox",
    responses((status = 200, body = Vec<Node>)))]
async fn nodes(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Vec<Node>>> {
    Ok(Json(pve(&state, &user).await?.data.nodes))
}

/// Node graphs (RRD averages).
#[utoipa::path(get, path = "/proxmox/nodes/{node}/rrd", tag = "proxmox",
    params(("node" = String, Path), RangeQuery),
    responses((status = 200, body = Vec<RrdPoint>)))]
async fn node_rrd(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(node): Path<String>,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<Vec<RrdPoint>>> {
    let p = pve(&state, &user).await?;
    safe_segment(&node).map_err(|_| ApiError::BadRequest("invalid node name".into()))?;
    if !p.data.nodes.iter().any(|n| n.name == node) {
        return Err(ApiError::NotFound(format!("no node '{node}'")));
    }
    let raw = api(&p)?.node_rrd(&node, q.range).await.map_err(upstream)?;
    Ok(Json(raw.iter().filter_map(RrdPoint::from_raw).collect()))
}

/// Storage pools per node.
#[utoipa::path(get, path = "/proxmox/storage", tag = "proxmox",
    responses((status = 200, body = Vec<StoragePool>)))]
async fn storage(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<StoragePool>>> {
    Ok(Json(pve(&state, &user).await?.data.storage))
}

/// All VMs and containers, with linked services.
#[utoipa::path(get, path = "/proxmox/guests", tag = "proxmox",
    responses((status = 200, body = Vec<Guest>)))]
async fn guests(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Vec<Guest>>> {
    Ok(Json(pve(&state, &user).await?.data.guests))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceRef {
    pub id: String,
    pub name: String,
    pub health: Health,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GuestDetail {
    pub guest: Guest,
    /// QEMU guest state (`running`, `paused`, …) from the live status.
    pub qmp_status: Option<String>,
    pub ha_managed: bool,
    pub linked_services: Vec<ServiceRef>,
    /// Latest tasks for this guest (power actions, backups), newest first.
    pub recent_tasks: Vec<Task>,
    /// Latest finished backup (`vzdump`) task.
    pub last_backup: Option<Task>,
}

/// One guest with live status, linked services and recent tasks.
#[utoipa::path(get, path = "/proxmox/guests/{vmid}", tag = "proxmox",
    params(("vmid" = u32, Path)),
    responses((status = 200, body = GuestDetail), (status = 404, body = ErrorBody)))]
async fn guest(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(vmid): Path<u32>,
) -> ApiResult<Json<GuestDetail>> {
    let p = pve(&state, &user).await?;
    let g = find_guest(&p, vmid)?.clone();
    let pa = api(&p)?;
    let (current, tasks) = tokio::join!(
        pa.guest_status(&g.node, g.guest_type, vmid),
        pa.tasks(&g.node, Some(vmid), 20)
    );
    let current = current.map_err(upstream)?;
    let recent_tasks: Vec<Task> = tasks
        .map_err(upstream)?
        .into_iter()
        .map(|t| Task::from_list(&g.node, t))
        .collect();
    let last_backup = recent_tasks
        .iter()
        .find(|t| t.kind.as_deref() == Some("vzdump") && t.state != TaskState::Running)
        .cloned();
    let linked_services = g
        .linked_services
        .iter()
        .filter(|id| user.can_see(id))
        .filter_map(|id| p.rows.iter().find(|r| &r.id == id))
        .map(|r| ServiceRef {
            id: r.id.clone(),
            name: r.name.clone(),
            health: state.cache.get(&r.id).map_or(Health::Unknown, |s| s.health),
        })
        .collect();
    Ok(Json(GuestDetail {
        qmp_status: current.qmpstatus,
        ha_managed: current
            .ha
            .get("managed")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            != 0,
        linked_services,
        recent_tasks: recent_tasks.into_iter().take(10).collect(),
        last_backup,
        guest: g,
    }))
}

/// Guest graphs (RRD averages).
#[utoipa::path(get, path = "/proxmox/guests/{vmid}/rrd", tag = "proxmox",
    params(("vmid" = u32, Path), RangeQuery),
    responses((status = 200, body = Vec<RrdPoint>)))]
async fn guest_rrd(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(vmid): Path<u32>,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<Vec<RrdPoint>>> {
    let p = pve(&state, &user).await?;
    let g = find_guest(&p, vmid)?;
    let raw = api(&p)?
        .guest_rrd(&g.node, g.guest_type, vmid, q.range)
        .await
        .map_err(upstream)?;
    Ok(Json(raw.iter().filter_map(RrdPoint::from_raw).collect()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PowerRequest {
    pub action: PowerAction,
    /// Must be `true` for `shutdown`, `stop` and `reboot`.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PowerStarted {
    pub upid: String,
    pub node: String,
    /// Activity log entry; an `action` event follows when the task finishes.
    pub activity_id: i64,
}

/// Start, shut down, stop (hard) or reboot a guest. Returns the Proxmox task.
#[utoipa::path(post, path = "/proxmox/guests/{vmid}/power", tag = "proxmox",
    params(("vmid" = u32, Path)),
    request_body = PowerRequest,
    responses(
        (status = 200, body = PowerStarted),
        (status = 400, body = ErrorBody, description = "confirm_required, or a template"),
        (status = 403, body = ErrorBody),
        (status = 502, body = ErrorBody, description = "Proxmox refused the action"),
    ))]
async fn power(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(vmid): Path<u32>,
    Json(req): Json<PowerRequest>,
) -> ApiResult<Json<PowerStarted>> {
    let p = pve(&state, &user).await?;
    if !user.can_act(&p.id) {
        return Err(ApiError::Forbidden);
    }
    if req.action.destructive() && !req.confirm {
        return Err(ApiError::ConfirmRequired);
    }
    let g = find_guest(&p, vmid)?.clone();
    if g.template {
        return Err(ApiError::BadRequest(
            "templates cannot be powered on or off".into(),
        ));
    }
    let label = format!(
        "{} {vmid} · {} · {}",
        if g.guest_type == GuestType::Lxc {
            "CT"
        } else {
            "VM"
        },
        g.name,
        g.node
    );
    let action = format!("guest.power.{}", req.action.as_str());
    let target = format!("guest:{vmid}");
    let entry = state
        .log(
            Some(&user.user),
            &action,
            &target,
            ActivityResult::Pending,
            Some(&label),
            Some(&p.id),
        )
        .await?;
    let upid = match api(&p)?
        .power(&g.node, g.guest_type, vmid, req.action)
        .await
    {
        Ok(upid) => upid,
        Err(e) => {
            let msg = crate::modules::describe_error(&e);
            state
                .finish(
                    entry.id,
                    ActivityResult::Error,
                    Some(&format!("{label}: {msg}")),
                    Some(&p.id),
                )
                .await;
            return Err(ApiError::Upstream(msg));
        }
    };
    tokio::spawn(follow_task(
        state.clone(),
        p.id.clone(),
        p.module.clone(),
        g.node.clone(),
        upid.clone(),
        entry.id,
        label,
    ));
    state.supervisor.wake(&p.id);
    Ok(Json(PowerStarted {
        upid,
        node: g.node,
        activity_id: entry.id,
    }))
}

/// Poll a power task until it ends, then complete the activity entry and refresh Proxmox.
async fn follow_task(
    state: AppState,
    service_id: String,
    module: Arc<dyn ServiceModule>,
    node: String,
    upid: String,
    activity_id: i64,
    label: String,
) {
    let Some(pm) = module.as_proxmox() else {
        return;
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
    let (result, detail) = loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        match pm.api().task_status(&node, &upid).await {
            Ok(s) => {
                let t = Task::from_status(&upid, &node, s);
                match t.state {
                    TaskState::Running if tokio::time::Instant::now() < deadline => continue,
                    TaskState::Running => {
                        break (
                            ActivityResult::Error,
                            format!("{label}: still running after 10 min"),
                        );
                    }
                    TaskState::Ok => break (ActivityResult::Ok, label),
                    TaskState::Error => {
                        break (
                            ActivityResult::Error,
                            format!(
                                "{label}: {}",
                                t.exit_status.unwrap_or_else(|| "failed".into())
                            ),
                        );
                    }
                }
            }
            Err(e) if tokio::time::Instant::now() >= deadline => {
                break (
                    ActivityResult::Error,
                    format!("{label}: {}", crate::modules::describe_error(&e)),
                );
            }
            Err(_) => continue,
        }
    };
    state
        .finish(activity_id, result, Some(&detail), Some(&service_id))
        .await;
    state.supervisor.wake(&service_id);
}

/// Status of a Proxmox task (e.g. from a power action).
#[utoipa::path(get, path = "/proxmox/tasks/{upid}", tag = "proxmox",
    params(("upid" = String, Path)),
    responses((status = 200, body = Task), (status = 400, body = ErrorBody)))]
async fn task(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(upid): Path<String>,
) -> ApiResult<Json<Task>> {
    let p = pve(&state, &user).await?;
    let node = upid_node(&upid)
        .filter(|n| safe_segment(n).is_ok() && safe_segment(&upid).is_ok())
        .ok_or_else(|| ApiError::BadRequest("invalid UPID".into()))?
        .to_owned();
    let status = api(&p)?.task_status(&node, &upid).await.map_err(upstream)?;
    Ok(Json(Task::from_status(&upid, &node, status)))
}
