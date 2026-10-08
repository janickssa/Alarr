//! Server-Sent Events: live snapshots, health changes, actions and alerts.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures_util::{StreamExt, stream};
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::auth::CurrentUser;
use crate::db::Role;
use crate::events::Event;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(events))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct EventsQuery {
    /// Comma-separated service ids the client is showing; they are polled in fast mode
    /// while the stream is open.
    pub watch: Option<String>,
}

/// Aborts the fast-mode refresher when the client disconnects.
struct AbortOnDrop(tokio::task::AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn to_sse(event: &Event) -> SseEvent {
    SseEvent::default()
        .event(event.name())
        .data(event.data_json())
}

/// Live updates. Sends the current snapshot of every visible service on connect, then
/// `snapshot`, `health`, `action` and `alert` events (see the `Event` schema), filtered by
/// what the user may see.
#[utoipa::path(get, path = "/events", tag = "events",
    params(EventsQuery),
    responses((status = 200, content_type = "text/event-stream", description = "SSE stream of `Event`s")))]
async fn events(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<EventsQuery>,
) -> impl axum::response::IntoResponse {
    let watch: Vec<String> = q
        .watch
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty() && user.can_see(id))
        .map(str::to_owned)
        .collect();
    let refresher = {
        let state = state.clone();
        tokio::spawn(async move {
            if watch.is_empty() {
                return;
            }
            loop {
                for id in &watch {
                    state.supervisor.watch(id);
                }
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
        })
    };
    let guard = AbortOnDrop(refresher.abort_handle());

    let rx = state.events.subscribe();
    let initial: Vec<Event> = state
        .cache
        .all()
        .into_iter()
        .filter(|s| user.can_see(&s.service_id))
        .map(Event::Snapshot)
        .collect();
    let live = BroadcastStream::new(rx).filter_map(move |item| {
        let visible = match &item {
            // Guests only see their own actions, never other users' logins and changes.
            Ok(Event::Action(a)) if user.user.role == Role::Guest => {
                a.entry.user_id == Some(user.user.id)
                    && a.service_id.as_deref().is_none_or(|id| user.can_see(id))
            }
            Ok(e) => e.service_id().is_none_or(|id| user.can_see(id)),
            Err(_) => false,
        };
        let out = if visible {
            item.ok().map(|e| to_sse(&e))
        } else {
            None
        };
        async move { out }
    });
    let stream = stream::iter(initial.iter().map(to_sse).collect::<Vec<_>>())
        .chain(live)
        .map(move |e| {
            let _keep = &guard;
            Ok::<_, Infallible>(e)
        });
    // nginx (e.g. Nginx Proxy Manager) buffers proxied responses unless told not to, which
    // would hold events back until the buffer fills.
    (
        [("x-accel-buffering", "no")],
        Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))),
    )
}
