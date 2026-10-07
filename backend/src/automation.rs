//! Automation: auto-throttle qBittorrent (alternative speed limits on) while Jellyfin has
//! active streams, optionally only remote ones.
//!
//! Hysteresis so it doesn't flap: streams must play for [`ON_DELAY_S`] before throttling and
//! be gone for [`OFF_DELAY_S`] before lifting it. It only lifts limits it turned on itself, and
//! if the owner turns them off by hand during streaming it leaves them off until streaming
//! stops. Every switch is written to the activity log.

use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::db::ActivityResult;
use crate::modules::{ModuleData, ServiceKind};
use crate::state::AppState;

pub const ON_DELAY_S: i64 = 20;
pub const OFF_DELAY_S: i64 = 300;
const SETTINGS_KEY: &str = "automation.throttle";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct ThrottleSettings {
    pub enabled: bool,
    /// Count only streams from outside the LAN (Jellyfin's "remote" sessions).
    pub only_remote: bool,
    /// qBittorrent service to throttle; the first one when unset.
    pub qbittorrent_id: Option<String>,
}

impl ThrottleSettings {
    pub async fn load(state: &AppState) -> anyhow::Result<Self> {
        Ok(state
            .db
            .setting(SETTINGS_KEY)
            .await?
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default())
    }

    pub async fn save(&self, state: &AppState) -> anyhow::Result<()> {
        state
            .db
            .set_setting(SETTINGS_KEY, &serde_json::to_string(self)?)
            .await
    }
}

/// The hysteresis state machine, separate from I/O so it can be tested.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Throttle {
    /// The limits are on because of us.
    pub engaged: bool,
    /// The owner turned the limits off while we had them on: hands off until streams stop.
    pub overridden: bool,
    streams_since: Option<DateTime<Utc>>,
    idle_since: Option<DateTime<Utc>>,
}

impl Throttle {
    /// Given the stream count and whether alternative limits are on now, return the state
    /// the limits should be switched to, if any. Call [`Throttle::switched`] after switching.
    pub fn step(&mut self, now: DateTime<Utc>, streams: usize, alt_on: bool) -> Option<bool> {
        if streams > 0 {
            self.idle_since = None;
            let since = *self.streams_since.get_or_insert(now);
            if self.engaged && !alt_on {
                // Turned off by hand.
                self.engaged = false;
                self.overridden = true;
                return None;
            }
            if !self.engaged
                && !self.overridden
                && !alt_on
                && now - since >= Duration::seconds(ON_DELAY_S)
            {
                return Some(true);
            }
        } else {
            self.streams_since = None;
            self.overridden = false;
            if self.engaged {
                let since = *self.idle_since.get_or_insert(now);
                if now - since >= Duration::seconds(OFF_DELAY_S) {
                    if alt_on {
                        return Some(false);
                    }
                    self.engaged = false;
                }
            }
        }
        None
    }

    pub fn switched(&mut self, on: bool) {
        self.engaged = on;
        if !on {
            self.idle_since = None;
        }
    }
}

/// Current status for the qBittorrent page.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ThrottleStatus {
    #[serde(flatten)]
    pub settings: ThrottleSettings,
    /// The limits are on because of auto-throttle right now.
    pub engaged: bool,
    /// Streams counted right now (after the remote-only filter).
    pub streams: usize,
}

/// Count streams across enabled Jellyfin services; `None` if no Jellyfin has fresh data.
pub fn count_streams(state: &AppState, only_remote: bool) -> Option<usize> {
    let mut any = false;
    let mut n = 0;
    for s in state.cache.all() {
        if s.kind != ServiceKind::Jellyfin || s.stale {
            continue;
        }
        if let ModuleData::Jellyfin(d) = &s.data {
            any = true;
            n += d
                .streams
                .iter()
                .filter(|st| !st.paused && (!only_remote || st.remote))
                .count();
        }
    }
    any.then_some(n)
}

fn target(state: &AppState, settings: &ThrottleSettings) -> Option<(String, bool)> {
    state
        .cache
        .all()
        .into_iter()
        .filter(|s| s.kind == ServiceKind::Qbittorrent && !s.stale)
        .filter(|s| {
            settings
                .qbittorrent_id
                .as_ref()
                .is_none_or(|id| *id == s.service_id)
        })
        .find_map(|s| match &s.data {
            ModuleData::Qbittorrent(d) => Some((s.service_id.clone(), d.alt_speed)),
            _ => None,
        })
}

/// Evaluate once: count streams, step the state machine and switch the limits if needed.
pub async fn tick(
    state: &AppState,
    throttle: &mut Throttle,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let settings = ThrottleSettings::load(state).await?;
    if !settings.enabled {
        *throttle = Throttle::default();
        return Ok(());
    }
    let (Some(streams), Some((qbt, alt_on))) = (
        count_streams(state, settings.only_remote),
        target(state, &settings),
    ) else {
        return Ok(());
    };
    let Some(on) = throttle.step(now, streams, alt_on) else {
        return Ok(());
    };
    let Some(module) = state.supervisor.module(&qbt) else {
        return Ok(());
    };
    let detail = if on {
        format!(
            "{streams} {}stream{} on Jellyfin",
            if settings.only_remote { "remote " } else { "" },
            if streams == 1 { "" } else { "s" }
        )
    } else {
        "no streams for 5 min".into()
    };
    let result = module
        .run_action("toggle_alt_speed", serde_json::json!({}))
        .await;
    let ok = result.as_ref().is_ok_and(|r| r.ok);
    if ok {
        throttle.switched(on);
        state.supervisor.wake(&qbt);
    }
    state
        .log(
            None,
            if on {
                "automation.throttle.on"
            } else {
                "automation.throttle.off"
            },
            &qbt,
            if ok {
                ActivityResult::Ok
            } else {
                ActivityResult::Error
            },
            Some(&match result {
                Err(e) => format!("{detail}: {e}"),
                Ok(_) => detail,
            }),
            Some(&qbt),
        )
        .await?;
    Ok(())
}

/// Run the automation every 10 s. The state machine lives in `AppState` so the API can
/// report whether the limits are on because of it.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(StdDuration::from_secs(10));
        loop {
            interval.tick().await;
            let mut throttle = state.throttle.lock().await;
            if let Err(e) = tick(&state, &mut throttle, Utc::now()).await {
                tracing::warn!(error = %e, "automation failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hysteresis() {
        let t0 = Utc::now();
        let at = |s: i64| t0 + Duration::seconds(s);
        let mut t = Throttle::default();
        // A short stream doesn't throttle.
        assert_eq!(t.step(at(0), 1, false), None);
        assert_eq!(t.step(at(10), 0, false), None);
        assert_eq!(t.step(at(15), 1, false), None);
        assert_eq!(t.step(at(36), 1, false), Some(true));
        t.switched(true);
        // Brief gaps don't lift it.
        assert_eq!(t.step(at(60), 0, true), None);
        assert_eq!(t.step(at(200), 1, true), None);
        assert_eq!(t.step(at(300), 0, true), None);
        assert_eq!(t.step(at(599), 0, true), None);
        assert_eq!(t.step(at(600), 0, true), Some(false));
        t.switched(false);
        assert!(!t.engaged);
    }

    #[test]
    fn respects_manual_changes() {
        let t0 = Utc::now();
        let at = |s: i64| t0 + Duration::seconds(s);
        // Already on by hand: leave it, and never turn it off.
        let mut t = Throttle::default();
        assert_eq!(t.step(at(0), 1, true), None);
        assert_eq!(t.step(at(30), 1, true), None);
        assert_eq!(t.step(at(40), 0, true), None);
        assert_eq!(t.step(at(400), 0, true), None);

        // Turned off by hand while throttling: stay off until streams stop.
        let mut t = Throttle::default();
        t.step(at(0), 1, false);
        assert_eq!(t.step(at(20), 1, false), Some(true));
        t.switched(true);
        assert_eq!(t.step(at(30), 1, false), None);
        assert!(t.overridden);
        assert_eq!(t.step(at(100), 1, false), None);
        // Streams stop, then start again: back to normal.
        assert_eq!(t.step(at(110), 0, false), None);
        assert_eq!(t.step(at(120), 1, false), None);
        assert_eq!(t.step(at(140), 1, false), Some(true));
    }
}
