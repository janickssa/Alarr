//! Per-instance polling tasks with fast mode and backoff, and the in-memory snapshot cache.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::db::Db;
use crate::events::{Event, EventHub, HealthChange};
use crate::modules::{
    Health, InstanceConfig, ModuleData, ServiceKind, ServiceModule, ServiceSnapshot, build_module,
    describe_error,
};

/// How long a "client is viewing this" mark keeps an instance in fast mode.
const WATCH_TTL: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(300);
const POLL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct SnapshotCache {
    snapshots: RwLock<HashMap<String, ServiceSnapshot>>,
    watched: Mutex<HashMap<String, Instant>>,
}

impl SnapshotCache {
    pub fn get(&self, id: &str) -> Option<ServiceSnapshot> {
        self.snapshots.read().expect("lock").get(id).cloned()
    }

    pub fn all(&self) -> Vec<ServiceSnapshot> {
        self.snapshots
            .read()
            .expect("lock")
            .values()
            .cloned()
            .collect()
    }

    pub(crate) fn set(&self, snapshot: ServiceSnapshot) -> Option<ServiceSnapshot> {
        self.snapshots
            .write()
            .expect("lock")
            .insert(snapshot.service_id.clone(), snapshot)
    }

    pub fn remove(&self, id: &str) {
        self.snapshots.write().expect("lock").remove(id);
    }

    /// Mark an instance as being viewed. Returns true if it was not watched before.
    pub fn watch(&self, id: &str) -> bool {
        let mut watched = self.watched.lock().expect("lock");
        let now = Instant::now();
        let was = watched.get(id).is_some_and(|until| *until > now);
        watched.insert(id.to_owned(), now + WATCH_TTL);
        !was
    }

    pub fn is_watched(&self, id: &str) -> bool {
        self.watched
            .lock()
            .expect("lock")
            .get(id)
            .is_some_and(|until| *until > Instant::now())
    }
}

#[derive(Clone)]
pub struct PollDeps {
    pub db: Db,
    pub cache: Arc<SnapshotCache>,
    pub events: EventHub,
}

struct Poller {
    module: Arc<dyn ServiceModule>,
    wake: Arc<Notify>,
    task: JoinHandle<()>,
}

pub struct Supervisor {
    pollers: Mutex<HashMap<String, Poller>>,
    deps: PollDeps,
    demo: bool,
}

impl Supervisor {
    pub fn new(deps: PollDeps, demo: bool) -> Self {
        Self {
            pollers: Mutex::new(HashMap::new()),
            deps,
            demo,
        }
    }

    /// Start (or restart) polling an enabled instance.
    pub fn start(&self, cfg: &InstanceConfig) -> anyhow::Result<()> {
        let module = build_module(cfg, self.demo)?;
        let wake = Arc::new(Notify::new());
        let (normal, fast) = match cfg.setting_u64("poll_interval_s") {
            Some(s) => (Duration::from_secs(s.max(1)), Duration::from_secs(s.max(1))),
            None => cfg.kind.poll_intervals(),
        };
        let task = tokio::spawn(run(
            cfg.id.clone(),
            cfg.kind,
            module.clone(),
            wake.clone(),
            self.deps.clone(),
            normal,
            fast,
        ));
        if let Some(old) = self
            .pollers
            .lock()
            .expect("lock")
            .insert(cfg.id.clone(), Poller { module, wake, task })
        {
            old.task.abort();
        }
        Ok(())
    }

    /// Stop polling and forget the cached snapshot.
    pub fn stop(&self, id: &str) {
        if let Some(p) = self.pollers.lock().expect("lock").remove(id) {
            p.task.abort();
        }
        self.deps.cache.remove(id);
    }

    pub fn module(&self, id: &str) -> Option<Arc<dyn ServiceModule>> {
        self.pollers
            .lock()
            .expect("lock")
            .get(id)
            .map(|p| p.module.clone())
    }

    /// Poll now (after an action, or on a client's request).
    pub fn wake(&self, id: &str) {
        if let Some(p) = self.pollers.lock().expect("lock").get(id) {
            p.wake.notify_one();
        }
    }

    /// Mark as viewed (fast mode); wakes the poller when it was idle on the slow interval.
    pub fn watch(&self, id: &str) {
        if self.deps.cache.watch(id) {
            self.wake(id);
        }
    }

    pub fn running(&self) -> Vec<String> {
        self.pollers.lock().expect("lock").keys().cloned().collect()
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        for (_, p) in self.pollers.lock().expect("lock").drain() {
            p.task.abort();
        }
    }
}

/// Remembers the last minute and state written per target so each poll doesn't write.
#[derive(Default)]
struct UptimeWriter {
    last: HashMap<String, (i64, i64)>,
}

impl UptimeWriter {
    async fn record(&mut self, db: &Db, target: String, health: Health, response_ms: Option<u64>) {
        let Some(state) = health.uptime_state() else {
            return;
        };
        let now = crate::db::now();
        let minute = now / 60;
        if let Some((m, s)) = self.last.get(&target)
            && *m == minute
            && *s <= state
        {
            return;
        }
        if let Err(e) = db.record_uptime(&target, now, state, response_ms).await {
            tracing::warn!(error = %e, "could not record uptime");
            return;
        }
        self.last.insert(target, (minute, state));
    }
}

async fn run(
    id: String,
    kind: ServiceKind,
    module: Arc<dyn ServiceModule>,
    wake: Arc<Notify>,
    deps: PollDeps,
    normal: Duration,
    fast: Duration,
) {
    let mut backoff: Option<Duration> = None;
    let mut uptime = UptimeWriter::default();
    loop {
        let previous = deps.cache.get(&id);
        let result = match tokio::time::timeout(POLL_TIMEOUT, module.poll()).await {
            Ok(r) => r,
            Err(_) => Err(anyhow::anyhow!(
                "timeout after {} s",
                POLL_TIMEOUT.as_secs()
            )),
        };
        let snapshot = match result {
            Ok(o) => ServiceSnapshot {
                service_id: id.clone(),
                kind,
                health: o.health,
                health_reason: o.health_reason,
                response_ms: o.response_ms,
                updated_at: chrono::Utc::now(),
                version: o.version,
                stale: false,
                data: o.data,
            },
            Err(e) => ServiceSnapshot {
                service_id: id.clone(),
                kind,
                health: Health::Down,
                health_reason: Some(describe_error(&e)),
                response_ms: None,
                updated_at: chrono::Utc::now(),
                version: previous.as_ref().and_then(|p| p.version.clone()),
                // Keep the last good data so pages can still show it (marked stale).
                stale: previous.is_some(),
                data: previous
                    .as_ref()
                    .map(|p| p.data.clone())
                    .unwrap_or(ModuleData::None),
            },
        };

        let previous_health = previous.map(|p| p.health);
        if previous_health != Some(snapshot.health) {
            tracing::info!(service = %id, health = ?snapshot.health, reason = ?snapshot.health_reason, "health changed");
            deps.events.send(Event::Health(HealthChange {
                service_id: id.clone(),
                previous: previous_health,
                health: snapshot.health,
                reason: snapshot.health_reason.clone(),
            }));
        }

        uptime
            .record(
                &deps.db,
                format!("svc:{id}"),
                snapshot.health,
                snapshot.response_ms,
            )
            .await;
        if let (ModuleData::Proxmox(data), false) = (&snapshot.data, snapshot.stale) {
            for g in data.guests.iter().filter(|g| !g.template) {
                uptime
                    .record(
                        &deps.db,
                        format!("guest:{}", g.vmid),
                        g.status.health(),
                        None,
                    )
                    .await;
            }
        }

        let down = snapshot.health == Health::Down;
        deps.cache.set(snapshot.clone());
        deps.events.send(Event::Snapshot(snapshot));

        let base = if deps.cache.is_watched(&id) {
            fast
        } else {
            normal
        };
        let delay = if down {
            let next = backoff.map_or(base * 2, |b| (b * 2).min(MAX_BACKOFF));
            backoff = Some(next);
            next
        } else {
            backoff = None;
            base
        };
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = wake.notified() => {}
        }
    }
}
