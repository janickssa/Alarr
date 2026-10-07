//! Building the application: database, secrets, demo data, seed import, owner bootstrap.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use serde_json::json;

use crate::api::services::{CreateService, new_row};
use crate::config::{Config, secret_key_from_env};
use crate::crypto::{self, SecretBox};
use crate::db::{Db, Role, ServiceRow};
use crate::modules::{ServiceKind, catalogue};
use crate::services;
use crate::state::AppState;

/// Build the state for `serve` (or `serve --demo`) and start the pollers.
pub async fn build(config: Config, demo: bool) -> anyhow::Result<AppState> {
    let state = if demo {
        let state = AppState::new(config, Db::in_memory().await?, SecretBox::ephemeral(), true);
        seed_demo(&state).await?;
        state
    } else {
        let key = secret_key_from_env()?;
        let db = Db::open(&config.server.database).await?;
        let state = AppState::new(config, db, SecretBox::new(&key), false);
        import_seed(&state).await?;
        bootstrap_owner(&state).await?;
        state
    };
    state.start_pollers().await?;
    Ok(state)
}

/// While no user exists, print a one-time setup token for creating the owner.
pub async fn bootstrap_owner(state: &AppState) -> anyhow::Result<()> {
    if state.db.count_users().await? > 0 {
        return Ok(());
    }
    let token = crypto::random_code(12);
    tracing::warn!(
        "No owner account yet. Open the web UI and create it with this one-time setup token: {token}"
    );
    *state.setup_token.lock().expect("lock") = Some(token);
    Ok(())
}

#[derive(Deserialize)]
struct SeedFile {
    #[serde(default)]
    service: Vec<CreateService>,
}

/// Import `services.seed.toml` once, when no services exist yet.
async fn import_seed(state: &AppState) -> anyhow::Result<()> {
    let Some(path) = state.config.seed_path() else {
        return Ok(());
    };
    if !state.db.list_services().await?.is_empty() {
        return Ok(());
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let seed: SeedFile = toml::from_str(&text).with_context(|| format!("in {}", path.display()))?;
    let mut rows: Vec<ServiceRow> = Vec::new();
    for req in seed.service {
        let name = req.name.clone();
        match new_row(state, req, &rows) {
            Ok(row) => {
                state.db.save_service(&row).await?;
                rows.push(row);
            }
            Err(e) => tracing::warn!(service = %name, error = ?e, "skipping seed entry"),
        }
    }
    tracing::info!(count = rows.len(), path = %path.display(), "imported services from seed file");
    Ok(())
}

/// Demo data: owner `demo`/`demo`, guest `guest`/`guest`, Proxmox and health checks.
async fn seed_demo(state: &AppState) -> anyhow::Result<()> {
    let db = &state.db;
    db.create_user(
        "demo",
        &crypto::hash_password("demo")?,
        Role::Owner,
        &[],
        false,
    )
    .await?;
    let guest_services = vec![
        "proxmox".to_owned(),
        "vaultwarden".to_owned(),
        "bookstack".to_owned(),
    ];
    db.create_user(
        "guest",
        &crypto::hash_password("guest")?,
        Role::Guest,
        &guest_services,
        false,
    )
    .await?;

    let mut order = 0;
    let mut add = |id: &str,
                   kind: ServiceKind,
                   base_url: &str,
                   linked: Option<u32>,
                   settings: serde_json::Value,
                   secrets: BTreeMap<String, String>| {
        let entry = catalogue::find(id).expect("demo preset in catalogue");
        order += 1;
        let mut settings = settings;
        if let Some(path) = entry.health_path {
            settings["path"] = json!(path);
        }
        ServiceRow {
            id: id.to_owned(),
            kind: kind.as_str().to_owned(),
            preset: Some(entry.preset.to_owned()),
            name: entry.name.to_owned(),
            base_url: Some(base_url.to_owned()),
            enabled: true,
            category: None,
            settings,
            secrets: secrets
                .into_iter()
                .map(|(k, v)| (k.clone(), services::seal(&state.secrets, id, &k, &v)))
                .collect(),
            linked_vmid: linked,
            sort_order: order,
        }
    };
    let mut rows = vec![add(
        "proxmox",
        ServiceKind::Proxmox,
        "https://10.0.20.11:8006",
        None,
        json!({"token_id": "quorum@pve!quorum"}),
        [("token_secret".to_owned(), "demo-secret".to_owned())].into(),
    )];
    // Some are linked by config, the others by a `svc-<id>` tag on the container.
    for (id, ct, linked) in [
        ("vaultwarden", 114, true),
        ("podgrab", 118, true),
        ("apt-cacher-ng", 103, false),
        ("smb-share", 111, false),
        ("bookstack", 117, false),
        ("drawio", 116, true),
        ("kiwix", 122, false),
        ("mkdocs", 128, false),
    ] {
        rows.push(add(
            id,
            ServiceKind::HealthCheck,
            &format!("http://10.0.20.{ct}"),
            linked.then_some(ct),
            json!({}),
            BTreeMap::new(),
        ));
    }
    for (id, kind, ct) in [
        ("jellyfin", ServiceKind::Jellyfin, 108),
        ("jellyfin-public", ServiceKind::Jellyfin, 105),
        ("seerr", ServiceKind::Seerr, 126),
        ("sonarr", ServiceKind::Sonarr, 123),
        ("radarr", ServiceKind::Radarr, 124),
        ("prowlarr", ServiceKind::Prowlarr, 125),
        ("bazarr", ServiceKind::Bazarr, 127),
        ("qbittorrent", ServiceKind::Qbittorrent, 120),
        ("navidrome", ServiceKind::Navidrome, 119),
    ] {
        let (settings, secret) = match kind {
            ServiceKind::Qbittorrent | ServiceKind::Navidrome => {
                (json!({"username": "demo"}), "password")
            }
            _ => (json!({}), "api_key"),
        };
        rows.push(add(
            id,
            kind,
            &format!("http://10.0.20.{ct}"),
            Some(ct),
            settings,
            [(secret.to_owned(), "demo".to_owned())].into(),
        ));
    }
    for row in &rows {
        db.save_service(row).await?;
    }
    tracing::warn!(
        "DEMO MODE: fake data, in-memory database. Sign in as demo/demo (owner) or guest/guest."
    );
    Ok(())
}

/// Hourly housekeeping: uptime downsampling, expired sessions and pairing codes, rate limits.
pub fn spawn_maintenance(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            let now = crate::db::now();
            if let Err(e) = state.db.downsample_uptime(now).await {
                tracing::warn!(error = %e, "uptime downsampling failed");
            }
            if let Err(e) = state.db.cleanup_expired(now).await {
                tracing::warn!(error = %e, "cleanup failed");
            }
            state.limiter.cleanup();
        }
    });
}
