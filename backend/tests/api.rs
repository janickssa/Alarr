//! API tests: auth, permissions, the confirm flag, pairing, services and secrets.

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use quorum_server::config::Config;
use quorum_server::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

struct App {
    state: AppState,
    router: Router,
}

impl App {
    /// Demo-mode app: fake modules, users demo/demo (owner) and guest/guest.
    async fn demo() -> Self {
        let state = quorum_server::app::build(Config::parse("").unwrap(), true)
            .await
            .unwrap();
        // Wait until every service has been polled once (tests run in parallel and the
        // first polls can be slow under load).
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
        Self::from_state(state)
    }

    fn from_state(state: AppState) -> Self {
        Self {
            router: quorum_server::api::router(state.clone()),
            state,
        }
    }

    async fn call(
        &self,
        method: &str,
        uri: &str,
        auth: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(a) = auth {
            req = if a.starts_with("qd_") {
                req.header(header::AUTHORIZATION, format!("Bearer {a}"))
            } else {
                req.header(header::COOKIE, a)
            };
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let response = self.router.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value, cookie)
    }

    async fn login(&self, user: &str, password: &str) -> String {
        let (status, _, cookie) = self
            .call(
                "POST",
                "/api/v1/auth/login",
                None,
                Some(json!({"username": user, "password": password})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "login {user}");
        cookie.expect("session cookie")
    }
}

#[tokio::test]
async fn public_endpoints_and_auth_required() {
    let app = App::demo().await;
    let (status, body, _) = app.call("GET", "/api/v1/info", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], quorum_server::APP_NAME);
    assert_eq!(body["demo"], true);

    let (status, body, _) = app.call("GET", "/api/v1/openapi.json", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["paths"]["/proxmox/guests/{vmid}/power"]["post"].is_object());

    for path in [
        "/api/v1/overview",
        "/api/v1/services",
        "/api/v1/proxmox/guests",
        "/api/v1/events",
    ] {
        let (status, body, _) = app.call("GET", path, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(body["error"], "unauthorized");
    }
    let (status, _, _) = app
        .call(
            "GET",
            "/api/v1/overview",
            Some("quorum_session=qs_forged"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = app.call("GET", "/api/v1/nope", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn openapi_operation_ids_are_unique() {
    let spec = quorum_server::api::openapi();
    let mut seen = std::collections::HashSet::new();
    for item in spec["paths"].as_object().unwrap().values() {
        for op in item.as_object().unwrap().values() {
            if let Some(id) = op["operationId"].as_str() {
                assert!(seen.insert(id.to_owned()), "duplicate operationId {id}");
            }
        }
    }
}

#[tokio::test]
async fn login_logout_and_rate_limit() {
    let app = App::demo().await;
    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"username": "demo", "password": "wrong"})),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let cookie = app.login("demo", "demo").await;
    let (status, me, _) = app
        .call("GET", "/api/v1/auth/me", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["role"], "owner");
    assert_eq!(me["auth"], "session");
    // /auth/me re-issues the cookie (sliding expiry), as SameSite=Lax: iOS home-screen
    // apps can drop SameSite=Strict cookies after a relaunch.
    let req = Request::get("/api/v1/auth/me")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(req).await.unwrap();
    let set = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(set.starts_with(&format!("{cookie};")), "{set}");
    assert!(
        set.contains("SameSite=Lax") && set.contains("HttpOnly") && set.contains("Max-Age="),
        "{set}"
    );

    let (status, _, cleared) = app
        .call("POST", "/api/v1/auth/logout", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(cleared.as_deref(), Some("quorum_session="));
    let (status, _, _) = app
        .call("GET", "/api/v1/auth/me", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 2 attempts used above; the limit is 10 per 5 minutes.
    let mut last = StatusCode::OK;
    for _ in 0..9 {
        last = app
            .call(
                "POST",
                "/api/v1/auth/login",
                None,
                Some(json!({"username": "demo", "password": "x"})),
            )
            .await
            .0;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn destructive_power_actions_need_confirm_and_are_logged() {
    let app = App::demo().await;
    let cookie = app.login("demo", "demo").await;

    let (status, body, _) = app
        .call(
            "POST",
            "/api/v1/proxmox/guests/123/power",
            Some(&cookie),
            Some(json!({"action": "reboot"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "confirm_required");

    let (status, body, _) = app
        .call(
            "POST",
            "/api/v1/proxmox/guests/123/power",
            Some(&cookie),
            Some(json!({"action": "reboot", "confirm": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["node"], "pve-02");
    assert!(body["upid"].as_str().unwrap().starts_with("UPID:pve-02:"));

    let (_, log, _) = app
        .call("GET", "/api/v1/activity", Some(&cookie), None)
        .await;
    let entry = log
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["action"] == "guest.power.reboot")
        .expect("power action logged");
    assert_eq!(entry["target"], "guest:123");
    assert_eq!(entry["username"], "demo");
    assert_eq!(entry["result"], "pending");

    // Starting a running guest is refused by Proxmox and logged as an error.
    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/proxmox/guests/108/power",
            Some(&cookie),
            Some(json!({"action": "start"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);

    let (status, task, _) = app
        .call(
            "GET",
            &format!("/api/v1/proxmox/tasks/{}", body["upid"].as_str().unwrap()),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["kind"], "vzreboot");
}

#[tokio::test]
async fn guests_only_see_allowed_services_and_cannot_act() {
    let app = App::demo().await;
    let cookie = app.login("guest", "guest").await;

    let (_, services, _) = app
        .call("GET", "/api/v1/services", Some(&cookie), None)
        .await;
    let mut ids: Vec<&str> = services
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    ids.sort();
    assert_eq!(ids, ["bookstack", "proxmox", "vaultwarden"]);

    let (status, _, _) = app
        .call("GET", "/api/v1/services/podgrab", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = app
        .call(
            "GET",
            "/api/v1/services/podgrab/snapshot",
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, guests, _) = app
        .call("GET", "/api/v1/proxmox/guests", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(guests.as_array().unwrap().len() > 20);

    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/proxmox/guests/123/power",
            Some(&cookie),
            Some(json!({"action": "start"})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/services/vaultwarden/actions/refresh",
            Some(&cookie),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    for (method, path) in [
        ("GET", "/api/v1/users"),
        ("POST", "/api/v1/pairing/start"),
        ("PUT", "/api/v1/settings"),
    ] {
        let (status, _, _) = app.call(method, path, Some(&cookie), Some(json!({}))).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }

    let (_, overview, _) = app
        .call("GET", "/api/v1/overview", Some(&cookie), None)
        .await;
    assert_eq!(overview["highlights"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn members_can_act_but_not_manage() {
    let app = App::demo().await;
    let owner = app.login("demo", "demo").await;
    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/users",
            Some(&owner),
            Some(json!({"username": "mara", "password": "longenough", "role": "member"})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let member = app.login("mara", "longenough").await;

    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/services",
            Some(&member),
            Some(json!({"kind": "health_check", "name": "X", "base_url": "http://x"})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/proxmox/guests/123/power",
            Some(&member),
            Some(json!({"action": "shutdown", "confirm": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn service_crud_never_returns_secrets() {
    let app = App::demo().await;
    let owner = app.login("demo", "demo").await;

    let (status, body, _) = app
        .call(
            "POST",
            "/api/v1/services",
            Some(&owner),
            Some(json!({
                "kind": "proxmox", "name": "Second cluster", "base_url": "https://pve2.example.lan:8006",
                "settings": {"token_id": "quorum@pve!quorum"},
                "secrets": {"token_secret": "super-secret-value"}
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["id"], "second-cluster");
    assert_eq!(body["secrets"]["token_secret"]["set"], true);
    assert_eq!(body["secrets"]["token_secret"]["display"], "••••");
    assert!(!body.to_string().contains("super-secret-value"));

    // Stored encrypted, too.
    let row = app
        .state
        .db
        .service("second-cluster")
        .await
        .unwrap()
        .unwrap();
    assert!(!row.secrets["token_secret"].contains("super-secret-value"));

    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/services",
            Some(&owner),
            Some(json!({"kind": "proxmox", "name": "Bad", "base_url": "http://x"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body, _) = app
        .call(
            "PATCH",
            "/api/v1/services/second-cluster",
            Some(&owner),
            Some(json!({"name": "Lab cluster", "secrets": {"token_secret": null}})),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "secret is required for proxmox: {body}"
    );

    let (status, body, _) = app
        .call(
            "POST",
            "/api/v1/services/second-cluster/disable",
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["health"], "disabled");

    let (status, _, _) = app
        .call(
            "DELETE",
            "/api/v1/services/second-cluster",
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, log, _) = app
        .call("GET", "/api/v1/activity", Some(&owner), None)
        .await;
    let actions: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(actions.contains(&"service.create") && actions.contains(&"service.delete"));
}

#[tokio::test]
async fn pairing_issues_a_revocable_device_token() {
    let app = App::demo().await;
    let owner = app.login("demo", "demo").await;
    let (status, started, _) = app
        .call(
            "POST",
            "/api/v1/pairing/start",
            Some(&owner),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let code = started["code"].as_str().unwrap();
    assert!(
        started["qr_payload"]
            .as_str()
            .unwrap()
            .starts_with("quorum://pair?")
    );

    let (status, done, _) = app
        .call(
            "POST",
            "/api/v1/pairing/complete",
            None,
            Some(json!({"code": code.to_lowercase(), "device_name": "iPhone", "platform": "ios"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token = done["token"].as_str().unwrap().to_owned();

    // The code is single-use.
    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/pairing/complete",
            None,
            Some(json!({"code": code, "device_name": "Other", "platform": "pwa"})),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, me, _) = app.call("GET", "/api/v1/auth/me", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["auth"], "device");

    // The iPhone registers its APNs token for itself; the owner's session can't do it for it.
    let id = done["device"]["id"].as_i64().unwrap();
    let push = format!("/api/v1/devices/{id}/push-token");
    let body = json!({"token": "A1B2".repeat(16), "environment": "sandbox"});
    let (status, _, _) = app
        .call("POST", &push, Some(&owner), Some(body.clone()))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = app
        .call(
            "POST",
            &push,
            Some(&token),
            Some(json!({"token": "not hex"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = app.call("POST", &push, Some(&token), Some(body)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, devices, _) = app.call("GET", "/api/v1/devices", Some(&owner), None).await;
    let d = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .unwrap();
    assert_eq!(
        (d["push"].as_bool(), d["push_env"].as_str()),
        (Some(true), Some("sandbox"))
    );
    let targets = app.state.db.push_targets().await.unwrap();
    assert_eq!(targets[0].token, "a1b2".repeat(16), "stored lower-case");
    let (status, _, _) = app.call("DELETE", &push, Some(&token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(app.state.db.push_targets().await.unwrap().is_empty());

    // The widget summary works with a device token.
    let (status, w, _) = app.call("GET", "/api/v1/widget", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(w["status"]["total"].as_u64().unwrap() > 0);
    assert!(w["downloads"]["dl_speed"].as_u64().is_some(), "{w}");
    assert!(w["streams"]["count"].as_u64().is_some(), "{w}");
    assert!(!w["services"].as_array().unwrap().is_empty());

    let (status, _, _) = app
        .call(
            "DELETE",
            &format!("/api/v1/devices/{id}"),
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = app.call("GET", "/api/v1/auth/me", Some(&token), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The PWA gets its device token as an HttpOnly cookie, never in the body.
    let (_, started, _) = app
        .call(
            "POST",
            "/api/v1/pairing/start",
            Some(&owner),
            Some(json!({})),
        )
        .await;
    let (status, done, cookie) = app
        .call(
            "POST",
            "/api/v1/pairing/complete",
            None,
            Some(json!({"code": started["code"], "device_name": "Pixel", "platform": "pwa"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(done["token"].is_null());
    let cookie = cookie.expect("device cookie");
    assert!(cookie.starts_with("quorum_device=qd_"), "{cookie}");
    let (status, me, _) = app
        .call("GET", "/api/v1/auth/me", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["auth"], "device");
    // Signing out unpairs the device.
    let (status, _, _) = app
        .call("POST", "/api/v1/auth/logout", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = app
        .call("GET", "/api/v1/auth/me", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn owner_setup_with_one_time_token() {
    let db = quorum_server::db::Db::in_memory().await.unwrap();
    let state = AppState::new(
        Config::parse("").unwrap(),
        db,
        quorum_server::crypto::SecretBox::ephemeral(),
        false,
    );
    quorum_server::app::bootstrap_owner(&state).await.unwrap();
    let token = state.setup_token.lock().unwrap().clone().unwrap();
    let app = App::from_state(state);

    let (_, status_body, _) = app.call("GET", "/api/v1/auth/setup", None, None).await;
    assert_eq!(status_body["needs_setup"], true);
    let req =
        |t: &str| json!({"setup_token": t, "username": "janick", "password": "correct horse"});
    let (status, _, _) = app
        .call("POST", "/api/v1/auth/setup", None, Some(req("WRONG")))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, me, cookie) = app
        .call("POST", "/api/v1/auth/setup", None, Some(req(&token)))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["role"], "owner");
    assert!(cookie.is_some());
    let (status, _, _) = app
        .call("POST", "/api/v1/auth/setup", None, Some(req(&token)))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn overview_health_and_proxmox_reads() {
    let app = App::demo().await;
    let owner = app.login("demo", "demo").await;
    // Let the health checks run once.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, overview, _) = app
        .call("GET", "/api/v1/overview", Some(&owner), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(overview["status"]["total"], 23);
    // Health checks: Podgrab slow, draw.io down. Media: Sonarr indexer warning, Prowlarr
    // failing indexer, qBittorrent errored torrent. Network: a certificate expiring in 9 d.
    assert_eq!(
        overview["status"]["message"], "6 problems",
        "{}",
        overview["problems"]
    );

    let (_, health, _) = app.call("GET", "/api/v1/health", Some(&owner), None).await;
    let entries = health["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["id"] == "guest:123"));
    assert_eq!(entries[0]["timeline"].as_array().unwrap().len(), 24);

    let (_, cluster, _) = app
        .call("GET", "/api/v1/proxmox/cluster", Some(&owner), None)
        .await;
    assert_eq!(cluster["cluster"]["quorate"], true);
    let (_, guest, _) = app
        .call("GET", "/api/v1/proxmox/guests/114", Some(&owner), None)
        .await;
    assert_eq!(guest["linked_services"][0]["id"], "vaultwarden");
    assert_eq!(guest["last_backup"]["kind"], "vzdump");
    let (_, tagged, _) = app
        .call("GET", "/api/v1/proxmox/guests/117", Some(&owner), None)
        .await;
    assert_eq!(tagged["guest"]["linked_services"][0], "bookstack");
    let (status, rrd, _) = app
        .call(
            "GET",
            "/api/v1/proxmox/nodes/pve-01/rrd?range=day",
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rrd.as_array().unwrap().len(), 70);
    let (status, _, _) = app
        .call(
            "GET",
            "/api/v1/proxmox/nodes/..%2Fx/rrd",
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sse_stream_opens_for_signed_in_users() {
    let app = App::demo().await;
    let owner = app.login("demo", "demo").await;
    let req = Request::get("/api/v1/events?watch=proxmox")
        .header(header::COOKIE, &owner)
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    // Reverse proxies (nginx / Nginx Proxy Manager) must not buffer the stream.
    assert_eq!(response.headers()["x-accel-buffering"], "no");
    let mut body = response.into_body();
    let frame = tokio::time::timeout(Duration::from_secs(2), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let text = String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap();
    assert!(text.starts_with("event: snapshot"), "{text}");
}

#[tokio::test]
async fn media_overview_tracks_requests_across_services() {
    let app = App::demo().await;
    let owner = app.login("demo", "demo").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (status, m, _) = app
        .call("GET", "/api/v1/media/overview", Some(&owner), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{m}");
    assert_eq!(m["counts"]["streams"], 3); // 2 Jellyfin + 1 Navidrome
    assert_eq!(m["counts"]["transcodes"], 1);
    let np = m["now_playing"].as_array().unwrap();
    let orbit = np.iter().find(|n| n["title"] == "The Long Orbit").unwrap();
    assert_eq!(orbit["transcode_reason"], "audio codec not supported");
    assert_eq!(
        orbit["detail"].as_str().unwrap(),
        "HEVC 1080p → H.264 · 12 Mbit/s"
    );

    let req = |title: &str| {
        m["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["request"]["title"] == title)
            .unwrap()
            .clone()
    };
    let state = |r: &Value, i: usize| r["steps"][i]["state"].as_str().unwrap().to_owned();
    // Glass Meridian: approved, grabbed by Radarr, downloading in qBittorrent.
    let glass = req("Glass Meridian");
    assert_eq!(
        (state(&glass, 2).as_str(), state(&glass, 3).as_str()),
        ("done", "current")
    );
    assert!(
        glass["steps"][3]["detail"]
            .as_str()
            .unwrap()
            .starts_with("62 %")
    );
    assert_eq!(
        glass["torrent_hash"],
        "8c212779b4abde7c6bc608063a0d008b7e40ce32"
    );
    // Saltwater Kings: grabbed but stuck with no seeders.
    let salt = req("Saltwater Kings");
    assert_eq!(state(&salt, 2), "stuck");
    assert_eq!(salt["steps"][2]["detail"], "No seeders · 6 h");
    // Harbor Lights is available, so it has left the tracker (Seerr lists it).
    assert!(
        !m["requests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["request"]["title"] == "Harbor Lights")
    );
    // Static Bloom: waiting for approval.
    assert_eq!(state(&req("Static Bloom"), 1), "current");
    assert!(!m["calendar"].as_array().unwrap().is_empty());

    let (_, results, _) = app
        .call("GET", "/api/v1/media/search?q=glass", Some(&owner), None)
        .await;
    let kinds: Vec<&str> = results
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"movie") && kinds.contains(&"torrent"),
        "{results}"
    );
}

#[tokio::test]
async fn media_actions_queries_and_confirm() {
    let app = App::demo().await;
    let owner = app.login("demo", "demo").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Demo media state is shared by every test in this binary, so only touch items the
    // other tests don't assert on: two seeding ISOs and the Copper Valley request.
    let hash = format!("{:040x}", 0xabc000 + 16);
    let other = format!("{:040x}", 0xabc000 + 13);
    let (status, body, _) = app
        .call(
            "POST",
            "/api/v1/services/qbittorrent/actions/delete",
            Some(&owner),
            Some(json!({"args": {"hashes": [hash]}})),
        )
        .await;
    assert_eq!(
        (status, body["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("confirm_required"))
    );
    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/services/qbittorrent/actions/delete",
            Some(&owner),
            Some(json!({"confirm": true, "args": {"hashes": [hash], "delete_files": false}})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = app
        .call(
            "POST",
            "/api/v1/services/qbittorrent/actions/stop",
            Some(&owner),
            Some(json!({"args": {"hashes": [other]}})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body, _) = app
        .call(
            "POST",
            "/api/v1/services/seerr/actions/approve",
            Some(&owner),
            Some(json!({"args": {"id": 30}})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Copper Valley approved");

    let (status, lib, _) = app
        .call(
            "GET",
            "/api/v1/services/radarr/query/library",
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        lib.as_array()
            .unwrap()
            .iter()
            .any(|m| m["title"] == "Glass Meridian")
    );
    let (status, found, _) = app
        .call(
            "GET",
            "/api/v1/services/seerr/query/search?q=harbor",
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(found.as_array().unwrap().len(), 2);
    let (status, _, _) = app
        .call(
            "GET",
            "/api/v1/services/seerr/query/nope",
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = app
        .call(
            "GET",
            "/api/v1/services/radarr/image?path=/etc/passwd",
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, log, _) = app
        .call("GET", "/api/v1/activity", Some(&owner), None)
        .await;
    let actions: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(
        actions.contains(&"service.action.delete") && actions.contains(&"service.action.approve")
    );
}

/// swift-openapi-generator skips properties whose `oneOf` includes a bare `null`; optional
/// references are written as a plain `$ref` outside `required` instead.
#[test]
fn openapi_has_no_null_one_of() {
    let spec = quorum_server::api::openapi();
    let text = spec.to_string();
    assert!(
        !text.contains(r#"{"type":"null"}"#),
        "a oneOf with a null branch is left"
    );
    let problem = &spec["components"]["schemas"]["Problem"];
    assert_eq!(
        problem["properties"]["quick_action"]["$ref"],
        "#/components/schemas/QuickAction"
    );
    assert!(
        !problem["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "quick_action")
    );
}
