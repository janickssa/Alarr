//! Alerts engine end to end in demo mode: rules, delays, deduplication, ntfy delivery,
//! resolution with a recovery message, settings, mutes and the test send.

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use http_body_util::BodyExt;
use quorum_server::alerts::Engine;
use quorum_server::config::Config;
use quorum_server::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn app(ntfy: &MockServer) -> (AppState, Router) {
    let config = Config::parse(&format!(
        "[server]\npublic_url = \"https://quorum.example\"\n[ntfy]\nurl = \"{}\"\ntopic = \"quorum\"\n",
        ntfy.uri()
    ))
    .unwrap();
    let state = quorum_server::app::build(config, true).await.unwrap();
    let ids: Vec<String> = state
        .db
        .list_services()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    for _ in 0..250 {
        if ids.iter().all(|id| state.cache.get(id).is_some()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let router = quorum_server::api::router(state.clone());
    (state, router)
}

async fn call(
    router: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value, Option<String>) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        cookie,
    )
}

async fn login(router: &Router, user: &str) -> String {
    let (status, _, cookie) = call(
        router,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({"username": user, "password": user})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    cookie.unwrap()
}

fn keys(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|a| a["key"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn alerts_fire_once_deliver_to_ntfy_and_resolve() {
    let ntfy = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&ntfy)
        .await;
    let (state, router) = app(&ntfy).await;
    let owner = login(&router, "demo").await;
    let guest = login(&router, "guest").await;

    let t0 = Utc::now();
    let mut engine = Engine::new(state.clone());
    engine.tick(t0).await.unwrap();
    let (_, list, _) = call(&router, "GET", "/api/v1/alerts", Some(&owner), None).await;
    let k = keys(&list);
    // Immediate: certificate, torrent error, new Seerr requests. Down waits a minute.
    assert!(
        k.contains(&"certificate_expiring:nginx-proxy-manager:4".to_owned()),
        "{k:?}"
    );
    assert!(
        k.iter()
            .any(|k| k.starts_with("download_failed:qbittorrent:")),
        "{k:?}"
    );
    assert!(
        k.iter().any(|k| k.starts_with("new_request:seerr:")),
        "{k:?}"
    );
    assert!(!k.contains(&"service_down:drawio".to_owned()));

    engine
        .tick(t0 + chrono::Duration::seconds(61))
        .await
        .unwrap();
    let (_, list, _) = call(&router, "GET", "/api/v1/alerts", Some(&owner), None).await;
    let down = list
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["key"] == "service_down:drawio")
        .expect("draw.io down alert")
        .clone();
    assert_eq!(down["severity"], "critical");
    assert_eq!(down["title"], "draw.io is down");
    assert_eq!(down["delivery"][0]["channel"], "ntfy");
    assert_eq!(down["delivery"][0]["result"], "sent");
    let count = list.as_array().unwrap().len();

    // No duplicates while the conditions hold.
    engine
        .tick(t0 + chrono::Duration::seconds(90))
        .await
        .unwrap();
    let (_, list, _) = call(&router, "GET", "/api/v1/alerts", Some(&owner), None).await;
    assert_eq!(list.as_array().unwrap().len(), count);
    let sent = ntfy.received_requests().await.unwrap();
    assert_eq!(sent.len(), count);
    let body: Value = serde_json::from_slice(&sent.last().unwrap().body).unwrap();
    assert_eq!(body["topic"], "quorum");
    assert_eq!(body["click"], "https://quorum.example/services/drawio");

    // Guests only see alerts of services they were given.
    let (_, glist, _) = call(&router, "GET", "/api/v1/alerts", Some(&guest), None).await;
    assert!(glist.as_array().unwrap().len() < count);

    // Turning draw.io off clears the condition: resolved, with a "back up" message.
    let (status, _, _) = call(
        &router,
        "POST",
        "/api/v1/services/drawio/disable",
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    engine
        .tick(t0 + chrono::Duration::seconds(120))
        .await
        .unwrap();
    let (_, active, _) = call(
        &router,
        "GET",
        "/api/v1/alerts?active=true",
        Some(&owner),
        None,
    )
    .await;
    assert!(!keys(&active).contains(&"service_down:drawio".to_owned()));
    let sent = ntfy.received_requests().await.unwrap();
    let last: Value = serde_json::from_slice(&sent.last().unwrap().body).unwrap();
    assert_eq!(last["title"], "draw.io is back up");
    assert_eq!(last["priority"], 2);

    // A new request is announced once, never again.
    let seen = state
        .db
        .alert_key_seen("new_request:seerr:31")
        .await
        .unwrap();
    assert!(seen);
}

#[tokio::test]
async fn settings_mutes_and_test_send() {
    let ntfy = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&ntfy)
        .await;
    let (state, router) = app(&ntfy).await;
    let owner = login(&router, "demo").await;
    let guest = login(&router, "guest").await;

    let (status, s, _) = call(
        &router,
        "GET",
        "/api/v1/alerts/settings",
        Some(&guest),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(s["types"].as_array().unwrap().len(), 6);
    assert_eq!(s["types"][0]["alert_type"], "service_down");
    assert_eq!(s["types"][0]["channels"], json!(["ntfy"]));
    assert_eq!(s["channels"][0]["configured"], true);
    assert_eq!(s["channels"][1]["id"], "push");
    assert_eq!(s["channels"][1]["configured"], false);

    let update = json!({"types": [{"alert_type": "new_request", "enabled": false, "channels": ["ntfy", "push"],
                                   "quiet_hours": {"start": "22:00", "end": "07:00"}}]});
    let (status, _, _) = call(
        &router,
        "PUT",
        "/api/v1/alerts/settings",
        Some(&guest),
        Some(update.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, s, _) = call(
        &router,
        "PUT",
        "/api/v1/alerts/settings",
        Some(&owner),
        Some(update),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let nr = s["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["alert_type"] == "new_request")
        .unwrap();
    assert_eq!(nr["enabled"], false);
    assert_eq!(nr["channels"], json!(["ntfy", "push"]));
    assert_eq!(nr["quiet_hours"]["start"], "22:00");
    for bad in [
        json!({"types": [{"alert_type": "disk_full", "enabled": true, "channels": ["sms"], "quiet_hours": null}]}),
        json!({"types": [{"alert_type": "disk_full", "enabled": true, "channels": [], "quiet_hours": {"start": "25:00", "end": "07:00"}}]}),
    ] {
        let (status, _, _) = call(
            &router,
            "PUT",
            "/api/v1/alerts/settings",
            Some(&owner),
            Some(bad),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    // Disabled types don't fire.
    Engine::new(state.clone()).tick(Utc::now()).await.unwrap();
    let (_, list, _) = call(&router, "GET", "/api/v1/alerts", Some(&owner), None).await;
    assert!(!keys(&list).iter().any(|k| k.starts_with("new_request:")));

    let (status, m, _) = call(
        &router,
        "PUT",
        "/api/v1/alerts/mutes",
        Some(&guest),
        Some(json!({"types": ["download_failed", "disk_full", "disk_full"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(m["types"], json!(["disk_full", "download_failed"]));
    let (_, m, _) = call(&router, "GET", "/api/v1/alerts/mutes", Some(&owner), None).await;
    assert_eq!(m["types"], json!([]));

    let (status, r, _) = call(
        &router,
        "POST",
        "/api/v1/alerts/test",
        Some(&owner),
        Some(json!({"channel": "ntfy"})),
    )
    .await;
    assert_eq!((status, r["ok"].clone()), (StatusCode::OK, json!(true)));
    let (_, r, _) = call(
        &router,
        "POST",
        "/api/v1/alerts/test",
        Some(&owner),
        Some(json!({"channel": "push"})),
    )
    .await;
    assert_eq!(r["ok"], false);
    let (status, _, _) = call(
        &router,
        "POST",
        "/api/v1/alerts/test",
        Some(&guest),
        Some(json!({"channel": "ntfy"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, activity, _) = call(&router, "GET", "/api/v1/activity", Some(&owner), None).await;
    assert!(
        activity
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["action"] == "alerts.settings")
    );
}
