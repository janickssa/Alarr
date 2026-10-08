//! Authentication: session cookies (web), device bearer tokens (PWA/iOS), roles and rate limits.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::header;
use axum::http::request::Parts;

use crate::api::error::ApiError;
use crate::crypto::sha256_hex;
use crate::db::{Role, User};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "quorum_session";
/// Device token cookie for the installed PWA, so the token is never readable by scripts.
pub const DEVICE_COOKIE: &str = "quorum_device";
/// Browsers cap cookie lifetime at 400 days; the device stays paired until revoked.
const DEVICE_COOKIE_MAX_AGE: i64 = 400 * 86_400;
/// Sessions slide: each use extends them to 30 days from now.
pub const SESSION_TTL: i64 = 30 * 86_400;
pub const SESSION_PREFIX: &str = "qs_";
pub const DEVICE_PREFIX: &str = "qd_";

#[derive(Debug, Clone)]
pub enum AuthVia {
    /// Web session; holds the token hash (for logout).
    Session(String),
    /// Paired device id.
    Device(i64),
}

/// The signed-in user of a request. Extracting it rejects unauthenticated requests with 401.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user: User,
    pub via: AuthVia,
}

impl CurrentUser {
    pub fn is_owner(&self) -> bool {
        self.user.role == Role::Owner
    }

    pub fn require_owner(&self) -> Result<(), ApiError> {
        if self.is_owner() {
            Ok(())
        } else {
            Err(ApiError::Forbidden)
        }
    }

    /// Owners and members see everything; guests only the services they were given.
    pub fn can_see(&self, service_id: &str) -> bool {
        match self.user.role {
            Role::Owner | Role::Member => true,
            Role::Guest => self.user.allowed_services.iter().any(|s| s == service_id),
        }
    }

    /// Run power and service actions. Guests need `can_act` and access to the service.
    pub fn can_act(&self, service_id: &str) -> bool {
        match self.user.role {
            Role::Owner | Role::Member => true,
            Role::Guest => self.user.can_act && self.can_see(service_id),
        }
    }
}

fn bearer(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}

pub fn cookie<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
    parts
        .headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(token) = bearer(parts) {
            if token.starts_with(DEVICE_PREFIX)
                && let Some((device, user)) = state.db.device_user(&sha256_hex(token)).await?
            {
                return Ok(Self {
                    user,
                    via: AuthVia::Device(device.id),
                });
            }
            return Err(ApiError::Unauthorized);
        }
        if let Some(token) = cookie(parts, SESSION_COOKIE) {
            let hash = sha256_hex(token);
            if let Some(user) = state.db.session_user(&hash, SESSION_TTL).await? {
                return Ok(Self {
                    user,
                    via: AuthVia::Session(hash),
                });
            }
        }
        if let Some(token) = cookie(parts, DEVICE_COOKIE)
            && token.starts_with(DEVICE_PREFIX)
            && let Some((device, user)) = state.db.device_user(&sha256_hex(token)).await?
        {
            return Ok(Self {
                user,
                via: AuthVia::Device(device.id),
            });
        }
        Err(ApiError::Unauthorized)
    }
}

/// `Set-Cookie` value for a new session.
pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={SESSION_TTL}{}",
        if secure { "; Secure" } else { "" }
    )
}

/// `Set-Cookie` value carrying a PWA device token.
pub fn device_cookie(token: &str, secure: bool) -> String {
    format!(
        "{DEVICE_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={DEVICE_COOKIE_MAX_AGE}{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn clear_device_cookie(secure: bool) -> String {
    format!(
        "{DEVICE_COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn clear_session_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

/// The client's address, used to rate-limit sign-in and pairing. Normally the connection's
/// peer address. When the peer is one of `[server] trusted_proxies` (a reverse proxy such as
/// Nginx Proxy Manager), the client is the right-most `X-Forwarded-For` entry that is not
/// itself a trusted proxy; the header is ignored from anyone else, so it can't be spoofed.
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        Ok(Self(client_ip(
            peer,
            &parts.headers,
            &state.config.server.trusted_proxies,
        )))
    }
}

pub fn client_ip(
    peer: IpAddr,
    headers: &axum::http::HeaderMap,
    proxies: &[crate::config::Cidr],
) -> IpAddr {
    let trusted = |ip: IpAddr| proxies.iter().any(|p| p.contains(ip));
    if !trusted(peer) {
        return peer;
    }
    let forwarded: Vec<IpAddr> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter_map(|e| e.trim().parse().ok())
        .collect();
    forwarded
        .into_iter()
        .rev()
        .find(|ip| !trusted(*ip))
        .unwrap_or(peer)
}

type WindowKey = (&'static str, IpAddr);

/// Fixed-window rate limiter per (scope, client IP).
#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<WindowKey, (Instant, u32)>>,
}

impl RateLimiter {
    /// Allow `max` attempts per `window`. Returns 429 when exceeded.
    pub fn check(
        &self,
        scope: &'static str,
        ip: IpAddr,
        max: u32,
        window: Duration,
    ) -> Result<(), ApiError> {
        let mut windows = self.windows.lock().expect("lock");
        let now = Instant::now();
        let entry = windows.entry((scope, ip)).or_insert((now, 0));
        if now.duration_since(entry.0) >= window {
            *entry = (now, 0);
        }
        entry.1 += 1;
        if entry.1 > max {
            Err(ApiError::TooManyRequests)
        } else {
            Ok(())
        }
    }

    /// Drop windows older than an hour.
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.windows
            .lock()
            .expect("lock")
            .retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(3600));
    }
}

/// Login, setup and pairing: 10 attempts per 5 minutes per client.
pub fn limit_auth(state: &AppState, scope: &'static str, ip: IpAddr) -> Result<(), ApiError> {
    state.limiter.check(scope, ip, 10, Duration::from_secs(300))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarded_for_only_from_trusted_proxies() {
        let proxies: Vec<crate::config::Cidr> = vec!["10.0.20.5".to_owned().try_into().unwrap()];
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        let mut h = axum::http::HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.9, 10.8.0.2".parse().unwrap());
        // Through the proxy: the right-most address the proxy saw.
        assert_eq!(client_ip(ip("10.0.20.5"), &h, &proxies), ip("10.8.0.2"));
        // Anyone else can't pick their own address.
        assert_eq!(client_ip(ip("10.8.0.7"), &h, &proxies), ip("10.8.0.7"));
        // No proxies configured: the header is ignored.
        assert_eq!(client_ip(ip("10.0.20.5"), &h, &[]), ip("10.0.20.5"));
        // Proxy without the header (health checks): the proxy itself.
        let empty = axum::http::HeaderMap::new();
        assert_eq!(
            client_ip(ip("10.0.20.5"), &empty, &proxies),
            ip("10.0.20.5")
        );
        // Chained trusted proxies are skipped; garbage entries are ignored.
        h.insert(
            "x-forwarded-for",
            "10.8.0.3, junk, 10.0.20.5".parse().unwrap(),
        );
        assert_eq!(client_ip(ip("10.0.20.5"), &h, &proxies), ip("10.8.0.3"));
    }

    #[test]
    fn rate_limiter_blocks_after_max() {
        let rl = RateLimiter::default();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        for _ in 0..3 {
            assert!(rl.check("login", ip, 3, Duration::from_secs(60)).is_ok());
        }
        assert!(rl.check("login", ip, 3, Duration::from_secs(60)).is_err());
        assert!(rl.check("pairing", ip, 3, Duration::from_secs(60)).is_ok());
        assert!(rl.check("login", ip, 3, Duration::ZERO).is_ok());
    }
}
