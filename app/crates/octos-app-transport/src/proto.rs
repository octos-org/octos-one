//! Transport-agnostic UI Protocol core shared by the WebSocket and stdio
//! transports. Everything here operates on JSON-RPC *text frames* and the
//! `OutboundCommand` / `TransportEvent` channel types — it never touches the
//! byte transport itself. The `ws` and `stdio` modules own the socket / pipe
//! and delegate command dispatch (`build_outbound`) and inbound handling
//! (`handle_inbound_text`) here so there is a single source of truth for the
//! wire contract regardless of how frames travel.

use std::collections::HashMap;

use octos_core::app_ui::AppUiBackendEvent as UiNotification;
use octos_core::ui_protocol::{
    methods, ApprovalRespondResult, DiffPreviewGetResult, RpcError, TaskOutputReadResult, UiCursor,
    UiRpcResult,
};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use octos_core::SessionKey;

use crate::capability::Capabilities;
use crate::cursor::{CursorPersist, CursorStore};
use crate::jsonrpc::{serialize_request, JsonRpcId, RpcEnvelope, RpcRegistry};
use crate::{ConnectionState, LifecycleResult, OutboundCommand, TransportEvent};

/// Bounded channel depth for both the outbound command queue and the inbound
/// event queue (shared by every transport).
pub const CHANNEL_BUFFER: usize = 64;

pub(crate) struct PendingRequest {
    pub(crate) method: &'static str,
    pub(crate) reply: PendingReply,
}

pub(crate) enum PendingReply {
    Lifecycle,
    Approval(oneshot::Sender<Result<ApprovalRespondResult, RpcError>>),
    DiffPreview(oneshot::Sender<Result<DiffPreviewGetResult, RpcError>>),
    TaskOutput(oneshot::Sender<Result<TaskOutputReadResult, RpcError>>),
    /// `session/list` — result re-emitted as `TransportEvent::SessionsListed`.
    SessionList,
    /// `session/hydrate` — result re-emitted as
    /// `TransportEvent::SessionHydrated` tagged with the session key.
    SessionHydrate { session_id: String },
}

/// Per-connection mutable state: the replay cursor, in-flight requests keyed
/// by JSON-RPC id, and the id registry.
pub(crate) struct SharedState {
    /// Per-session replay cursors (W08 multi-session). Keyed by `SessionKey`
    /// so concurrent live sessions never clobber each other's replay position.
    pub(crate) cursors: CursorStore,
    /// One-shot legacy seed: the single `TransportConfig.cursor` (usually None),
    /// applied to the first bracketed `session/open` that has no stored cursor.
    pub(crate) pending_initial: Option<UiCursor>,
    pub(crate) pending: HashMap<JsonRpcId, PendingRequest>,
    pub(crate) registry: std::sync::Arc<RpcRegistry>,
}

impl SharedState {
    pub(crate) fn new(
        cursor: Option<UiCursor>,
        persist: Option<std::sync::Arc<dyn CursorPersist>>,
    ) -> Self {
        Self {
            cursors: match persist {
                Some(p) => CursorStore::new_persisted(p),
                None => CursorStore::new(),
            },
            pending_initial: cursor,
            pending: HashMap::new(),
            registry: std::sync::Arc::new(RpcRegistry::new()),
        }
    }
}

/// Result of translating an `OutboundCommand` into a wire frame. The transport
/// sends `frame` its own way (WS text message / stdin line) and, on a
/// successful send, records `pending` under `id`.
pub(crate) enum Outbound {
    Send {
        id: JsonRpcId,
        frame: String,
        pending: Option<PendingRequest>,
    },
    /// Serialization failed — skip this command (already logged).
    Skip,
    /// `OutboundCommand::Disconnect` — the transport should drain and exit.
    Disconnect,
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// Translate an `OutboundCommand` into a serialized JSON-RPC request frame plus
/// the `PendingReply` to record when it is sent. Consumes a fresh id from the
/// registry. Transport-agnostic: the caller owns the actual write.
pub(crate) fn build_outbound(cmd: OutboundCommand, shared: &mut SharedState) -> Outbound {
    let id = shared.registry.next_id();
    let (method, body, pending): (&'static str, Value, Option<PendingReply>) = match cmd {
        OutboundCommand::OpenSession(mut params) => {
            // Resume bracket: replay from THIS session's last cursor (W08
            // multi-session), falling back to the one-shot legacy seed for the
            // very first open. Per-session so concurrent sessions never share a
            // cursor.
            if params.after.is_none() {
                params.after = shared
                    .cursors
                    .get(&params.session_id)
                    .cloned()
                    .or_else(|| shared.pending_initial.take());
            }
            (methods::SESSION_OPEN, to_value(&params), Some(PendingReply::Lifecycle))
        }
        OutboundCommand::OpenSessionFresh(params) => {
            // Open WITHOUT a replay bracket (`params.after` stays None). With
            // per-session cursors (W08) there is no shared cursor to reset —
            // every other session keeps its own replay position.
            (methods::SESSION_OPEN, to_value(&params), Some(PendingReply::Lifecycle))
        }
        OutboundCommand::StartTurn(p) => {
            (methods::TURN_START, to_value(&p), Some(PendingReply::Lifecycle))
        }
        OutboundCommand::InterruptTurn(p) => {
            (methods::TURN_INTERRUPT, to_value(&p), Some(PendingReply::Lifecycle))
        }
        OutboundCommand::SendApprovalResponse { params, reply } => {
            (methods::APPROVAL_RESPOND, to_value(&params), Some(PendingReply::Approval(reply)))
        }
        OutboundCommand::FetchDiffPreview { params, reply } => {
            (methods::DIFF_PREVIEW_GET, to_value(&params), Some(PendingReply::DiffPreview(reply)))
        }
        OutboundCommand::RequestTaskOutput { params, reply } => {
            (methods::TASK_OUTPUT_READ, to_value(&params), Some(PendingReply::TaskOutput(reply)))
        }
        OutboundCommand::ListSessions => (
            methods::SESSION_LIST,
            // `cwd: None` = legacy per-profile listing; the field is
            // skip_serializing_if so the wire shape stays the historical
            // empty object.
            to_value(&octos_core::ui_protocol::SessionListParams { cwd: None }),
            Some(PendingReply::SessionList),
        ),
        OutboundCommand::HydrateSession { session_id } => (
            methods::SESSION_HYDRATE,
            to_value(&octos_core::ui_protocol::SessionHydrateParams {
                session_id: octos_core::SessionKey(session_id.clone()),
                after: None,
                include: vec!["messages".to_owned()],
            }),
            Some(PendingReply::SessionHydrate { session_id }),
        ),
        OutboundCommand::Disconnect => return Outbound::Disconnect,
    };

    match serialize_request(&id, method, &body) {
        Ok(frame) => Outbound::Send {
            id,
            frame,
            pending: pending.map(|reply| PendingRequest { method, reply }),
        },
        Err(e) => {
            log::warn!("transport: serialize {method}: {e}");
            Outbound::Skip
        }
    }
}

/// `message/delta` is the only ephemeral notification per
/// `03-PROTOCOL-CONTRACT.md` § "Live streaming output".
pub(crate) fn is_ephemeral_method(method: &str) -> bool {
    method == methods::MESSAGE_DELTA
}

/// Try to send an event without blocking. Logs a warning if the receiver
/// can't keep up — backpressure protects the read loop from a slow UI.
pub(crate) fn try_emit(events: &mpsc::Sender<TransportEvent>, evt: TransportEvent) {
    match events.try_send(evt) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            log::warn!("transport: event channel full, dropping frame");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {}
    }
}

async fn emit_durable_notification(
    events: &mpsc::Sender<TransportEvent>,
    payload: UiNotification,
    cursor: Option<UiCursor>,
) {
    if events
        .send(TransportEvent::DurableNotification { payload, cursor })
        .await
        .is_err()
    {
        log::debug!("transport: event receiver closed while sending durable notification");
    }
}

/// Inbound text-frame dispatcher. Returns `Some(new_state)` if the frame
/// implies a `ConnectionState` transition the caller should announce.
pub(crate) async fn handle_inbound_text(
    text: &str,
    shared: &mut SharedState,
    events: &mpsc::Sender<TransportEvent>,
    state: &mut ConnectionState,
) -> Option<ConnectionState> {
    let env = match RpcEnvelope::parse(text) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("transport: bad json frame: {e}");
            return None;
        }
    };
    match env {
        RpcEnvelope::Notification(n) => {
            handle_notification(&n.method, n.params, shared, events).await;
            None
        }
        RpcEnvelope::Response(r) => match shared.pending.remove(&r.id) {
            Some(p) => handle_response(p, r.result, events, state),
            None => {
                log::warn!("transport: response for unknown id {}", r.id);
                None
            }
        },
        RpcEnvelope::ErrorResponse(er) => {
            if let Some(id) = er.id.clone() {
                if let Some(pending) = shared.pending.remove(&id) {
                    let method = pending.method.to_owned();
                    fail_pending(pending, er.error.clone());
                    try_emit(
                        events,
                        TransportEvent::RpcError {
                            request_id: id,
                            method,
                            error: er.error,
                        },
                    );
                }
            } else {
                log::warn!("transport: error response missing id: {:?}", er.error);
            }
            None
        }
        RpcEnvelope::Request(req) => {
            log::warn!("transport: server initiated request {} (ignored)", req.method);
            None
        }
    }
}

fn handle_response(
    pending: PendingRequest,
    result_value: Value,
    events: &mpsc::Sender<TransportEvent>,
    state: &mut ConnectionState,
) -> Option<ConnectionState> {
    let method = pending.method;
    match pending.reply {
        PendingReply::Lifecycle => {
            match UiRpcResult::from_method_and_result(method, result_value.clone()) {
                Ok(UiRpcResult::SessionOpen(open)) => {
                    let caps = Capabilities::parse(&result_value);
                    try_emit(events, TransportEvent::CapabilityNegotiated(caps));
                    try_emit(events, TransportEvent::RpcResult(LifecycleResult::SessionOpen(open)));
                    if !matches!(state, ConnectionState::Live) {
                        *state = ConnectionState::Live;
                        return Some(ConnectionState::Live);
                    }
                    None
                }
                Ok(UiRpcResult::TurnStart(r)) => {
                    try_emit(events, TransportEvent::RpcResult(LifecycleResult::TurnStart(r)));
                    None
                }
                Ok(UiRpcResult::TurnInterrupt(r)) => {
                    try_emit(events, TransportEvent::RpcResult(LifecycleResult::TurnInterrupt(r)));
                    None
                }
                Ok(other) => {
                    log::warn!("transport: lifecycle result unexpected variant: {:?}", other.kind());
                    None
                }
                Err(e) => {
                    log::warn!("transport: decode lifecycle result for {method}: {e:?}");
                    None
                }
            }
        }
        PendingReply::Approval(reply) => {
            let _ = reply.send(
                serde_json::from_value::<ApprovalRespondResult>(result_value)
                    .map_err(|e| RpcError::invalid_params(e.to_string())),
            );
            None
        }
        PendingReply::DiffPreview(reply) => {
            let _ = reply.send(
                serde_json::from_value::<DiffPreviewGetResult>(result_value)
                    .map_err(|e| RpcError::invalid_params(e.to_string())),
            );
            None
        }
        PendingReply::TaskOutput(reply) => {
            let _ = reply.send(
                serde_json::from_value::<TaskOutputReadResult>(result_value)
                    .map_err(|e| RpcError::invalid_params(e.to_string())),
            );
            None
        }
        PendingReply::SessionList => {
            match serde_json::from_value::<octos_core::ui_protocol::SessionListResult>(result_value)
            {
                Ok(r) => try_emit(events, TransportEvent::SessionsListed { sessions: r.sessions }),
                Err(e) => log::warn!("transport: decode session/list result: {e}"),
            }
            None
        }
        PendingReply::SessionHydrate { session_id } => {
            // Raw pass-through: the backend decodes `SessionHydrateResult`
            // (it owns the chat-store routing; keeps the transport thin).
            try_emit(events, TransportEvent::SessionHydrated { session_id, result: result_value });
            None
        }
    }
}

fn fail_pending(pending: PendingRequest, err: RpcError) {
    match pending.reply {
        PendingReply::Lifecycle => {} // surfaced as TransportEvent::RpcError
        PendingReply::Approval(reply) => {
            let _ = reply.send(Err(err));
        }
        PendingReply::DiffPreview(reply) => {
            let _ = reply.send(Err(err));
        }
        PendingReply::TaskOutput(reply) => {
            let _ = reply.send(Err(err));
        }
        // Sidebar hydrate is best-effort; the retry rides the next
        // `session/open` → `CapabilityNegotiated` → `ListSessions` cycle.
        PendingReply::SessionList => {}
        // History hydrate is best-effort too — the user can re-tap the
        // session row; the error already surfaced as a warn.
        PendingReply::SessionHydrate { session_id } => {
            log::warn!("transport: session/hydrate failed for {session_id}: {err:?}");
        }
    }
}

async fn handle_notification(
    method: &str,
    params: Value,
    shared: &mut SharedState,
    events: &mpsc::Sender<TransportEvent>,
) {
    let payload = match UiNotification::from_method_and_params(method, params.clone()) {
        Ok(p) => p,
        Err(_) => {
            // `server/heartbeat` is a periodic keepalive (~20s, empty params)
            // with no app-facing payload — ignore it quietly rather than
            // logging an "unknown notification" warning on every tick.
            if method != "server/heartbeat" {
                log::warn!("transport: unknown notification method '{method}'");
            }
            return;
        }
    };
    if is_ephemeral_method(method) {
        try_emit(events, TransportEvent::EphemeralNotification { payload });
        return;
    }
    let cursor = params
        .get("cursor")
        .and_then(|v| serde_json::from_value::<UiCursor>(v.clone()).ok());
    if let Some(c) = cursor.clone() {
        // W08: advance the cursor for THIS notification's session only, so
        // concurrent live sessions don't overwrite each other's replay position.
        if let Some(session) = params
            .get("session_id")
            .and_then(|v| serde_json::from_value::<SessionKey>(v.clone()).ok())
        {
            shared.cursors.set(session, c);
        }
    }
    emit_durable_notification(events, payload, cursor).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ephemeral_helper_only_message_delta() {
        assert!(is_ephemeral_method(methods::MESSAGE_DELTA));
        assert!(!is_ephemeral_method(methods::TOOL_STARTED));
    }
}
