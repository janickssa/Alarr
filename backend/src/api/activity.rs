//! Activity log.

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::ApiResult;
use crate::auth::CurrentUser;
use crate::db::{ActivityEntry, Role};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ActivityQuery {
    /// 1–200, default 50.
    pub limit: Option<i64>,
    /// Return entries older than this id (pagination).
    pub before: Option<i64>,
}

/// Who did what, when, to what, and the result. Newest first. Guests see their own entries.
#[utoipa::path(get, path = "/activity", tag = "activity", operation_id = "list_activity",
    params(ActivityQuery),
    responses((status = 200, body = Vec<ActivityEntry>)))]
async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ActivityQuery>,
) -> ApiResult<Json<Vec<ActivityEntry>>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let only = (user.user.role == Role::Guest).then_some(user.user.id);
    Ok(Json(state.db.list_activity(limit, q.before, only).await?))
}
