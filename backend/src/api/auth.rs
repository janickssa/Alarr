//! Owner setup, login, logout, current user and password change.

use std::sync::LazyLock;

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::{ApiError, ApiResult, ErrorBody};
use crate::auth::{AuthVia, ClientIp, CurrentUser, SESSION_PREFIX, SESSION_TTL, limit_auth};
use crate::crypto::{self, sha256_hex};
use crate::db::{ActivityResult, Role, User};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(setup_status, setup))
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
        .routes(routes!(change_password))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupStatus {
    /// True until the owner account exists.
    pub needs_setup: bool,
    pub demo: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetupRequest {
    /// The one-time token printed in the server log on first start.
    pub setup_token: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Me {
    pub user: User,
    /// `session` (web cookie) or `device` (bearer token).
    pub auth: String,
    pub device_id: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordChange {
    pub current_password: String,
    pub new_password: String,
}

pub fn validate_username(name: &str) -> ApiResult<()> {
    let ok = (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "username: 1–64 characters, letters, digits, '.', '_' or '-'".into(),
        ))
    }
}

pub fn validate_password(password: &str) -> ApiResult<()> {
    if (8..=1024).contains(&password.chars().count()) {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "password must be at least 8 characters".into(),
        ))
    }
}

/// Hash compared against when the user doesn't exist, so timing doesn't reveal usernames.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| crypto::hash_password("quorum-dummy-password").unwrap_or_default());

type WithCookie = ([(header::HeaderName, String); 1], Json<Me>);

async fn start_session(state: &AppState, user: User) -> ApiResult<WithCookie> {
    let token = crypto::random_token(SESSION_PREFIX, 32);
    state
        .db
        .create_session(&sha256_hex(&token), user.id, SESSION_TTL)
        .await?;
    let cookie = crate::auth::session_cookie(&token, state.config.secure_cookies());
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(Me {
            user,
            auth: "session".into(),
            device_id: None,
        }),
    ))
}

/// Whether the owner account still has to be created.
#[utoipa::path(get, path = "/auth/setup", tag = "auth", security(()),
    responses((status = 200, body = SetupStatus)))]
async fn setup_status(State(state): State<AppState>) -> ApiResult<Json<SetupStatus>> {
    Ok(Json(SetupStatus {
        needs_setup: state.db.count_users().await? == 0,
        demo: state.demo,
    }))
}

/// Create the owner account with the one-time setup token. Signs the owner in.
#[utoipa::path(post, path = "/auth/setup", tag = "auth", security(()),
    request_body = SetupRequest,
    responses(
        (status = 200, body = Me, description = "Owner created; session cookie set"),
        (status = 403, body = ErrorBody, description = "Wrong setup token or already set up"),
        (status = 429, body = ErrorBody),
    ))]
async fn setup(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<SetupRequest>,
) -> ApiResult<WithCookie> {
    limit_auth(&state, "setup", ip)?;
    validate_username(&req.username)?;
    validate_password(&req.password)?;
    {
        let guard = state.setup_token.lock().expect("lock");
        let ok = guard.as_deref().is_some_and(|t| {
            crypto::constant_time_eq(t.as_bytes(), req.setup_token.trim().as_bytes())
        });
        if !ok {
            return Err(ApiError::Forbidden);
        }
    }
    if state.db.count_users().await? > 0 {
        return Err(ApiError::Forbidden);
    }
    let hash = crypto::hash_password(&req.password)?;
    let id = state
        .db
        .create_user(&req.username, &hash, Role::Owner, &[], false)
        .await?;
    *state.setup_token.lock().expect("lock") = None;
    let user = state.db.user(id).await?.ok_or(ApiError::Unauthorized)?;
    state
        .log(
            Some(&user),
            "auth.setup",
            &format!("user:{}", user.username),
            ActivityResult::Ok,
            None,
            None,
        )
        .await?;
    tracing::info!(username = %user.username, "owner account created");
    start_session(&state, user).await
}

/// Sign in with username and password. Sets an HttpOnly session cookie.
#[utoipa::path(post, path = "/auth/login", tag = "auth", security(()),
    request_body = LoginRequest,
    responses(
        (status = 200, body = Me, description = "Signed in; session cookie set"),
        (status = 401, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ))]
async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<LoginRequest>,
) -> ApiResult<WithCookie> {
    limit_auth(&state, "login", ip)?;
    let found = state.db.user_by_name(&req.username).await?;
    let password = req.password.clone();
    let hash = found
        .as_ref()
        .map(|(_, h)| h.clone())
        .unwrap_or_else(|| DUMMY_HASH.clone());
    let valid = tokio::task::spawn_blocking(move || crypto::verify_password(&password, &hash))
        .await
        .map_err(anyhow::Error::from)?;
    match found {
        Some((user, _)) if valid => {
            state
                .log(
                    Some(&user),
                    "auth.login",
                    &format!("user:{}", user.username),
                    ActivityResult::Ok,
                    None,
                    None,
                )
                .await?;
            start_session(&state, user).await
        }
        _ => {
            let target = format!("user:{}", req.username.chars().take(64).collect::<String>());
            state
                .log(
                    None,
                    "auth.login",
                    &target,
                    ActivityResult::Error,
                    Some(&format!("from {ip}")),
                    None,
                )
                .await?;
            Err(ApiError::Unauthorized)
        }
    }
}

/// Sign out: ends the web session, or unpairs the device (PWA cookie or bearer token).
#[utoipa::path(post, path = "/auth/logout", tag = "auth",
    responses((status = 204, description = "Signed out; cookies cleared")))]
async fn logout(State(state): State<AppState>, user: CurrentUser) -> ApiResult<impl IntoResponse> {
    match &user.via {
        AuthVia::Session(hash) => state.db.delete_session(hash).await?,
        AuthVia::Device(id) => state.db.delete_device(*id).await?,
    }
    state
        .log(
            Some(&user.user),
            "auth.logout",
            &format!("user:{}", user.user.username),
            ActivityResult::Ok,
            None,
            None,
        )
        .await?;
    let secure = state.config.secure_cookies();
    Ok((
        StatusCode::NO_CONTENT,
        // Two Set-Cookie headers: an array would keep only the last one.
        axum::response::AppendHeaders([
            (
                header::SET_COOKIE,
                crate::auth::clear_session_cookie(secure),
            ),
            (header::SET_COOKIE, crate::auth::clear_device_cookie(secure)),
        ]),
    ))
}

/// The signed-in user. A cookie-based sign-in gets its cookie re-issued, which keeps it
/// from expiring while the app is used and moves cookies set by older versions to the
/// current attributes.
#[utoipa::path(get, path = "/auth/me", tag = "auth",
    responses((status = 200, body = Me), (status = 401, body = ErrorBody)))]
async fn me(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    user: CurrentUser,
) -> impl IntoResponse {
    let secure = state.config.secure_cookies();
    let (auth, device_id, refresh) = match user.via {
        AuthVia::Session(_) => (
            "session",
            None,
            crate::auth::cookie_in(&headers, crate::auth::SESSION_COOKIE)
                .map(|t| crate::auth::session_cookie(t, secure)),
        ),
        AuthVia::Device(id) => (
            "device",
            Some(id),
            crate::auth::cookie_in(&headers, crate::auth::DEVICE_COOKIE)
                .map(|t| crate::auth::device_cookie(t, secure)),
        ),
    };
    (
        axum::response::AppendHeaders(refresh.map(|c| (header::SET_COOKIE, c))),
        Json(Me {
            user: user.user,
            auth: auth.into(),
            device_id,
        }),
    )
}

/// Change your own password. Signs out your other web sessions.
#[utoipa::path(put, path = "/auth/password", tag = "auth",
    request_body = PasswordChange,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn change_password(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    user: CurrentUser,
    Json(req): Json<PasswordChange>,
) -> ApiResult<StatusCode> {
    limit_auth(&state, "password", ip)?;
    validate_password(&req.new_password)?;
    let hash = state
        .db
        .password_hash(user.user.id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !crypto::verify_password(&req.current_password, &hash) {
        return Err(ApiError::Forbidden);
    }
    state
        .db
        .set_password(user.user.id, &crypto::hash_password(&req.new_password)?)
        .await?;
    let keep = match &user.via {
        AuthVia::Session(hash) => Some(hash.as_str()),
        AuthVia::Device(_) => None,
    };
    state.db.delete_user_sessions(user.user.id, keep).await?;
    state
        .log(
            Some(&user.user),
            "auth.password",
            &format!("user:{}", user.user.username),
            ActivityResult::Ok,
            None,
            None,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
