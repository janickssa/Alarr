//! HTTP API router and OpenAPI document.

use axum::{Json, Router};
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(OpenApi)]
#[openapi(info(
    title = "Quorum API",
    description = "Quorum homelab control center API"
))]
struct ApiDoc;

/// Basic information about the running server.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServerInfo {
    /// Product name.
    pub name: String,
    /// Server version.
    pub version: String,
}

/// Return the server name and version. Unauthenticated; used for reachability checks.
#[utoipa::path(
    get,
    path = "/info",
    tag = "server",
    responses((status = 200, description = "Server info", body = ServerInfo))
)]
async fn info() -> Json<ServerInfo> {
    Json(ServerInfo {
        name: crate::APP_NAME.to_owned(),
        version: crate::VERSION.to_owned(),
    })
}

fn split() -> (Router, utoipa::openapi::OpenApi) {
    let (api_router, mut spec) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(info))
        .split_for_parts();
    spec.info.version = crate::VERSION.to_owned();
    spec.servers = Some(vec![utoipa::openapi::Server::new("/api/v1")]);
    (api_router, spec)
}

/// The OpenAPI document for the whole API.
pub fn openapi() -> utoipa::openapi::OpenApi {
    split().1
}

/// The full application router with the API mounted under `/api/v1`.
pub fn router() -> Router {
    let (api_router, spec) = split();
    let spec = std::sync::Arc::new(spec);
    let api_router = api_router.route(
        "/openapi.json",
        axum::routing::get(move || {
            let spec = spec.clone();
            async move { Json((*spec).clone()) }
        }),
    );
    Router::new().nest("/api/v1", api_router)
}
