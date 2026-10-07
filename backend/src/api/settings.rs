//! App settings and user management.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::auth::{validate_password, validate_username};
use super::error::{ApiError, ApiResult, ErrorBody};
use crate::auth::CurrentUser;
use crate::crypto;
use crate::db::{ActivityResult, Role, User};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_settings, put_settings))
        .routes(routes!(list_users, create_user))
        .routes(routes!(update_user, delete_user))
}

/// Settings shared by every client.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct AppSettings {
    /// Service ids in Home card order; services not listed follow in their default order.
    pub home_order: Vec<String>,
    /// Service ids hidden from Home.
    pub home_hidden: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BackendInfo {
    pub name: String,
    pub version: String,
    pub demo: bool,
    pub public_url: Option<String>,
    pub started_at: DateTime<Utc>,
    /// Services with a running poller.
    pub pollers: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SettingsResponse {
    pub settings: AppSettings,
    pub backend: BackendInfo,
}

const SETTINGS_KEY: &str = "app";

pub async fn load_settings(state: &AppState) -> anyhow::Result<AppSettings> {
    Ok(state
        .db
        .setting(SETTINGS_KEY)
        .await?
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default())
}

fn response(state: &AppState, settings: AppSettings) -> SettingsResponse {
    SettingsResponse {
        settings,
        backend: BackendInfo {
            name: crate::APP_NAME.into(),
            version: crate::VERSION.into(),
            demo: state.demo,
            public_url: state.config.server.public_url.clone(),
            started_at: state.started_at,
            pollers: state.supervisor.running().len() as u32,
        },
    }
}

/// App settings and backend info.
#[utoipa::path(get, path = "/settings", tag = "settings",
    responses((status = 200, body = SettingsResponse)))]
async fn get_settings(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> ApiResult<Json<SettingsResponse>> {
    let settings = load_settings(&state).await?;
    Ok(Json(response(&state, settings)))
}

/// Replace the app settings. Owner only.
#[utoipa::path(put, path = "/settings", tag = "settings",
    request_body = AppSettings,
    responses((status = 200, body = SettingsResponse), (status = 403, body = ErrorBody)))]
async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(settings): Json<AppSettings>,
) -> ApiResult<Json<SettingsResponse>> {
    user.require_owner()?;
    state
        .db
        .set_setting(
            SETTINGS_KEY,
            &serde_json::to_string(&settings).map_err(anyhow::Error::from)?,
        )
        .await?;
    state
        .log(
            Some(&user.user),
            "settings.update",
            "settings",
            ActivityResult::Ok,
            None,
            None,
        )
        .await?;
    Ok(Json(response(&state, settings)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUser {
    pub username: String,
    pub password: String,
    pub role: Role,
    /// Guests only: service ids they may see.
    #[serde(default)]
    pub allowed_services: Vec<String>,
    /// Guests only: may run power and service actions.
    #[serde(default)]
    pub can_act: bool,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct UpdateUser {
    pub role: Option<Role>,
    pub allowed_services: Option<Vec<String>>,
    pub can_act: Option<bool>,
    /// Set a new password (signs the user out everywhere on the web).
    pub password: Option<String>,
}

/// All users. Owner only.
#[utoipa::path(get, path = "/users", tag = "users",
    responses((status = 200, body = Vec<User>), (status = 403, body = ErrorBody)))]
async fn list_users(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<User>>> {
    user.require_owner()?;
    Ok(Json(state.db.list_users().await?))
}

/// Add a user. Owner only.
#[utoipa::path(post, path = "/users", tag = "users",
    request_body = CreateUser,
    responses((status = 201, body = User), (status = 409, body = ErrorBody)))]
async fn create_user(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<CreateUser>,
) -> ApiResult<(StatusCode, Json<User>)> {
    user.require_owner()?;
    validate_username(&req.username)?;
    validate_password(&req.password)?;
    if state.db.user_by_name(&req.username).await?.is_some() {
        return Err(ApiError::Conflict("username taken".into()));
    }
    let hash = crypto::hash_password(&req.password)?;
    let id = state
        .db
        .create_user(
            &req.username,
            &hash,
            req.role,
            &req.allowed_services,
            req.can_act,
        )
        .await?;
    let created = state
        .db
        .user(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("user vanished"))?;
    state
        .log(
            Some(&user.user),
            "user.create",
            &format!("user:{}", created.username),
            ActivityResult::Ok,
            Some(created.role.as_str()),
            None,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Change a user's role, guest access or password. Owner only.
#[utoipa::path(put, path = "/users/{id}", tag = "users",
    params(("id" = i64, Path)),
    request_body = UpdateUser,
    responses((status = 200, body = User), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn update_user(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateUser>,
) -> ApiResult<Json<User>> {
    user.require_owner()?;
    let target = state
        .db
        .user(id)
        .await?
        .ok_or(ApiError::NotFound("no such user".into()))?;
    let role = req.role.unwrap_or(target.role);
    if target.role == Role::Owner && role != Role::Owner && state.db.count_owners().await? <= 1 {
        return Err(ApiError::BadRequest("cannot demote the last owner".into()));
    }
    state
        .db
        .update_user(
            id,
            role,
            &req.allowed_services
                .unwrap_or(target.allowed_services.clone()),
            req.can_act.unwrap_or(target.can_act),
        )
        .await?;
    if let Some(password) = req.password {
        validate_password(&password)?;
        state
            .db
            .set_password(id, &crypto::hash_password(&password)?)
            .await?;
        state.db.delete_user_sessions(id, None).await?;
    }
    state
        .log(
            Some(&user.user),
            "user.update",
            &format!("user:{}", target.username),
            ActivityResult::Ok,
            None,
            None,
        )
        .await?;
    Ok(Json(
        state
            .db
            .user(id)
            .await?
            .ok_or(ApiError::NotFound("no such user".into()))?,
    ))
}

/// Remove a user, their sessions and devices. Owner only; not yourself.
#[utoipa::path(delete, path = "/users/{id}", tag = "users",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn delete_user(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    user.require_owner()?;
    if id == user.user.id {
        return Err(ApiError::BadRequest("you cannot delete yourself".into()));
    }
    let target = state
        .db
        .user(id)
        .await?
        .ok_or(ApiError::NotFound("no such user".into()))?;
    state.db.delete_user(id).await?;
    state
        .log(
            Some(&user.user),
            "user.delete",
            &format!("user:{}", target.username),
            ActivityResult::Ok,
            None,
            None,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
