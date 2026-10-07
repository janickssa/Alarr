//! Device pairing (owner shows a code/QR, the device redeems it for a token) and device list.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::error::{ApiError, ApiResult, ErrorBody};
use crate::auth::{ClientIp, CurrentUser, DEVICE_PREFIX, limit_auth};
use crate::crypto::{random_code, random_token, sha256_hex};
use crate::db::{ActivityResult, Device};
use crate::state::AppState;

/// Pairing codes are valid for 10 minutes.
const PAIRING_TTL: i64 = 600;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(start))
        .routes(routes!(complete))
        .routes(routes!(list_devices))
        .routes(routes!(delete_device))
        .routes(routes!(set_push_token, delete_push_token))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct StartPairing {
    /// User the device will belong to (default: the owner starting the pairing).
    pub user_id: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PairingStarted {
    /// Code to type on the device, formatted `XXXX-XXXX`.
    pub code: String,
    pub expires_at: DateTime<Utc>,
    /// Content for the QR code: `quorum://pair?server=<public url>&code=<code>`.
    pub qr_payload: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CompletePairing {
    pub code: String,
    /// e.g. "Janick's iPhone".
    pub device_name: String,
    /// `ios`, `pwa` or `other`.
    pub platform: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PairingCompleted {
    /// Bearer token for `Authorization: Bearer …`. Shown once; store it in the Keychain.
    /// `null` for the PWA, which gets it as an HttpOnly cookie instead.
    pub token: Option<String>,
    pub device: Device,
}

fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Start pairing a device. Owner only.
#[utoipa::path(post, path = "/pairing/start", tag = "pairing",
    request_body = StartPairing,
    responses((status = 200, body = PairingStarted), (status = 403, body = ErrorBody)))]
async fn start(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<StartPairing>,
) -> ApiResult<Json<PairingStarted>> {
    user.require_owner()?;
    let target = match req.user_id {
        Some(id) => state
            .db
            .user(id)
            .await?
            .ok_or(ApiError::NotFound("no such user".into()))?,
        None => user.user.clone(),
    };
    let code = random_code(8);
    let expires_at = crate::db::now() + PAIRING_TTL;
    state
        .db
        .create_pairing(&sha256_hex(&code), target.id, user.user.id, expires_at)
        .await?;
    let display = format!("{}-{}", &code[..4], &code[4..]);
    let mut params = vec![("code", display.clone())];
    if let Some(url) = &state.config.server.public_url {
        params.insert(0, ("server", url.clone()));
    }
    let qr_payload = reqwest::Url::parse_with_params("quorum://pair", &params)
        .map_err(anyhow::Error::from)?
        .to_string();
    state
        .log(
            Some(&user.user),
            "pairing.start",
            &format!("user:{}", target.username),
            ActivityResult::Ok,
            None,
            None,
        )
        .await?;
    Ok(Json(PairingStarted {
        code: display,
        expires_at: crate::db::ts(expires_at),
        qr_payload,
    }))
}

/// Redeem a pairing code for a device token. No session needed; rate limited.
#[utoipa::path(post, path = "/pairing/complete", tag = "pairing", security(()),
    request_body = CompletePairing,
    responses(
        (status = 200, body = PairingCompleted),
        (status = 401, body = ErrorBody, description = "Unknown or expired code"),
        (status = 429, body = ErrorBody),
    ))]
async fn complete(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<CompletePairing>,
) -> ApiResult<axum::response::Response> {
    limit_auth(&state, "pairing", ip)?;
    let name = req.device_name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(ApiError::BadRequest("device_name: 1–64 characters".into()));
    }
    let platform = match req.platform.as_str() {
        p @ ("ios" | "pwa" | "other") => p,
        _ => {
            return Err(ApiError::BadRequest(
                "platform must be ios, pwa or other".into(),
            ));
        }
    };
    let Some(user_id) = state
        .db
        .take_pairing(&sha256_hex(&normalize_code(&req.code)))
        .await?
    else {
        state
            .log(
                None,
                "pairing.complete",
                &format!("device:{name}"),
                ActivityResult::Error,
                Some(&format!("from {ip}")),
                None,
            )
            .await?;
        return Err(ApiError::Unauthorized);
    };
    let token = random_token(DEVICE_PREFIX, 32);
    let device = state
        .db
        .create_device(user_id, name, platform, &sha256_hex(&token))
        .await?;
    let user = state.db.user(user_id).await?;
    state
        .log(
            user.as_ref(),
            "pairing.complete",
            &format!("device:{}", device.id),
            ActivityResult::Ok,
            Some(name),
            None,
        )
        .await?;
    if platform == "pwa" {
        let cookie = crate::auth::device_cookie(&token, state.config.secure_cookies());
        return Ok((
            [(axum::http::header::SET_COOKIE, cookie)],
            Json(PairingCompleted {
                token: None,
                device,
            }),
        )
            .into_response());
    }
    Ok(Json(PairingCompleted {
        token: Some(token),
        device,
    })
    .into_response())
}

/// Paired devices: all for the owner, your own for everyone else.
#[utoipa::path(get, path = "/devices", tag = "devices",
    responses((status = 200, body = Vec<Device>)))]
async fn list_devices(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Device>>> {
    let filter = (!user.is_owner()).then_some(user.user.id);
    Ok(Json(state.db.list_devices(filter).await?))
}

/// Revoke a device. The owner can revoke any device; others only their own.
#[utoipa::path(delete, path = "/devices/{id}", tag = "devices",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_device(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let device = state
        .db
        .device(id)
        .await?
        .filter(|d| user.is_owner() || d.user_id == user.user.id)
        .ok_or(ApiError::NotFound("no such device".into()))?;
    state.db.delete_device(device.id).await?;
    state
        .log(
            Some(&user.user),
            "device.revoke",
            &format!("device:{}", device.id),
            ActivityResult::Ok,
            Some(&device.name),
            None,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PushToken {
    /// APNs device token, hex as the app gets it from `registerForRemoteNotifications`.
    pub token: String,
    /// `sandbox` (Xcode debug build) or `production` (TestFlight, ad hoc). Default: the
    /// server's `[apns] environment`.
    pub environment: Option<String>,
}

/// The calling device itself (push tokens are registered by the iPhone, never for it).
fn own_device(user: &CurrentUser, id: i64) -> Result<(), ApiError> {
    match user.via {
        crate::auth::AuthVia::Device(d) if d == id => Ok(()),
        _ => Err(ApiError::Forbidden),
    }
}

/// Register this device's APNs token so alerts chosen for `push` reach it.
#[utoipa::path(post, path = "/devices/{id}/push-token", tag = "devices",
    params(("id" = i64, Path)),
    request_body = PushToken,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)))]
async fn set_push_token(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(req): Json<PushToken>,
) -> ApiResult<StatusCode> {
    own_device(&user, id)?;
    let token = req.token.trim().to_ascii_lowercase();
    if !(32..=200).contains(&token.len()) || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::BadRequest(
            "token: APNs device token in hex".into(),
        ));
    }
    let env = req.environment.unwrap_or_else(|| {
        state
            .config
            .apns
            .as_ref()
            .map(|a| a.environment.clone())
            .unwrap_or_else(|| "production".into())
    });
    if env != "sandbox" && env != "production" {
        return Err(ApiError::BadRequest(
            "environment must be sandbox or production".into(),
        ));
    }
    let before = state
        .db
        .device(id)
        .await?
        .ok_or(ApiError::NotFound("no such device".into()))?;
    state.db.set_push_token(id, &token, &env).await?;
    if !before.push {
        state
            .log(
                Some(&user.user),
                "device.push_on",
                &format!("device:{id}"),
                ActivityResult::Ok,
                Some(&format!("{} ({env})", before.name)),
                None,
            )
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Stop push to this device (turned off in the app).
#[utoipa::path(delete, path = "/devices/{id}/push-token", tag = "devices",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 403, body = ErrorBody)))]
async fn delete_push_token(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let device = state
        .db
        .device(id)
        .await?
        .filter(|d| user.is_owner() || d.user_id == user.user.id)
        .ok_or(ApiError::NotFound("no such device".into()))?;
    state.db.clear_push_token(id).await?;
    if device.push {
        state
            .log(
                Some(&user.user),
                "device.push_off",
                &format!("device:{id}"),
                ActivityResult::Ok,
                Some(&device.name),
                None,
            )
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    #[test]
    fn codes_normalize() {
        assert_eq!(super::normalize_code(" abcd-ef23 "), "ABCDEF23");
    }
}
