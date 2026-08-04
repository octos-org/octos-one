//! Live session list pane — sidebar widget + REST-hydrate plumbing.
//!
//! Implements the M1 subset of `workstreams/W04-sessions-tasks-files.md` § 4
//! ("Sessions sub-surface"): a `PortalList`-backed sidebar pane that mirrors
//! `octos_app_store::sessions::SessionMap`, click-to-select, hover-to-delete,
//! and a streaming dot per `is_session_active`. Cold hydrate flows through
//! `RestClient::list_sessions` (`octos-app-transport::rest::mod.rs:165`) on
//! `App::handle_startup`.
//!
//! The widget owns no state of its own; it reads a process-wide
//! `LazyLock<RwLock<AppState>>` so `draw_walk` doesn't need a `Scope` plumb.
//! This mirrors `aichat/examples/aichat/src/main.rs:1144` (`pub static
//! CHAT_DATA`) and follows `aichat:1774-1881`'s `ChatList` widget pattern.
//! Mutations always go through `MatchEvent::handle_actions` on `App` (see
//! `main.rs:1920`); the widget itself is read-only.
//!
//! Cross-thread plumbing: REST results land back on the UI thread via
//! `Cx::post_action`, the same channel `aichat/old/widgets/src/image_cache.rs:471`
//! uses for off-thread image decode. Each network call spawns a tokio task on
//! the workspace runtime (`app/Cargo.toml` enables `rt-multi-thread`).

use std::sync::{LazyLock, RwLock};

use chrono::{DateTime, Utc};
use makepad_widgets::*;
use octos_app_store::auth::ProfileId as StoreProfileId;
use octos_app_store::sessions::Session;
use octos_app_store::state::AppState;
use octos_app_transport::rest::{RestClient, SessionListItem};
use octos_core::SessionKey;

/// Process-wide app-state holder. The widget reads it during `draw_walk`,
/// `App::handle_actions` writes to it, and the REST hydrate / delete tasks
/// post actions that `App` then folds in. Mirrors `aichat/examples/aichat/src/main.rs:1144`.
///
/// `AppState::default()` carries `HashMap`s + `Vec`s, so we wrap in
/// `LazyLock` (stable since 1.80; we're on 1.95). Same shape as aichat's
/// `pub static CHAT_DATA: RwLock<...>`, just with the runtime-init wrapper.
pub static APP_STATE: LazyLock<RwLock<AppState>> =
    LazyLock::new(|| RwLock::new(AppState::default()));

/// Cross-thread action posted by `hydrate_sessions` once the REST round-trip
/// completes. `App::handle_actions` folds it into `APP_STATE`. Carries a
/// `RestError` text on failure so the toast queue can surface it.
#[derive(Debug)]
pub enum SessionListAction {
    /// `RestClient::list_sessions` succeeded. The `Vec<Session>` is already
    /// projected from the wire shape via `project_item`.
    Hydrated(Vec<Session>),
    /// `RestClient::list_sessions` or `delete_session` failed. The string is
    /// the rendered `RestError`.
    Failed(String),
    /// User clicked a row. `App::handle_actions` swaps `current_session` so
    /// the W03 chat thread re-mounts.
    Selected(SessionKey),
    /// User clicked the row's `x` button. `App::handle_actions` issues the
    /// REST `DELETE` and applies the optimistic remove.
    DeleteRequested(SessionKey),
    /// `RestClient::delete_session` succeeded. Optimistic remove already
    /// applied; this is just a confirmation hook for future toast plumbing.
    Deleted(SessionKey),
}

/// Project a wire `SessionListItem` (transport rest module) to the store's
/// `Session`. Title falls back to a short `session_id` slice when the server
/// omits it (open question 1 in `W04-sessions-tasks-files.md` § 14).
fn project_item(item: SessionListItem, fallback_profile: &StoreProfileId) -> Session {
    let id = item.session_id;
    let title = item
        .title
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| short_id(&id));
    let updated_at: DateTime<Utc> = item.last_message_at.unwrap_or_else(Utc::now);
    let profile_id = item
        .profile_id
        .map(StoreProfileId::from)
        .unwrap_or_else(|| fallback_profile.clone());
    let mut s = Session::new(id, profile_id, title, updated_at);
    s.updated_at = updated_at;
    s
}

fn short_id(id: &SessionKey) -> String {
    let s = &id.0;
    if s.len() <= 8 {
        format!("Session {s}")
    } else {
        format!("Session {}…", &s[..8])
    }
}

/// Project the raw `session/list` JSON rows (M12 D-5 — the WS replacement
/// for the retired `GET /api/sessions`) and post the same
/// `SessionListAction::Hydrated` the REST path used. No network here — just
/// serde + projection, safe to call from the agent's event drain.
pub fn hydrate_from_ws_value(sessions: serde_json::Value, fallback_profile: &StoreProfileId) {
    match serde_json::from_value::<Vec<SessionListItem>>(sessions) {
        Ok(items) => {
            let sessions: Vec<Session> = items
                .into_iter()
                .map(|i| project_item(i, fallback_profile))
                .collect();
            Cx::post_action(SessionListAction::Hydrated(sessions));
        }
        Err(e) => {
            Cx::post_action(SessionListAction::Failed(format!("session/list decode: {e}")));
        }
    }
}

/// Spawn a thread that runs a small tokio runtime, calls
/// `RestClient::list_sessions`, and posts a `SessionListAction` back to the
/// UI thread. Off-thread by design — the REST call may take the full 500 ms
/// p95 noted in W04 § 12 and we don't want to stall `handle_startup`.
///
/// We use `std::thread::spawn` + a per-call `current_thread` runtime instead
/// of `tokio::spawn` so the call site doesn't need to already be inside a
/// runtime context. The transport's WS task owns its own runtime; this is a
/// short-lived REST helper that doesn't share it.
pub fn hydrate_sessions(client: RestClient, fallback_profile: StoreProfileId) {
    let _ = std::thread::Builder::new()
        .name("octos-sessions-hydrate".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    Cx::post_action(SessionListAction::Failed(format!(
                        "spawn tokio runtime: {e}"
                    )));
                    return;
                }
            };
            let res = rt.block_on(async { client.list_sessions().await });
            match res {
                Ok(items) => {
                    let sessions: Vec<Session> = items
                        .into_iter()
                        .map(|i| project_item(i, &fallback_profile))
                        .collect();
                    Cx::post_action(SessionListAction::Hydrated(sessions));
                }
                Err(e) => {
                    Cx::post_action(SessionListAction::Failed(format!("{e}")));
                }
            }
        });
}

/// Spawn a thread that issues `DELETE /api/sessions/{id}`. Mirrors
/// `hydrate_sessions` lifecycle. The optimistic remove already happened in
/// `App::handle_actions`; on failure we re-hydrate (W04 § 4).
pub fn delete_session_remote(
    client: RestClient,
    id: SessionKey,
    fallback_profile: StoreProfileId,
) {
    let _ = std::thread::Builder::new()
        .name("octos-sessions-delete".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    Cx::post_action(SessionListAction::Failed(format!(
                        "spawn tokio runtime: {e}"
                    )));
                    return;
                }
            };
            let id_for_call = id.clone();
            // Clone the client so the async block can move its copy while
            // the recovery path keeps the original for re-hydration. Cheap
            // — `reqwest::Client` is `Arc`-shaped internally.
            let client_for_async = client.clone();
            let res = rt.block_on(async move {
                client_for_async.delete_session(&id_for_call).await
            });
            match res {
                Ok(()) => Cx::post_action(SessionListAction::Deleted(id)),
                Err(e) => {
                    // Re-hydrate so the optimistic remove rolls back.
                    Cx::post_action(SessionListAction::Failed(format!("{e}")));
                    hydrate_sessions(client, fallback_profile);
                }
            }
        });
}

/// `PortalList`-wrapping widget. Reads `APP_STATE.sessions.sessions_for_sidebar()`
/// during `draw_walk`. Pattern lifted from
/// `aichat/examples/aichat/src/main.rs:1774-1881`.
#[derive(Script, ScriptHook, Widget)]
pub struct SessionList {
    #[deref]
    view: View,
}

impl Widget for SessionList {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Snapshot under the read lock so `draw_walk`'s inner loop doesn't
        // hold it across `set_text` / `item` calls (those allocate; cheap
        // enough to clone strings out).
        let rows: Vec<RowSnapshot> = {
            let state = match APP_STATE.read() {
                Ok(g) => g,
                Err(_) => return DrawStep::done(),
            };
            let current = state.current_session.clone();
            state
                .sessions
                .sessions_for_sidebar()
                .iter()
                .map(|s| RowSnapshot::new(s, current.as_ref() == Some(&s.id)))
                .collect()
        };

        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, rows.len());
                while let Some(item_id) = list.next_visible_item(cx) {
                    let Some(row) = rows.get(item_id) else { continue };
                    let item_widget = list.item(cx, item_id, id!(SessionItem));

                    // The row's whole click target is `row_click` (a Button),
                    // and Buttons render only their OWN text — child Labels
                    // nested inside one are never drawn (Button::draw_walk
                    // paints bg/icon/text and stops). So the title IS the
                    // button's text; the preview line was dropped with it.
                    item_widget
                        .button(cx, ids!(row_click))
                        .set_text(cx, &row.title);

                    let dot = item_widget.label(cx, ids!(streaming_dot));
                    dot.set_visible(cx, row.is_active);

                    item_widget
                        .label(cx, ids!(selected_marker))
                        .set_visible(cx, row.is_selected);

                    item_widget.draw_all_unscoped(cx);
                }
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        if let Event::Actions(actions) = event {
            let list = self.view.portal_list(cx, ids!(list));
            if !list.any_items_with_actions(actions) {
                return;
            }
            // Map row index -> SessionKey via the same selector used in draw,
            // so we don't need to round-trip through scope or carry an id on
            // the row widget itself.
            let ordered: Vec<SessionKey> = {
                let state = match APP_STATE.read() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                state
                    .sessions
                    .sessions_for_sidebar()
                    .iter()
                    .map(|s| s.id.clone())
                    .collect()
            };
            for (item_id, item) in list.items_with_actions(actions) {
                let Some(id) = ordered.get(item_id).cloned() else { continue };
                if item.button(cx, ids!(delete_button)).clicked(actions) {
                    Cx::post_action(SessionListAction::DeleteRequested(id));
                    continue;
                }
                if item.button(cx, ids!(row_click)).clicked(actions) {
                    Cx::post_action(SessionListAction::Selected(id));
                }
            }
        }
    }
}

/// Per-row data captured under the `APP_STATE` read lock so the lock is
/// released before `set_text` / `draw` calls land.
struct RowSnapshot {
    title: String,
    /// Kept for the session-row preview line — dropped with the row's
    /// nested Labels (see the `row_click` note in `draw_walk`); may
    /// return when the row template regains a second line.
    #[allow(dead_code)]
    preview: Option<String>,
    is_active: bool,
    is_selected: bool,
}

impl RowSnapshot {
    fn new(s: &Session, is_selected: bool) -> Self {
        Self {
            title: s.title.clone(),
            preview: s.last_message_preview.clone(),
            is_active: s.is_streaming || s.has_active_task,
            is_selected,
        }
    }
}
