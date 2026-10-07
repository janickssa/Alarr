//! Live events, broadcast to SSE clients.

use serde::Serialize;
use tokio::sync::broadcast;
use utoipa::ToSchema;

use crate::db::ActivityEntry;
use crate::modules::{Health, ServiceSnapshot};

/// An event on `GET /api/v1/events`. The SSE event name is the `event` field;
/// the SSE data is the JSON of `data`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum Event {
    /// A service's cached snapshot changed (sent after every poll).
    Snapshot(ServiceSnapshot),
    /// A service's health changed.
    Health(HealthChange),
    /// A user action started or finished (mirrors the activity log).
    Action(ActionEvent),
    /// An alert fired (alerts engine arrives in phase 4).
    Alert(AlertEvent),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HealthChange {
    pub service_id: String,
    pub previous: Option<Health>,
    pub health: Health,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActionEvent {
    /// Service the action belongs to, if any (used for permission filtering).
    pub service_id: Option<String>,
    pub entry: ActivityEntry,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AlertEvent {
    pub service_id: Option<String>,
    pub title: String,
    pub message: String,
}

impl Event {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Snapshot(_) => "snapshot",
            Self::Health(_) => "health",
            Self::Action(_) => "action",
            Self::Alert(_) => "alert",
        }
    }

    /// The service this event is about; `None` = visible to every signed-in user.
    pub fn service_id(&self) -> Option<&str> {
        match self {
            Self::Snapshot(s) => Some(&s.service_id),
            Self::Health(h) => Some(&h.service_id),
            Self::Action(a) => a.service_id.as_deref(),
            Self::Alert(a) => a.service_id.as_deref(),
        }
    }

    /// JSON of the payload only.
    pub fn data_json(&self) -> String {
        let value = match self {
            Self::Snapshot(s) => serde_json::to_string(s),
            Self::Health(h) => serde_json::to_string(h),
            Self::Action(a) => serde_json::to_string(a),
            Self::Alert(a) => serde_json::to_string(a),
        };
        value.unwrap_or_else(|_| "{}".into())
    }
}

#[derive(Clone)]
pub struct EventHub {
    tx: broadcast::Sender<Event>,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHub {
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(512).0,
        }
    }

    pub fn send(&self, event: Event) {
        // No receivers is fine.
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}
