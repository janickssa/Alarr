//! Alerts: history, per-type settings (owner), a test send, and per-user/device mutes.

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::{ApiError, ApiResult};
use crate::alerts::channels::{CHANNEL_IDS, ChannelInfo, NotConfigured};
use crate::alerts::{AlertType, Engine, TypeSettings, load_settings};
use crate::auth::{AuthVia, CurrentUser};
use crate::db::{ActivityResult, Alert, Role};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_alerts))
        .routes(routes!(get_alert_settings, put_alert_settings))
        .routes(routes!(test_alert))
        .routes(routes!(get_mutes, put_mutes))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AlertsQuery {
    /// 1–200, default 50.
    pub limit: Option<i64>,
    /// Return alerts older than this id (pagination).
    pub before: Option<i64>,
    /// Only unresolved alerts.
    #[serde(default)]
    pub active: bool,
}

/// Visible to the user: guests only see alerts of services they were given.
fn visible(user: &CurrentUser, a: &Alert) -> bool {
    match &a.service_id {
        Some(id) => user.can_see(id),
        None => user.user.role != Role::Guest,
    }
}

/// Alerts, newest first.
#[utoipa::path(get, path = "/alerts", tag = "alerts", operation_id = "list_alerts",
    params(AlertsQuery),
    responses((status = 200, body = Vec<Alert>)))]
async fn list_alerts(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AlertsQuery>,
) -> ApiResult<Json<Vec<Alert>>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let mut out = state.db.list_alerts(limit, q.before, q.active).await?;
    out.retain(|a| visible(&user, a));
    Ok(Json(out))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AlertTypeView {
    #[serde(flatten)]
    pub settings: TypeSettings,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AlertSettingsView {
    pub types: Vec<AlertTypeView>,
    pub channels: Vec<ChannelInfo>,
    /// Quiet hours use this time zone (the server's local time).
    pub timezone: String,
}

async fn settings_view(state: &AppState) -> ApiResult<AlertSettingsView> {
    let mut settings = load_settings(state).await?;
    Ok(AlertSettingsView {
        types: AlertType::ALL
            .into_iter()
            .filter_map(|t| {
                Some(AlertTypeView {
                    settings: settings.remove(&t)?,
                    label: t.label().into(),
                    description: t.description().into(),
                })
            })
            .collect(),
        channels: state.channels.iter().map(|c| c.info()).collect(),
        timezone: chrono::Local::now().format("%:z").to_string(),
    })
}

/// Per-type alert settings and the channels.
#[utoipa::path(get, path = "/alerts/settings", tag = "alerts", operation_id = "get_alert_settings",
    responses((status = 200, body = AlertSettingsView)))]
async fn get_alert_settings(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> ApiResult<Json<AlertSettingsView>> {
    Ok(Json(settings_view(&state).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AlertSettingsUpdate {
    /// Types to change; types not listed keep their settings.
    pub types: Vec<TypeSettings>,
}

/// Change per-type settings (owner).
#[utoipa::path(put, path = "/alerts/settings", tag = "alerts", operation_id = "put_alert_settings",
    request_body = AlertSettingsUpdate,
    responses((status = 200, body = AlertSettingsView), (status = 400, description = "Invalid settings")))]
async fn put_alert_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<AlertSettingsUpdate>,
) -> ApiResult<Json<AlertSettingsView>> {
    user.require_owner()?;
    for t in &body.types {
        if let Some(c) = t
            .channels
            .iter()
            .find(|c| !CHANNEL_IDS.contains(&c.as_str()))
        {
            return Err(ApiError::BadRequest(format!("unknown channel: {c}")));
        }
        if let Some(q) = &t.quiet_hours {
            q.validate().map_err(ApiError::BadRequest)?;
        }
    }
    for t in &body.types {
        let mut t = t.clone();
        t.channels.sort();
        t.channels.dedup();
        state.db.save_alert_setting(&t.to_row()).await?;
    }
    let changed: Vec<&str> = body.types.iter().map(|t| t.alert_type.as_str()).collect();
    state
        .log(
            Some(&user.user),
            "alerts.settings",
            "alerts",
            ActivityResult::Ok,
            Some(&changed.join(", ")),
            None,
        )
        .await?;
    Ok(Json(settings_view(&state).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TestAlert {
    /// `ntfy` or `push`.
    pub channel: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TestAlertResult {
    pub ok: bool,
    pub message: String,
}

/// Send a test notification on one channel (owner).
#[utoipa::path(post, path = "/alerts/test", tag = "alerts", operation_id = "test_alert",
    request_body = TestAlert,
    responses((status = 200, body = TestAlertResult)))]
async fn test_alert(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<TestAlert>,
) -> ApiResult<Json<TestAlertResult>> {
    user.require_owner()?;
    if !CHANNEL_IDS.contains(&body.channel.as_str()) {
        return Err(ApiError::BadRequest("unknown channel".into()));
    }
    let entry = state
        .log(
            Some(&user.user),
            "alerts.test",
            &body.channel,
            ActivityResult::Pending,
            None,
            None,
        )
        .await?;
    let result = match Engine::test(&state, &body.channel).await {
        Ok(()) => TestAlertResult {
            ok: true,
            message: "Sent".into(),
        },
        Err(e) if e.downcast_ref::<NotConfigured>().is_some() => TestAlertResult {
            ok: false,
            message: "This channel is not configured".into(),
        },
        Err(e) => TestAlertResult {
            ok: false,
            message: e.to_string(),
        },
    };
    state
        .finish(
            entry.id,
            if result.ok {
                ActivityResult::Ok
            } else {
                ActivityResult::Error
            },
            Some(&result.message),
            None,
        )
        .await;
    Ok(Json(result))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Mutes {
    /// Muted alert types. From a paired device these are the device's own mutes; from the
    /// web, the user's (which also apply to all their devices).
    pub types: Vec<AlertType>,
}

fn device_of(user: &CurrentUser) -> Option<i64> {
    match user.via {
        AuthVia::Device(id) => Some(id),
        AuthVia::Session(_) => None,
    }
}

/// Alert types the signed-in user (or this device) muted.
#[utoipa::path(get, path = "/alerts/mutes", tag = "alerts", operation_id = "get_alert_mutes",
    responses((status = 200, body = Mutes)))]
async fn get_mutes(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Mutes>> {
    let types = state
        .db
        .alert_mutes(user.user.id, device_of(&user))
        .await?
        .iter()
        .filter_map(|t| AlertType::parse(t))
        .collect();
    Ok(Json(Mutes { types }))
}

/// Replace the muted alert types for the signed-in user (or this device).
#[utoipa::path(put, path = "/alerts/mutes", tag = "alerts", operation_id = "put_alert_mutes",
    request_body = Mutes,
    responses((status = 200, body = Mutes)))]
async fn put_mutes(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<Mutes>,
) -> ApiResult<Json<Mutes>> {
    let mut types: Vec<String> = body.types.iter().map(|t| t.as_str().to_owned()).collect();
    types.sort();
    types.dedup();
    state
        .db
        .set_alert_mutes(user.user.id, device_of(&user), &types)
        .await?;
    get_mutes(State(state), user).await
}
