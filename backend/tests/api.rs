use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn get_json(uri: &str) -> (StatusCode, serde_json::Value) {
    let response = quorum_server::api::router()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn info_returns_name_and_version() {
    let (status, body) = get_json("/api/v1/info").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], quorum_server::APP_NAME);
    assert_eq!(body["version"], quorum_server::VERSION);
}

#[tokio::test]
async fn openapi_is_served_and_lists_info() {
    let (status, body) = get_json("/api/v1/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["paths"]["/info"]["get"].is_object());
}
