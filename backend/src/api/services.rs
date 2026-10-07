//! Service instances: catalogue, CRUD, connection tests, enable/disable, snapshots and actions.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::{ApiError, ApiResult, ErrorBody};
use crate::auth::CurrentUser;
use crate::db::{ActivityResult, ServiceRow};
use crate::modules::catalogue::{self, CatalogueEntry};
use crate::modules::{
    ActionResult, Category, ConnectionTest, Health, ModuleError, ServiceKind, ServiceSnapshot,
    build_module, validate_config,
};
use crate::services::{self, ServiceView};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_catalogue))
        .routes(routes!(list, create))
        .routes(routes!(test_unsaved))
        .routes(routes!(get_one, update, delete))
        .routes(routes!(test_saved))
        .routes(routes!(enable))
        .routes(routes!(disable))
        .routes(routes!(snapshot))
        .routes(routes!(history))
        .routes(routes!(run_action))
}

fn double_option<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateService {
    /// Slug (`a-z`, `0-9`, `-`). Default: derived from the name.
    pub id: Option<String>,
    pub kind: ServiceKind,
    /// Catalogue preset (icon, accent, category, defaults).
    pub preset: Option<String>,
    pub name: String,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub category: Option<Category>,
    /// Module settings (`ProxmoxSettings`, `HealthCheckSettings`).
    #[schema(value_type = Option<Object>)]
    pub settings: Option<Value>,
    /// Secret values by name (e.g. `token_secret`). Stored encrypted, never returned.
    pub secrets: Option<BTreeMap<String, String>>,
    pub linked_vmid: Option<u32>,
    pub sort_order: Option<i64>,
}

/// Partial update. Absent fields stay as they are; `null` clears optional fields.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct UpdateService {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub base_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<Category>)]
    pub category: Option<Option<Category>>,
    /// Merged into the stored settings; a `null` value removes that key.
    #[schema(value_type = Option<Object>)]
    pub settings: Option<Value>,
    /// Set (string) or clear (`null`) individual secrets.
    #[schema(value_type = Option<BTreeMap<String, String>>)]
    pub secrets: Option<BTreeMap<String, Option<String>>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<u32>)]
    pub linked_vmid: Option<Option<u32>>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ActionRequest {
    /// Required for destructive actions.
    #[serde(default)]
    pub confirm: bool,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub args: Value,
}

fn is_slug(id: &str) -> bool {
    (1..=40).contains(&id.len())
        && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_end_matches('-').chars().take(36).collect();
    if slug.is_empty() {
        "service".into()
    } else {
        slug
    }
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError::BadRequest(msg.into())
}

/// Build a new service row from a create request: id, preset defaults, encrypted secrets.
pub fn new_row(
    state: &AppState,
    req: CreateService,
    existing: &[ServiceRow],
) -> ApiResult<ServiceRow> {
    let name = req.name.trim().to_owned();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(bad("name: 1–80 characters"));
    }
    let id = match req.id {
        Some(id) => {
            if !is_slug(&id) {
                return Err(bad("id: 1–40 characters, a-z, 0-9 and '-'"));
            }
            if existing.iter().any(|s| s.id == id) {
                return Err(ApiError::Conflict(format!(
                    "a service with id '{id}' exists"
                )));
            }
            id
        }
        None => {
            let base = slugify(&name);
            let mut id = base.clone();
            let mut n = 2;
            while existing.iter().any(|s| s.id == id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            id
        }
    };
    let preset = match req.preset.as_deref() {
        Some(p) => {
            let entry = catalogue::find(p).ok_or_else(|| bad(format!("unknown preset '{p}'")))?;
            if entry.kind != Some(req.kind) {
                return Err(bad(format!(
                    "preset '{p}' is not a {} service",
                    req.kind.as_str()
                )));
            }
            Some(entry)
        }
        None => None,
    };
    let mut settings = req
        .settings
        .unwrap_or_else(|| Value::Object(Default::default()));
    if !settings.is_object() {
        return Err(bad("settings must be an object"));
    }
    if let (Some(path), ServiceKind::HealthCheck) = (preset.and_then(|p| p.health_path), req.kind) {
        settings
            .as_object_mut()
            .expect("object")
            .entry("path")
            .or_insert(Value::from(path));
    }
    let mut row = ServiceRow {
        id,
        kind: req.kind.as_str().to_owned(),
        preset: preset.map(|p| p.preset.to_owned()),
        name,
        base_url: req
            .base_url
            .map(|u| u.trim().trim_end_matches('/').to_owned())
            .filter(|u| !u.is_empty()),
        enabled: req.enabled.unwrap_or(true),
        category: req.category.map(category_str),
        settings,
        secrets: BTreeMap::new(),
        linked_vmid: req.linked_vmid,
        sort_order: req.sort_order.unwrap_or(existing.len() as i64),
    };
    for (key, value) in req.secrets.unwrap_or_default() {
        set_secret(state, &mut row, req.kind, &key, Some(value))?;
    }
    check(state, &row)?;
    Ok(row)
}

fn category_str(c: Category) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn set_secret(
    state: &AppState,
    row: &mut ServiceRow,
    kind: ServiceKind,
    key: &str,
    value: Option<String>,
) -> ApiResult<()> {
    if !kind.secret_names().contains(&key) {
        return Err(bad(format!("unknown secret '{key}' for {}", kind.as_str())));
    }
    match value.filter(|v| !v.is_empty()) {
        Some(v) => {
            row.secrets.insert(
                key.to_owned(),
                services::seal(&state.secrets, &row.id, key, &v),
            );
        }
        None => {
            row.secrets.remove(key);
        }
    }
    Ok(())
}

fn check(state: &AppState, row: &ServiceRow) -> ApiResult<()> {
    let cfg = services::instance_config(row, &state.secrets)?;
    validate_config(&cfg).map_err(ApiError::BadRequest)
}

fn apply_update(state: &AppState, row: &mut ServiceRow, req: UpdateService) -> ApiResult<()> {
    let kind = ServiceKind::parse(&row.kind).ok_or_else(|| bad("unknown service kind"))?;
    if let Some(name) = req.name {
        let name = name.trim().to_owned();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(bad("name: 1–80 characters"));
        }
        row.name = name;
    }
    if let Some(url) = req.base_url {
        row.base_url = url
            .map(|u| u.trim().trim_end_matches('/').to_owned())
            .filter(|u| !u.is_empty());
    }
    if let Some(category) = req.category {
        row.category = category.map(category_str);
    }
    if let Some(patch) = req.settings {
        let patch = patch
            .as_object()
            .ok_or_else(|| bad("settings must be an object"))?;
        let settings = row
            .settings
            .as_object_mut()
            .ok_or_else(|| bad("stored settings invalid"))?;
        for (k, v) in patch {
            if v.is_null() {
                settings.remove(k);
            } else {
                settings.insert(k.clone(), v.clone());
            }
        }
    }
    for (key, value) in req.secrets.unwrap_or_default() {
        set_secret(state, row, kind, &key, value)?;
    }
    if let Some(vmid) = req.linked_vmid {
        row.linked_vmid = vmid;
    }
    if let Some(order) = req.sort_order {
        row.sort_order = order;
    }
    check(state, row)
}

async fn find(state: &AppState, user: &CurrentUser, id: &str) -> ApiResult<ServiceRow> {
    state
        .db
        .service(id)
        .await?
        .filter(|s| user.can_see(&s.id))
        .ok_or_else(|| ApiError::NotFound(format!("no service '{id}'")))
}

async fn view_of(state: &AppState, id: &str) -> ApiResult<ServiceView> {
    let rows = state.db.list_services().await?;
    let row = rows
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| ApiError::NotFound(format!("no service '{id}'")))?;
    let guests = services::guests_with_links(state, &rows)
        .map(|(_, g)| g)
        .unwrap_or_default();
    let links = services::links(&rows, &guests);
    Ok(services::view(
        row,
        state.cache.get(id).as_ref(),
        &guests,
        &links,
    ))
}

/// The module catalogue (icons, accents, categories, available kinds).
#[utoipa::path(get, path = "/services/catalogue", tag = "services",
    responses((status = 200, body = Vec<CatalogueEntry>)))]
async fn get_catalogue(_user: CurrentUser) -> Json<&'static [CatalogueEntry]> {
    Json(catalogue::CATALOGUE)
}

/// Services you can see, with live health.
#[utoipa::path(get, path = "/services", tag = "services", operation_id = "list_services",
    responses((status = 200, body = Vec<ServiceView>)))]
async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<ServiceView>>> {
    Ok(Json(services::visible_views(&state, &user).await?))
}

/// Add a service. Owner only. Starts polling when enabled.
#[utoipa::path(post, path = "/services", tag = "services",
    request_body = CreateService,
    responses((status = 201, body = ServiceView), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<CreateService>,
) -> ApiResult<(StatusCode, Json<ServiceView>)> {
    user.require_owner()?;
    let existing = state.db.list_services().await?;
    let row = new_row(&state, req, &existing)?;
    state.db.save_service(&row).await?;
    if row.enabled {
        state.start_service(&row);
    }
    state
        .log(
            Some(&user.user),
            "service.create",
            &format!("service:{}", row.id),
            ActivityResult::Ok,
            Some(&row.name),
            Some(&row.id),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(view_of(&state, &row.id).await?)))
}

/// Test a configuration before saving it. Owner only.
#[utoipa::path(post, path = "/services/test", tag = "services",
    request_body = CreateService,
    responses((status = 200, body = ConnectionTest), (status = 400, body = ErrorBody)))]
async fn test_unsaved(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(mut req): Json<CreateService>,
) -> ApiResult<Json<ConnectionTest>> {
    user.require_owner()?;
    req.id = Some("connection-test".into());
    let row = new_row(&state, req, &[])?;
    Ok(Json(test_row(&state, &row).await?))
}

async fn test_row(state: &AppState, row: &ServiceRow) -> ApiResult<ConnectionTest> {
    let cfg = services::instance_config(row, &state.secrets)?;
    let module = build_module(&cfg, state.demo).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(module.test_connection().await)
}

/// One service.
#[utoipa::path(get, path = "/services/{id}", tag = "services",
    params(("id" = String, Path)),
    responses((status = 200, body = ServiceView), (status = 404, body = ErrorBody)))]
async fn get_one(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ServiceView>> {
    find(&state, &user, &id).await?;
    Ok(Json(view_of(&state, &id).await?))
}

/// Change a service. Owner only. The poller restarts with the new config.
#[utoipa::path(patch, path = "/services/{id}", tag = "services",
    params(("id" = String, Path)),
    request_body = UpdateService,
    responses((status = 200, body = ServiceView), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateService>,
) -> ApiResult<Json<ServiceView>> {
    user.require_owner()?;
    let mut row = find(&state, &user, &id).await?;
    apply_update(&state, &mut row, req)?;
    state.db.save_service(&row).await?;
    if row.enabled {
        state.start_service(&row);
    }
    state
        .log(
            Some(&user.user),
            "service.update",
            &format!("service:{id}"),
            ActivityResult::Ok,
            None,
            Some(&id),
        )
        .await?;
    Ok(Json(view_of(&state, &id).await?))
}

/// Remove a service. Owner only.
#[utoipa::path(delete, path = "/services/{id}", tag = "services",
    params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    user.require_owner()?;
    let row = find(&state, &user, &id).await?;
    state.supervisor.stop(&id);
    state.db.delete_service(&id).await?;
    state
        .log(
            Some(&user.user),
            "service.delete",
            &format!("service:{id}"),
            ActivityResult::Ok,
            Some(&row.name),
            Some(&id),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Test a saved service, optionally with unsaved changes applied. Owner only.
#[utoipa::path(post, path = "/services/{id}/test", tag = "services",
    params(("id" = String, Path)),
    request_body = UpdateService,
    responses((status = 200, body = ConnectionTest), (status = 404, body = ErrorBody)))]
async fn test_saved(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateService>,
) -> ApiResult<Json<ConnectionTest>> {
    user.require_owner()?;
    let mut row = find(&state, &user, &id).await?;
    apply_update(&state, &mut row, req)?;
    Ok(Json(test_row(&state, &row).await?))
}

async fn set_enabled(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    enabled: bool,
) -> ApiResult<Json<ServiceView>> {
    user.require_owner()?;
    let mut row = find(state, user, id).await?;
    row.enabled = enabled;
    state.db.save_service(&row).await?;
    if enabled {
        state.start_service(&row);
    } else {
        state.supervisor.stop(id);
    }
    let action = if enabled {
        "service.enable"
    } else {
        "service.disable"
    };
    state
        .log(
            Some(&user.user),
            action,
            &format!("service:{id}"),
            ActivityResult::Ok,
            None,
            Some(id),
        )
        .await?;
    Ok(Json(view_of(state, id).await?))
}

/// Enable a service and start polling it. Owner only.
#[utoipa::path(post, path = "/services/{id}/enable", tag = "services",
    params(("id" = String, Path)),
    responses((status = 200, body = ServiceView)))]
async fn enable(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ServiceView>> {
    set_enabled(&state, &user, &id, true).await
}

/// Disable a service: stop polling and hide it from Home. Owner only.
#[utoipa::path(post, path = "/services/{id}/disable", tag = "services",
    params(("id" = String, Path)),
    responses((status = 200, body = ServiceView)))]
async fn disable(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ServiceView>> {
    set_enabled(&state, &user, &id, false).await
}

/// The cached snapshot (health + module data). Viewing it switches the poller to fast mode.
#[utoipa::path(get, path = "/services/{id}/snapshot", tag = "services",
    params(("id" = String, Path)),
    responses((status = 200, body = ServiceSnapshot), (status = 404, body = ErrorBody)))]
async fn snapshot(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ServiceSnapshot>> {
    find(&state, &user, &id).await?;
    state.supervisor.watch(&id);
    state
        .cache
        .get(&id)
        .map(Json)
        .ok_or_else(|| ApiError::NotFound("no data yet (disabled or first poll pending)".into()))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HistoryPoint {
    /// Start of the minute.
    pub at: DateTime<Utc>,
    /// Worst state in that minute (`healthy`, `warning` or `down`).
    pub health: Health,
    pub response_ms: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Incident {
    pub start: DateTime<Utc>,
    /// `None` while still down.
    pub end: Option<DateTime<Utc>>,
    pub minutes: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceHistory {
    /// Per-minute samples for the last 24 h, oldest first.
    pub points: Vec<HistoryPoint>,
    /// Percent, one decimal.
    pub uptime_24h: Option<f64>,
    pub uptime_7d: Option<f64>,
    /// Latest run of `down` minutes in the last 7 days.
    pub last_incident: Option<Incident>,
}

fn state_health(state: i64) -> Health {
    match state {
        2 => Health::Healthy,
        1 => Health::Warning,
        _ => Health::Down,
    }
}

fn last_incident(points: &[(i64, i64, Option<i64>)]) -> Option<Incident> {
    let last_down = points.iter().rposition(|p| p.1 == 0)?;
    let mut first = last_down;
    while first > 0 && points[first - 1].1 == 0 && points[first].0 - points[first - 1].0 <= 120 {
        first -= 1;
    }
    let ongoing = last_down == points.len() - 1;
    let start = points[first].0;
    let end = points[last_down].0 + 60;
    Some(Incident {
        start: crate::db::ts(start),
        end: (!ongoing).then(|| crate::db::ts(end)),
        minutes: (end - start) / 60,
    })
}

fn pct(points: &[&(i64, i64, Option<i64>)]) -> Option<f64> {
    let up = points.iter().filter(|p| p.1 > 0).count() as f64;
    (!points.is_empty()).then(|| (up / points.len() as f64 * 1000.0).round() / 10.0)
}

/// Per-minute health and response time for the last 24 h, uptime and the last incident.
#[utoipa::path(get, path = "/services/{id}/history", tag = "services",
    params(("id" = String, Path)),
    responses((status = 200, body = ServiceHistory), (status = 404, body = ErrorBody)))]
async fn history(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ServiceHistory>> {
    find(&state, &user, &id).await?;
    let now = crate::db::now();
    let week = state
        .db
        .uptime_points(&format!("svc:{id}"), now - 7 * 86_400)
        .await?;
    let day: Vec<_> = week.iter().filter(|p| p.0 >= now - 86_400).collect();
    Ok(Json(ServiceHistory {
        points: day
            .iter()
            .map(|p| HistoryPoint {
                at: crate::db::ts(p.0),
                health: state_health(p.1),
                response_ms: p.2,
            })
            .collect(),
        uptime_24h: pct(&day),
        uptime_7d: pct(&week.iter().collect::<Vec<_>>()),
        last_incident: last_incident(&week),
    }))
}

/// Run a module action (e.g. `scan_library`). `refresh` (poll now) works for every service.
#[utoipa::path(post, path = "/services/{id}/actions/{action}", tag = "services",
    params(("id" = String, Path), ("action" = String, Path)),
    request_body = ActionRequest,
    responses(
        (status = 200, body = ActionResult),
        (status = 400, body = ErrorBody, description = "Unknown action or confirm required"),
        (status = 403, body = ErrorBody),
    ))]
async fn run_action(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, action)): Path<(String, String)>,
    Json(req): Json<ActionRequest>,
) -> ApiResult<Json<ActionResult>> {
    find(&state, &user, &id).await?;
    if !user.can_act(&id) {
        return Err(ApiError::Forbidden);
    }
    if action == "refresh" {
        state.supervisor.wake(&id);
        return Ok(Json(ActionResult {
            ok: true,
            message: "refreshing".into(),
            task: None,
        }));
    }
    let module = state
        .supervisor
        .module(&id)
        .ok_or_else(|| ApiError::Conflict("service is disabled".into()))?;
    let descriptor = module
        .actions()
        .into_iter()
        .find(|a| a.id == action)
        .ok_or_else(|| bad(format!("unknown action '{action}'")))?;
    if descriptor.destructive && !req.confirm {
        return Err(ApiError::ConfirmRequired);
    }
    let target = format!("service:{id}");
    let log_action = format!("service.action.{action}");
    let entry = state
        .log(
            Some(&user.user),
            &log_action,
            &target,
            ActivityResult::Pending,
            None,
            Some(&id),
        )
        .await?;
    match module.run_action(&action, req.args).await {
        Ok(result) => {
            let r = if result.ok {
                ActivityResult::Ok
            } else {
                ActivityResult::Error
            };
            state
                .finish(entry.id, r, Some(&result.message), Some(&id))
                .await;
            state.supervisor.wake(&id);
            Ok(Json(result))
        }
        Err(e) => {
            state
                .finish(
                    entry.id,
                    ActivityResult::Error,
                    Some(&e.to_string()),
                    Some(&id),
                )
                .await;
            Err(match e {
                ModuleError::UnknownAction(_) | ModuleError::InvalidArgs(_) => bad(e.to_string()),
                ModuleError::Failed(e) => ApiError::Upstream(e.to_string()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incidents() {
        let p = |m: i64, s: i64| (m * 60, s, None);
        assert!(last_incident(&[p(1, 2), p(2, 2)]).is_none());
        let i = last_incident(&[p(1, 0), p(2, 2), p(3, 0), p(4, 0), p(5, 2)]).unwrap();
        assert_eq!(
            (i.start.timestamp(), i.minutes, i.end.is_some()),
            (180, 2, true)
        );
        let ongoing = last_incident(&[p(1, 2), p(2, 0)]).unwrap();
        assert!(ongoing.end.is_none());
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("Jellyfin (public)"), "jellyfin-public");
        assert_eq!(slugify("  !!"), "service");
        assert!(is_slug("jellyfin-pub"));
        assert!(!is_slug("-x"));
        assert!(!is_slug("Upper"));
    }
}
