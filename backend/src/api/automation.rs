//! Automation settings: qBittorrent auto-throttle while Jellyfin streams.

use axum::Json;
use axum::extract::State;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::{ApiError, ApiResult};
use crate::auth::CurrentUser;
use crate::automation::{ThrottleSettings, ThrottleStatus, count_streams};
use crate::db::ActivityResult;
use crate::modules::ServiceKind;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_throttle, put_throttle))
}

async fn status(state: &AppState) -> ApiResult<ThrottleStatus> {
    let settings = ThrottleSettings::load(state).await?;
    Ok(ThrottleStatus {
        streams: count_streams(state, settings.only_remote).unwrap_or(0),
        engaged: state.throttle.lock().await.engaged,
        settings,
    })
}

/// Auto-throttle settings and whether it is throttling now.
#[utoipa::path(get, path = "/automation/throttle", tag = "automation", operation_id = "get_throttle",
    responses((status = 200, body = ThrottleStatus)))]
async fn get_throttle(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> ApiResult<Json<ThrottleStatus>> {
    Ok(Json(status(&state).await?))
}

/// Change auto-throttle (owner).
#[utoipa::path(put, path = "/automation/throttle", tag = "automation", operation_id = "put_throttle",
    request_body = ThrottleSettings,
    responses((status = 200, body = ThrottleStatus), (status = 400, description = "Unknown qBittorrent service")))]
async fn put_throttle(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ThrottleSettings>,
) -> ApiResult<Json<ThrottleStatus>> {
    user.require_owner()?;
    if let Some(id) = &body.qbittorrent_id {
        let row = state.db.service(id).await?;
        if row.is_none_or(|r| r.kind != ServiceKind::Qbittorrent.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "{id} is not a qBittorrent service"
            )));
        }
    }
    body.save(&state).await?;
    state
        .log(
            Some(&user.user),
            if body.enabled {
                "automation.throttle.enable"
            } else {
                "automation.throttle.disable"
            },
            "automation",
            ActivityResult::Ok,
            body.only_remote.then_some("only remote streams"),
            None,
        )
        .await?;
    Ok(Json(status(&state).await?))
}
