//! `OctosUiAgent` — Makepad `Agent` implementation that bridges the UI to
//! `octos-app-transport`.
//!
//! Owns a Tokio runtime + the `(cmd_tx, evt_rx)` pair returned by
//! `octos_app_transport::ws::spawn`. UI calls translate to `OutboundCommand`s;
//! transport notifications drained on `handle_event` translate to `AgentEvent`s
//! the chat surface already understands.
//!
//! Crate boundary: `OctosUiAgent` is the *only* place inside `app/` that
//! talks to `octos-app-transport`. UI code goes through the `Agent` trait.

use std::collections::HashMap;

use makepad_ai::{Agent, AgentEvent, PromptId, SessionConfig, SessionId, StopReason};
use makepad_widgets::*;
use octos_app_store::state::{reduce as store_reduce, ConnectionEvent, Event as StoreEvent};
use octos_app_store::toasts::{Toast, ToastKind};
use octos_app_transport::{
    stdio, ws, Capabilities, ConnectionState, LifecycleResult, OutboundCommand, TransportConfig,
    TransportEvent,
};
use octos_core::app_ui::{
    AppUiBackendEvent as UiNotification, AppUiInputItem as InputItem,
    AppUiInterruptTurn as TurnInterruptParams, AppUiOpenSession as SessionOpenParams,
    AppUiSubmitPrompt as TurnStartParams,
};
use octos_core::ui_protocol::{
    ApprovalDecision, ApprovalId, ApprovalRespondParams, ReasoningEffortLevel, TaskOutputReadParams,
    UiCursor,
};
use octos_core::{ui_protocol::TurnId, SessionKey};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::app::sessions::APP_STATE;

/// Posted from the transport drain when a `session/hydrate` reply lands.
/// `App::handle_actions` folds it into `CHAT_DATA` if `session_id` still
/// matches the session the user resumed (guards against a stale reply after
/// the user has already switched again).
#[derive(Debug)]
pub struct SessionResumeHydrated {
    pub session_id: SessionId,
    /// `(role, content)` rows in seq order, roles as the wire sends them
    /// ("user" / "assistant" / other — the App filters).
    pub messages: Vec<(String, String)>,
}

/// `Agent` implementation backed by the Octos UI Protocol over WebSocket.
pub struct OctosUiAgent {
    /// Owned Tokio runtime — required because `ws::spawn` calls
    /// `tokio::spawn` internally and `app/` has no global runtime. Held for
    /// the agent's lifetime; the WS task lives inside it.
    _runtime: Runtime,
    /// Outbound side of the transport channel. Cloneable, lock-free
    /// `try_send` from the main thread.
    cmd_tx: Sender<OutboundCommand>,
    /// Inbound side; drained each tick on `handle_event`.
    evt_rx: Receiver<TransportEvent>,
    /// Makepad SessionId → octos-core SessionKey.
    session_keys: HashMap<SessionId, SessionKey>,
    /// octos-core SessionKey → Makepad SessionId (reverse lookup for
    /// notifications arriving from the wire).
    session_ids: HashMap<SessionKey, SessionId>,
    /// Sessions for which the server has answered `session/open`.
    ready_sessions: std::collections::HashSet<SessionId>,
    /// Makepad PromptId → octos-core TurnId.
    turn_ids: HashMap<PromptId, TurnId>,
    /// octos-core TurnId → Makepad PromptId (reverse lookup).
    prompt_ids: HashMap<TurnId, PromptId>,
    /// W08: Makepad PromptId → the SessionKey that owns it, so `cancel_prompt`
    /// (and future per-prompt ops) target the RIGHT session in a multi-session
    /// client instead of guessing the "first" one.
    prompt_sessions: HashMap<PromptId, SessionKey>,
    /// Most recent connection state — also mirrored into
    /// `APP_STATE.connection` (via `fold_connection_into_store`) for the
    /// top-bar status indicator and toast queue. Kept locally so we can
    /// detect transitions (Reconnecting → Live, Live → Failed, …) without
    /// re-reading the store under a write lock.
    connection_state: ConnectionState,
    /// Server-negotiated capability set. W05 reads this when deciding which
    /// approval / pane affordances to show. Stored as soon as
    /// `CapabilityNegotiated` arrives.
    #[allow(dead_code)]
    capabilities: Option<Capabilities>,
    /// Workspace cwd requested for every `session/open`, when configured.
    workspace_cwd: Option<String>,
    /// Profile id from the transport config — fallback owner for sidebar
    /// rows the server returns without a `profile_id` (session/list).
    fallback_profile: String,
    /// "Thinking" composer toggle state. When on, every turn requests a
    /// per-turn reasoning-effort override (thinking-capable models: DeepSeek
    /// V4, OpenAI reasoning models, Grok-4). Set by `Agent::set_thinking`;
    /// `None` (off) falls back to the gateway/profile default.
    thinking: bool,
    /// True when the stdio transport is in use (spawned `octos serve --stdio`)
    /// rather than WebSocket. Stdio carries no `X-Profile-Id` header, so
    /// `session/open` must name the profile in its params instead.
    stdio_transport: bool,
}

impl OctosUiAgent {
    /// Construct a new agent. Spawns the WebSocket task immediately so
    /// `create_session` can ship `session/open` on the first call.
    ///
    /// On a missing / invalid env (no bearer, unreachable URL) the transport
    /// task drops to `ConnectionState::Failed` after the budget expires; the
    /// agent stays usable, sends fail silently into a closed channel, and
    /// `is_session_ready` stays `false` forever — matching M1's "boots even
    /// without a server" requirement.
    pub fn new(config: TransportConfig) -> Self {
        let workspace_cwd = config.workspace_cwd.clone();
        let fallback_profile = config.profile_id.0.clone();
        let stdio_transport = config.stdio.is_some();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("octos-ui-agent: tokio runtime build");
        let (cmd_tx, evt_rx) = {
            let _guard = runtime.enter();
            // Wake the makepad UI thread whenever a transport event queues —
            // otherwise replies that land while the app is idle (no touch,
            // no animation) sit undrained until the next unrelated event.
            let waker = Some(std::sync::Arc::new(|| {
                makepad_widgets::SignalToUI::set_ui_signal();
            }) as std::sync::Arc<dyn Fn() + Send + Sync>);
            // stdio spawns `octos serve --stdio` as a child; ws dials a socket.
            if stdio_transport {
                stdio::spawn_with_waker(config, waker)
            } else {
                ws::spawn_with_waker(config, waker)
            }
        };
        Self {
            _runtime: runtime,
            cmd_tx,
            evt_rx,
            session_keys: HashMap::new(),
            session_ids: HashMap::new(),
            ready_sessions: std::collections::HashSet::new(),
            turn_ids: HashMap::new(),
            prompt_ids: HashMap::new(),
            prompt_sessions: HashMap::new(),
            connection_state: ConnectionState::Idle,
            capabilities: None,
            workspace_cwd,
            fallback_profile,
            thinking: false,
            stdio_transport,
        }
    }

    /// Synthesise a fresh `SessionKey` from a Makepad `SessionId`. The
    /// LiveId-as-u64 → hex string round-trip is stable for the agent's
    /// lifetime; the server treats the value as an opaque identifier.
    ///
    /// The key embeds a per-process boot nonce: `SessionId`s are sequential
    /// (1, 2, …) so without it every app launch would mint the same
    /// "octos-app:…0001" key and silently re-attach to (and grow) the
    /// previous launch's server session. Old sessions stay reachable via
    /// `session/list` + `resume_session`.
    fn make_session_key(&self, session_id: SessionId) -> SessionKey {
        static BOOT_NONCE: std::sync::LazyLock<u64> = std::sync::LazyLock::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        });
        // stdio: the kernel resolves the turn's ProfileRuntime from the
        // SESSION KEY's `{profile}:{channel}:{chat_id}` shape
        // (`SessionKey::profile_id()` — the `session/open` param alone does
        // not bind it; see octos `run_native_*_turn`'s
        // `session_id.profile_id().or(routed_profile_id)`). Without the
        // prefix every turn/start fails with "No ProfileRuntime registered
        // for profile '<unset>'". `api` is a registry channel name
        // (`is_channel_name`), which the profile-prefix parse requires;
        // the WS transport keeps the legacy opaque key (profile rides the
        // `X-Profile-Id` header / bearer instead).
        if self.stdio_transport {
            SessionKey(format!(
                "{}:api:{:08x}-{:08x}",
                self.fallback_profile,
                *BOOT_NONCE,
                session_id.0.0 as u32
            ))
        } else {
            SessionKey(format!(
                "octos-app:{:08x}-{:08x}",
                *BOOT_NONCE,
                session_id.0.0 as u32
            ))
        }
    }

    /// Profile to name in `session/open` params. The stdio transport carries
    /// no `X-Profile-Id` header, so the profile must ride in the params; the
    /// WebSocket transport uses the header and leaves this `None` (unchanged
    /// behaviour).
    fn open_profile_id(&self) -> Option<String> {
        self.stdio_transport.then(|| self.fallback_profile.clone())
    }

    /// Best-effort post to the transport task. Logs (and drops) on a closed
    /// or full channel — the caller can't usefully recover here.
    fn post(&self, cmd: OutboundCommand) {
        match self.cmd_tx.try_send(cmd) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                log::warn!("octos-ui-agent: command channel full; dropping");
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                log::warn!("octos-ui-agent: transport task gone; command dropped");
            }
        }
    }

    /// W05 — extract a cheap, send-safe handle for issuing
    /// `approval/respond` commands without cloning the whole agent. The
    /// handle wraps a `Sender<OutboundCommand>` plus a runtime handle, so
    /// `App::handle_actions` can fire `approval/respond` even though the
    /// agent itself is held behind `Box<dyn Agent>`.
    pub fn approval_handle(&self) -> ApprovalHandle {
        ApprovalHandle {
            cmd_tx: self.cmd_tx.clone(),
            runtime: self._runtime.handle().clone(),
        }
    }

    /// Cheap handle for one-shot `task/output/read` requests from the coding
    /// workspace. Mirrors `approval_handle` so UI code does not downcast the
    /// boxed `Agent`.
    pub fn task_output_handle(&self) -> TaskOutputHandle {
        TaskOutputHandle {
            cmd_tx: self.cmd_tx.clone(),
            runtime: self._runtime.handle().clone(),
        }
    }

    /// Translate one `TransportEvent` into zero-or-more `AgentEvent`s.
    /// Updates internal id maps and connection bookkeeping as a side effect.
    fn translate(&mut self, event: TransportEvent) -> Vec<AgentEvent> {
        match event {
            TransportEvent::ConnectionState(state) => {
                let prev = std::mem::replace(&mut self.connection_state, state.clone());
                self.fold_connection_into_store(&prev, &self.connection_state.clone());
                Vec::new()
            }
            TransportEvent::CapabilityNegotiated(caps) => {
                // W05 — mirror the negotiated capability flags into the
                // process-wide ApprovalsPane state. The widget reads
                // `APPROVAL_CAPS.read()` in `populate_card` to decide
                // whether to render typed sub-views and the scope dropdown.
                if let Ok(mut g) = crate::app::approvals::APPROVAL_CAPS.write() {
                    g.typed_approvals = caps.typed_approvals;
                    g.pane_snapshots = caps.pane_snapshots;
                }
                self.capabilities = Some(caps);
                // M12 D-5 — `GET /api/sessions` is retired; hydrate the
                // sidebar over the wire once the connection is live.
                self.post(OutboundCommand::ListSessions);
                Vec::new()
            }
            TransportEvent::RpcResult(LifecycleResult::SessionOpen(open)) => {
                let key = open.opened.session_id.clone();
                if let Some(sid) = self.session_ids.get(&key).copied() {
                    self.ready_sessions.insert(sid);
                    return vec![AgentEvent::SessionReady { session_id: sid }];
                }
                Vec::new()
            }
            TransportEvent::RpcResult(_) => Vec::new(),
            TransportEvent::RpcError {
                method, error, ..
            } => {
                let msg = format!("{method}: {} ({})", error.message, error.code);
                if method == "session/open" {
                    if let Some(&sid) = self.session_keys.keys().next() {
                        return vec![AgentEvent::SessionError {
                            session_id: sid,
                            error: msg,
                        }];
                    }
                }
                // W08 best-effort: an RPC error with no pending-request context
                // can't be attributed to a specific session/prompt, so it lands
                // on an arbitrary in-flight prompt. Precise routing would need
                // the transport to carry session/turn on `RpcError` (follow-up).
                if let Some(&pid) = self.prompt_ids.values().next() {
                    return vec![AgentEvent::PromptError {
                        prompt_id: pid,
                        error: msg,
                    }];
                }
                log::warn!("octos-ui-agent: rpc error with no handler: {msg}");
                Vec::new()
            }
            TransportEvent::DurableNotification { payload, cursor } => {
                self.fold_into_store(payload.clone(), cursor);
                self.translate_notification(payload)
            }
            TransportEvent::EphemeralNotification { payload } => {
                // Ephemerals (`message/delta`) carry no cursor — pass `None`
                // so `state::reduce` skips the cursor advance per
                // `octos-app-store/src/state.rs:148-152`.
                self.fold_into_store(payload.clone(), None);
                self.translate_notification(payload)
            }
            TransportEvent::SessionsListed { sessions } => {
                // Same downstream path as the old REST hydrate: project and
                // post `SessionListAction::Hydrated` for `handle_actions`.
                let fallback =
                    octos_app_store::auth::ProfileId::from(self.fallback_profile.clone());
                crate::app::sessions::hydrate_from_ws_value(sessions, &fallback);
                Vec::new()
            }
            TransportEvent::SessionHydrated { session_id, result } => {
                // Resume flow: decode the chat rows and hand them to the App
                // via a posted action (same pattern as `SessionListAction`).
                let key = SessionKey(session_id);
                let Some(&sid) = self.session_ids.get(&key) else {
                    log::warn!(
                        "octos-ui-agent: session/hydrate reply for unmapped {}",
                        key.0
                    );
                    return Vec::new();
                };
                match serde_json::from_value::<
                    octos_core::ui_protocol::SessionHydrateResult,
                >(result)
                {
                    Ok(r) => {
                        let mut messages: Vec<(String, String)> = Vec::new();
                        if let Some(rows) = r.messages {
                            for row in rows {
                                messages.push((row.role, row.content));
                            }
                        }
                        log::info!(
                            "octos-ui-agent: hydrated {} rows for {}",
                            messages.len(),
                            key.0
                        );
                        Cx::post_action(SessionResumeHydrated {
                            session_id: sid,
                            messages,
                        });
                    }
                    Err(e) => {
                        log::warn!("octos-ui-agent: decode session/hydrate: {e}")
                    }
                }
                Vec::new()
            }
        }
    }

    /// Fold a `tool/*` / `task/*` / `turn/*` notification into the global
    /// `APP_STATE`. Replaces the W04-todo "buffer for now" behaviour the
    /// previous translate path used. The store's `apply_protocol`
    /// (`octos-app-store/src/state.rs:148`) already handles every
    /// `UiNotification` variant; we just hand off here.
    ///
    /// Read-write lock contention is bounded — the lock is held only for
    /// the duration of `reduce`, which is a small in-memory mutation. If a
    /// reader (the `TaskDock` widget, the `SessionList` widget) is mid-draw,
    /// we wait for it. This mirrors the `APP_STATE.write()` pattern used by
    /// `App::handle_actions` at `app/src/main.rs:2587-2599` when applying
    /// optimistic session deletes.
    fn fold_into_store(&self, n: UiNotification, cursor: Option<UiCursor>) {
        let event = StoreEvent::Protocol { cursor, notification: n };
        match APP_STATE.write() {
            Ok(mut state) => store_reduce(&mut state, event),
            Err(e) => log::warn!("octos-ui-agent: APP_STATE poisoned: {e}"),
        }
    }

    /// W04 follow-up #3: mirror connection-state transitions into the store
    /// so the top bar can render a coloured dot (Live = green, Reconnecting
    /// = amber, Failed/Idle = red) and push a transient toast on edges.
    /// Toasts are deduped by transition, not state, so a flap that lands on
    /// the same state still emits one toast (e.g. Live → Reconnecting →
    /// Live shows both reconnecting + reconnect-success). `prev == next`
    /// is a no-op: the transport may resend the current state on internal
    /// re-entrancy.
    fn fold_connection_into_store(&self, prev: &ConnectionState, next: &ConnectionState) {
        if prev == next {
            return;
        }
        let store_event = match next {
            ConnectionState::Live | ConnectionState::ReplayApplying => {
                Some(ConnectionEvent::Connected)
            }
            ConnectionState::Reconnecting { .. } => Some(ConnectionEvent::Reconnecting),
            ConnectionState::Idle
            | ConnectionState::Dialing
            | ConnectionState::Handshaking
            | ConnectionState::Failed => Some(ConnectionEvent::Offline),
        };
        let toast = match (prev, next) {
            // Reconnect arc: prev was a degraded state, we're back to Live.
            (
                ConnectionState::Reconnecting { .. } | ConnectionState::ReplayApplying,
                ConnectionState::Live,
            ) => Some(Toast::new(ToastKind::ReconnectSuccess, "Reconnected")),
            // Falling into Reconnecting from anywhere — show backoff toast.
            (_, ConnectionState::Reconnecting { attempt }) => Some(Toast::new(
                ToastKind::Reconnecting,
                format!("Reconnecting (attempt {attempt})"),
            )),
            // Cumulative budget exhausted — terminal failure toast.
            (_, ConnectionState::Failed) => Some(Toast::new(
                ToastKind::Error,
                "Connection failed; restart to retry",
            )),
            // First Live (Dialing/Handshaking → Live) — confirm online.
            (ConnectionState::Dialing | ConnectionState::Handshaking, ConnectionState::Live) => {
                Some(Toast::new(ToastKind::ReconnectSuccess, "Connected"))
            }
            _ => None,
        };
        match APP_STATE.write() {
            Ok(mut state) => {
                if let Some(ev) = store_event {
                    store_reduce(&mut state, StoreEvent::Connection(ev));
                }
                if let Some(t) = toast {
                    store_reduce(&mut state, StoreEvent::Toast(t));
                }
            }
            Err(e) => log::warn!("octos-ui-agent: APP_STATE poisoned: {e}"),
        }
    }

    fn translate_notification(&mut self, n: UiNotification) -> Vec<AgentEvent> {
        match n {
            UiNotification::MessageDelta(ev) => self
                .prompt_ids
                .get(&ev.turn_id)
                .copied()
                .map(|pid| {
                    vec![AgentEvent::TextDelta {
                        prompt_id: pid,
                        text: ev.text,
                    }]
                })
                .unwrap_or_default(),
            // 2026-07 protocol catch-up: server-side reasoning stream maps
            // onto the chat surface's thinking strip.
            UiNotification::ReasoningDelta(ev) => self
                .prompt_ids
                .get(&ev.turn_id)
                .copied()
                .map(|pid| {
                    vec![AgentEvent::ThinkingDelta {
                        prompt_id: pid,
                        text: ev.text,
                    }]
                })
                .unwrap_or_default(),
            UiNotification::ToolStarted(ev) => self
                .prompt_ids
                .get(&ev.turn_id)
                .copied()
                .map(|pid| {
                    let input = ev
                        .arguments
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "{}".to_owned());
                    vec![AgentEvent::ToolRequest {
                        prompt_id: pid,
                        tool_use_id: ev.tool_call_id,
                        tool_name: ev.tool_name,
                        tool_input: input,
                    }]
                })
                .unwrap_or_default(),
            UiNotification::TurnCompleted(ev) => self
                .prompt_ids
                .remove(&ev.turn_id)
                .map(|pid| {
                    self.turn_ids.remove(&pid);
                    self.prompt_sessions.remove(&pid);
                    vec![AgentEvent::TurnComplete {
                        prompt_id: pid,
                        stop_reason: StopReason::EndTurn,
                    }]
                })
                .unwrap_or_default(),
            UiNotification::TurnError(ev) => self
                .prompt_ids
                .remove(&ev.turn_id)
                .map(|pid| {
                    self.turn_ids.remove(&pid);
                    self.prompt_sessions.remove(&pid);
                    vec![AgentEvent::PromptError {
                        prompt_id: pid,
                        error: format!("{}: {}", ev.code, ev.message),
                    }]
                })
                .unwrap_or_default(),
            // Drained into APP_STATE by `fold_into_store` above. The
            // TaskDock widget (`app/src/app/task_dock.rs`) reads them back
            // via `APP_STATE.tool_calls` / `APP_STATE.tasks` on each redraw,
            // so we don't bridge them through `AgentEvent` — there's no
            // round-trip required. ApprovalRequested lands on the
            // `ApprovalsSlice` (W05 surface). Warning toasts already land in
            // `state.toasts` via the store reducer.
            // Drained into APP_STATE via `fold_into_store`. Listed
            // explicitly so a new `UiNotification` variant tickles a compile
            // error here and forces a deliberate decision (forward-compat
            // per spec § 4.1).
            UiNotification::TurnStarted(_)
            | UiNotification::ToolProgress(_)
            | UiNotification::ToolCompleted(_)
            | UiNotification::TaskUpdated(_)
            | UiNotification::TaskOutputDelta(_)
            | UiNotification::ApprovalRequested(_)
            | UiNotification::ApprovalAutoResolved(_)
            | UiNotification::ApprovalDecided(_)
            | UiNotification::ApprovalCancelled(_)
            | UiNotification::ProgressUpdated(_)
            | UiNotification::ReplayLossy(_)
            | UiNotification::SessionOpened(_)
            | UiNotification::Warning(_)
            // 2026-07 protocol catch-up — folded into APP_STATE (or
            // deliberately unsurfaced) by the store reducer; nothing to
            // bridge through AgentEvent. Kept explicit per the note above.
            | UiNotification::UserQuestionRequested(_)
            | UiNotification::VisualGenerating(_)
            | UiNotification::VisualSucceeded(_)
            | UiNotification::VisualFailed(_)
            | UiNotification::VoiceExit(_)
            | UiNotification::MessagePersisted(_)
            | UiNotification::TurnSpawnComplete(_)
            | UiNotification::FileAttached(_)
            | UiNotification::SessionEventBridged(_)
            | UiNotification::RouterStatus(_)
            | UiNotification::RouterFailover(_)
            | UiNotification::QueueState(_)
            | UiNotification::AgentUpdated(_)
            | UiNotification::AgentOutputDelta(_)
            | UiNotification::AgentArtifactUpdated(_)
            | UiNotification::SessionGoalUpdated(_)
            | UiNotification::SessionGoalCleared(_)
            | UiNotification::LoopUpdated(_)
            | UiNotification::LoopFired(_)
            | UiNotification::LoopCompleted(_)
            | UiNotification::ContextCompactionCompleted(_)
            | UiNotification::ContextCompactionStarted(_)
            | UiNotification::ContextNormalizationReported(_)
            | UiNotification::SessionOrchestration(_)
            // 2026-07 protocol catch-up: no plan pane / voice surface here.
            | UiNotification::PlanUpdated(_)
            | UiNotification::VoiceAudioChunk(_)
            | UiNotification::Envelope(_) => Vec::new(),
        }
    }
}

impl Agent for OctosUiAgent {
    fn create_session(&mut self, _cx: &mut Cx, config: SessionConfig) -> SessionId {
        let session_id = SessionId::new();
        let key = self.make_session_key(session_id);
        log::info!("octos-ui-agent: create_session → session/open {}", key.0);
        self.session_keys.insert(session_id, key.clone());
        self.session_ids.insert(key.clone(), session_id);
        let profile_id = self.open_profile_id();
        self.post(OutboundCommand::OpenSession(SessionOpenParams {
            session_id: key,
            // 2026-07 protocol catch-up: server-side session topic label and
            // per-session sandbox override — both server-defaulted when None.
            topic: None,
            profile_id,
            // Per-session cwd override first (the AMA composer session hints
            // its workspace INTO the app-cards memory tree so its file tools
            // can author new app specs — `session.workspace_cwd.v1` is
            // default-enabled on the stdio transport), else the agent-wide
            // workspace.
            cwd: config.cwd.or_else(|| self.workspace_cwd.clone()),
            sandbox: None,
            after: None,
        }));
        session_id
    }

    /// Re-attach to an existing server session (sidebar resume): map a fresh
    /// local `SessionId` to the given key, re-open it (octos sessions are
    /// stateful — `session/open` on an existing key attaches), then request
    /// its chat history via `session/hydrate`. The history lands as a posted
    /// `SessionResumeHydrated` action.
    fn resume_session(&mut self, _cx: &mut Cx, backend_key: &str) -> Option<SessionId> {
        let key = SessionKey(backend_key.to_owned());
        // Re-use the existing mapping if the user re-taps the same session.
        let session_id = if let Some(&sid) = self.session_ids.get(&key) {
            sid
        } else {
            let sid = SessionId::new();
            self.session_keys.insert(sid, key.clone());
            self.session_ids.insert(key.clone(), sid);
            sid
        };
        log::info!("octos-ui-agent: resume_session → {}", key.0);
        let profile_id = self.open_profile_id();
        // Fresh open (no cursor bracket): the connection cursor belongs to
        // the session we're switching AWAY from.
        self.post(OutboundCommand::OpenSessionFresh(SessionOpenParams {
            session_id: key.clone(),
            topic: None,
            profile_id,
            cwd: self.workspace_cwd.clone(),
            sandbox: None,
            after: None,
        }));
        self.post(OutboundCommand::HydrateSession { session_id: key.0 });
        Some(session_id)
    }

    /// The octos `SessionKey` string mapped to `session_id`. Layer 3: the
    /// multi-app switcher created these sessions via `create_session` (which
    /// returns only the local `SessionId`); this hands back the backend key so
    /// the switcher can `resume_session` → hydrate on foreground switch.
    fn backend_key(&self, session_id: SessionId) -> Option<String> {
        self.session_keys.get(&session_id).map(|k| k.0.clone())
    }

    fn send_prompt(&mut self, _cx: &mut Cx, session_id: SessionId, text: &str) -> PromptId {
        let prompt_id = PromptId::new();
        let turn_id = TurnId::new();
        self.turn_ids.insert(prompt_id, turn_id.clone());
        self.prompt_ids.insert(turn_id.clone(), prompt_id);
        let Some(key) = self.session_keys.get(&session_id).cloned() else {
            log::warn!("octos-ui-agent: send_prompt for unknown session");
            return prompt_id;
        };
        // W08: remember which session owns this prompt, so cancel/routing can
        // target it without guessing.
        self.prompt_sessions.insert(prompt_id, key.clone());
        self.post(OutboundCommand::StartTurn(TurnStartParams {
            session_id: key,
            turn_id,
            input: vec![InputItem::Text {
                text: text.to_owned(),
            }],
            // 2026-07 protocol catch-up: attachments, topic routing, prompt
            // rewrite, per-turn reasoning effort, and live-video capture are
            // all opt-in; text-only turns send the neutral defaults.
            media: Vec::new(),
            topic: None,
            rewrite_for: None,
            // Driven by the composer's "Thinking" toggle (`set_thinking`).
            // `High` when on; `None` defers to the gateway/profile default.
            reasoning_effort: self.thinking.then_some(ReasoningEffortLevel::High),
            live_video: false,
        }));
        prompt_id
    }

    fn set_thinking(&mut self, on: bool) {
        self.thinking = on;
    }

    fn send_tool_result(
        &mut self,
        _cx: &mut Cx,
        _session_id: SessionId,
        _tool_use_id: &str,
        _result: &str,
        _is_error: bool,
    ) {
        log::warn!("octos-ui-agent: ignoring tool result; AppUI has no contract command for it");
    }

    fn cancel_prompt(&mut self, _cx: &mut Cx, prompt_id: PromptId) {
        let Some(turn_id) = self.turn_ids.get(&prompt_id).cloned() else {
            return;
        };
        // W08: cancel the turn on the SESSION that owns this prompt (recorded
        // in `send_prompt`) — not a guessed "first" session.
        let Some(key) = self.prompt_sessions.get(&prompt_id).cloned() else {
            return;
        };
        self.post(OutboundCommand::InterruptTurn(TurnInterruptParams {
            session_id: key,
            turn_id,
        }));
    }

    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event) -> Vec<AgentEvent> {
        let mut out = Vec::new();
        loop {
            match self.evt_rx.try_recv() {
                Ok(evt) => out.extend(self.translate(evt)),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    log::warn!("octos-ui-agent: transport channel disconnected");
                    break;
                }
            }
        }
        out
    }

    fn is_session_ready(&self, session_id: SessionId) -> bool {
        self.ready_sessions.contains(&session_id)
    }

    fn is_stateless(&self) -> bool {
        // Octos sessions are stateful server-side.
        false
    }
}

/// Handle exposed by `OctosUiAgent::task_output_handle` for one-shot
/// `task/output/read` RPCs from the coding task drill-down.
#[derive(Clone)]
pub struct TaskOutputHandle {
    cmd_tx: Sender<OutboundCommand>,
    runtime: tokio::runtime::Handle,
}

impl TaskOutputHandle {
    pub fn read(&self, params: TaskOutputReadParams) {
        use tokio::sync::oneshot;

        let task_id = params.task_id.clone();
        let session_id = params.session_id.clone();
        let (tx, rx) = oneshot::channel();
        let cmd = OutboundCommand::RequestTaskOutput { params, reply: tx };
        match self.cmd_tx.try_send(cmd) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
            | Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                Cx::post_action(crate::app::coding::TaskOutputAction {
                    task_id,
                    session_id,
                    outcome: crate::app::coding::TaskOutputOutcome::Failed(
                        "transport unavailable".to_owned(),
                    ),
                });
                return;
            }
        }
        self.runtime.spawn(async move {
            let outcome = match rx.await {
                Ok(Ok(result)) => crate::app::coding::TaskOutputOutcome::Loaded(result),
                Ok(Err(err)) => crate::app::coding::TaskOutputOutcome::Failed(format!(
                    "{} ({})",
                    err.message, err.code
                )),
                Err(_) => crate::app::coding::TaskOutputOutcome::Failed(
                    "transport dropped the reply channel".to_owned(),
                ),
            };
            Cx::post_action(crate::app::coding::TaskOutputAction {
                task_id,
                session_id,
                outcome,
            });
        });
    }
}

/// W05 — handle exposed by `OctosUiAgent::approval_handle`. Carries a
/// cheap clone of the transport sender plus a runtime handle so the
/// approvals widget can post `approval/respond` and forward the wire
/// reply back to the UI thread without holding the agent itself. Cloning
/// is `Arc`-shaped under the hood (mpsc + tokio runtime handles).
#[derive(Clone)]
pub struct ApprovalHandle {
    cmd_tx: Sender<OutboundCommand>,
    runtime: tokio::runtime::Handle,
}

impl ApprovalHandle {
    /// Issue `approval/respond` and forward the wire reply to the UI
    /// thread as an `ApprovalAsyncAction`. Idempotent — the server
    /// enforces single-decision semantics; we just surface the outcome.
    /// See `workstreams/W05-approvals-diff.md` § "Approval response flow".
    pub fn respond(
        &self,
        session_id: SessionKey,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
        scope: Option<String>,
    ) {
        use tokio::sync::oneshot;
        // `ApprovalDecision` is no longer `Copy` (FIX-01); clone it once for
        // the wire params and keep the original for the failure branch +
        // async reply.
        let mut params =
            ApprovalRespondParams::new(session_id, approval_id.clone(), decision.clone());
        params.approval_scope = scope;
        let (tx, rx) = oneshot::channel();
        let cmd = OutboundCommand::SendApprovalResponse { params, reply: tx };
        match self.cmd_tx.try_send(cmd) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
            | Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                Cx::post_action(crate::app::approvals::ApprovalAsyncAction {
                    approval_id,
                    decision,
                    outcome: crate::app::approvals::ApprovalAsyncOutcome::Failed {
                        message: "transport unavailable".to_owned(),
                        code: 0,
                        data: None,
                    },
                });
                return;
            }
        }
        let approval_id_for_task = approval_id.clone();
        self.runtime.spawn(async move {
            let outcome = match rx.await {
                Ok(Ok(res)) => crate::app::approvals::ApprovalAsyncOutcome::Accepted {
                    runtime_resumed: res.runtime_resumed,
                },
                // Forward the structured RpcError so the UI can detect
                // `-32011 APPROVAL_NOT_PENDING` and recover the decision
                // from `data.recorded_decision`. See
                // `octos-cli/src/api/ui_protocol_approvals.rs:198-215`.
                Ok(Err(err)) => crate::app::approvals::ApprovalAsyncOutcome::Failed {
                    message: err.message.clone(),
                    code: err.code,
                    data: err.data.clone(),
                },
                Err(_) => crate::app::approvals::ApprovalAsyncOutcome::Failed {
                    message: "transport dropped the reply channel".to_owned(),
                    code: 0,
                    data: None,
                },
            };
            Cx::post_action(crate::app::approvals::ApprovalAsyncAction {
                approval_id: approval_id_for_task,
                decision,
                outcome,
            });
        });
    }
}
