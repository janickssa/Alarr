//! HTTP API under `/api/v1` and its OpenAPI document.

pub mod activity;
pub mod alerts;
pub mod auth;
pub mod automation;
pub mod error;
pub mod events;
pub mod media;
pub mod overview;
pub mod pairing;
pub mod proxmox;
pub mod services;
pub mod settings;
pub mod widget;

use axum::extract::State;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    info(title = "Quorum API", description = "Quorum homelab control center API"),
    modifiers(&SecurityAddon),
    security(("session" = []), ("device" = [])),
    components(schemas(
        crate::events::Event,
        crate::modules::proxmox::ProxmoxSettings,
        crate::modules::health_check::HealthCheckSettings,
        crate::modules::arr::LibraryItem,
        crate::modules::seerr::SearchResult,
    ))
)]
struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                crate::auth::SESSION_COOKIE,
                "Web session cookie set by /auth/login",
            ))),
        );
        components.add_security_scheme(
            "device",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some("Device token from /pairing/complete"))
                    .build(),
            ),
        );
    }
}

/// Basic information about the running server.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServerInfo {
    /// Product name.
    pub name: String,
    /// Server version.
    pub version: String,
    /// True when running with `--demo` (fake data).
    pub demo: bool,
}

/// Return the server name and version. Unauthenticated; used for reachability checks.
#[utoipa::path(
    get,
    path = "/info",
    tag = "server",
    security(()),
    responses((status = 200, description = "Server info", body = ServerInfo))
)]
async fn info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json(ServerInfo {
        name: crate::APP_NAME.to_owned(),
        version: crate::VERSION.to_owned(),
        demo: state.demo,
    })
}

fn api_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(info))
        .merge(auth::router())
        .merge(pairing::router())
        .merge(services::router())
        .merge(overview::router())
        .merge(media::router())
        .merge(proxmox::router())
        .merge(activity::router())
        .merge(alerts::router())
        .merge(automation::router())
        .merge(settings::router())
        .merge(widget::router())
        .merge(events::router())
}

fn split() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    let (router, mut spec) = api_router().split_for_parts();
    spec.info.version = crate::VERSION.to_owned();
    spec.servers = Some(vec![utoipa::openapi::Server::new("/api/v1")]);
    (router, spec)
}

/// The OpenAPI document for the whole API.
pub fn openapi() -> utoipa::openapi::OpenApi {
    split().1
}

/// The full application: API under `/api/v1`, the embedded web UI for everything else.
pub fn router(state: AppState) -> Router {
    let (api, spec) = split();
    let spec = std::sync::Arc::new(spec);
    let api = api
        .route(
            "/openapi.json",
            axum::routing::get(move || {
                let spec = spec.clone();
                async move { Json((*spec).clone()) }
            }),
        )
        .fallback(|| async { error::ApiError::NotFound("no such API endpoint".into()) });
    Router::new()
        .nest("/api/v1", api)
        .fallback(crate::web::serve)
        .with_state(state)
}
