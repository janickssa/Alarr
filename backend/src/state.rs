//! Shared application state.

use std::ops::Deref;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use crate::alerts::channels::NotificationChannel;
use crate::auth::RateLimiter;
use crate::config::Config;
use crate::crypto::SecretBox;
use crate::db::{ActivityEntry, ActivityResult, Db, User};
use crate::events::{ActionEvent, Event, EventHub};
use crate::modules::{Health, ModuleData, ServiceKind, ServiceSnapshot};
use crate::poller::{PollDeps, SnapshotCache, Supervisor};
use crate::services;

pub struct AppStateInner {
    pub config: Config,
    pub db: Db,
    pub secrets: SecretBox,
    pub events: EventHub,
    pub cache: Arc<SnapshotCache>,
    pub supervisor: Supervisor,
    pub limiter: RateLimiter,
    /// One-time owner setup token, set while no user exists.
    pub setup_token: Mutex<Option<String>>,
    pub demo: bool,
    pub started_at: DateTime<Utc>,
    /// Notification channels for alerts (ntfy, push).
    pub channels: Vec<Arc<dyn NotificationChannel>>,
}

#[derive(Clone)]
pub struct AppState(pub Arc<AppStateInner>);

impl Deref for AppState {
    type Target = AppStateInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AppState {
    pub fn new(config: Config, db: Db, secrets: SecretBox, demo: bool) -> Self {
        let events = EventHub::new();
        let cache = Arc::new(SnapshotCache::default());
        let supervisor = Supervisor::new(
            PollDeps {
                db: db.clone(),
                cache: cache.clone(),
                events: events.clone(),
            },
            demo,
        );
        let channels = crate::alerts::channels::from_config(&config);
        Self(Arc::new(AppStateInner {
            channels,
            config,
            db,
            secrets,
            events,
            cache,
            supervisor,
            limiter: RateLimiter::default(),
            setup_token: Mutex::new(None),
            demo,
            started_at: Utc::now(),
        }))
    }

    /// Start pollers for every enabled service.
    pub async fn start_pollers(&self) -> anyhow::Result<()> {
        for row in self.db.list_services().await? {
            if row.enabled {
                self.start_service(&row);
            }
        }
        Ok(())
    }

    /// Start (or restart) one service's poller. A broken config shows up as `down`.
    pub fn start_service(&self, row: &crate::db::ServiceRow) {
        let result = services::instance_config(row, &self.secrets)
            .and_then(|cfg| self.supervisor.start(&cfg));
        if let Err(e) = result {
            tracing::warn!(service = %row.id, error = %e, "cannot start service");
            self.supervisor.stop(&row.id);
            self.cache.set(ServiceSnapshot {
                service_id: row.id.clone(),
                kind: ServiceKind::parse(&row.kind).unwrap_or(ServiceKind::HealthCheck),
                health: Health::Down,
                health_reason: Some(format!("configuration error: {e}")),
                response_ms: None,
                updated_at: Utc::now(),
                version: None,
                stale: false,
                data: ModuleData::None,
            });
        }
    }

    /// Write to the activity log and broadcast an `action` event.
    pub async fn log(
        &self,
        user: Option<&User>,
        action: &str,
        target: &str,
        result: ActivityResult,
        detail: Option<&str>,
        service_id: Option<&str>,
    ) -> anyhow::Result<ActivityEntry> {
        let entry = self
            .db
            .log_activity(user, action, target, result, detail)
            .await?;
        self.events.send(Event::Action(ActionEvent {
            service_id: service_id.map(str::to_owned),
            entry: entry.clone(),
        }));
        Ok(entry)
    }

    /// Complete a pending activity entry and broadcast it.
    pub async fn finish(
        &self,
        id: i64,
        result: ActivityResult,
        detail: Option<&str>,
        service_id: Option<&str>,
    ) {
        match self.db.finish_activity(id, result, detail).await {
            Ok(Some(entry)) => self.events.send(Event::Action(ActionEvent {
                service_id: service_id.map(str::to_owned),
                entry,
            })),
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e, "could not update activity entry"),
        }
    }
}
