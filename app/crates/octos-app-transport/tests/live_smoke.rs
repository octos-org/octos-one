//! Live integration smoke test against a running Octos UI Protocol server.
//!
//! Marked `#[ignore]` so it never runs in default `cargo test`. Drive only
//! through `scripts/smoke-live.sh` (or the same command spelled by hand)
//! after exporting `OCTOS_LIVE_TOKEN`. The test is skipped (with an
//! `eprintln!`) when the token env var is missing — useful for CI.
//!
//! Flow exercised:
//!   1. Spawn the WS transport task with the live `TransportConfig`.
//!   2. Send `OutboundCommand::OpenSession` and wait for
//!      `ConnectionState::Live` + `LifecycleResult::SessionOpen`.
//!   3. Send `OutboundCommand::StartTurn` with a tiny prompt.
//!   4. Drain the event channel up to 30 s, collecting `MessageDelta` text
//!      and stopping on `turn/completed` (success) or `turn/error`/`RpcError`
//!      (failure).
//!   5. Cleanly disconnect via `OutboundCommand::Disconnect`.
//!
//! Wire reference: `03-PROTOCOL-CONTRACT.md`. Public-API surface used:
//! `octos_app_transport::ws::spawn`, `OutboundCommand`, `TransportEvent`,
//! `ConnectionState`, `TransportConfig`. We do NOT touch anything beyond
//! the published API.

use std::time::{Duration, Instant};

use octos_app_transport::{
    Capabilities, ConnectionState, LifecycleResult, OutboundCommand, ProfileId, SecretString,
    TransportConfig, TransportEvent, ws,
};
use octos_core::SessionKey;
use octos_core::ui_protocol::{
    InputItem, SessionOpenParams, TurnId, TurnStartParams, UiNotification, methods,
};
use url::Url;

const SESSION_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const TURN_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn live_smoke_session_open_and_turn() {
    let Some(token) = std::env::var("OCTOS_LIVE_TOKEN").ok() else {
        eprintln!("OCTOS_LIVE_TOKEN not set; skipping live smoke test");
        return;
    };
    let base_url = std::env::var("OCTOS_LIVE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:56831".to_owned());
    let profile = std::env::var("OCTOS_LIVE_PROFILE").unwrap_or_else(|_| "admin".to_owned());

    let cfg = TransportConfig {
        base_url: Url::parse(&base_url).expect("base_url parses"),
        bearer: SecretString::new(token),
        profile_id: ProfileId::new(&profile),
        cursor: None,
        cursor_file: None,
        requested_capabilities: Capabilities::requested(),
        workspace_cwd: std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().into_owned()),
        stdio: None,
    };

    eprintln!("smoke: dialing {base_url} as profile={profile}");
    let (cmd_tx, mut events) = ws::spawn(cfg);

    // Fresh session; channel/chat naming is arbitrary at this layer.
    let session_id = SessionKey::new("smoke", &uuid_short());
    let open = SessionOpenParams {
        session_id: session_id.clone(),
        topic: None,
        profile_id: Some(profile.clone()),
        cwd: None,
        sandbox: None,
        after: None,
    };
    cmd_tx
        .send(OutboundCommand::OpenSession(open))
        .await
        .expect("send OpenSession");

    let phase_start = Instant::now();
    let mut got_live = false;
    let mut got_open_result = false;
    while phase_start.elapsed() < SESSION_OPEN_TIMEOUT {
        let remaining = SESSION_OPEN_TIMEOUT.saturating_sub(phase_start.elapsed());
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Some(TransportEvent::ConnectionState(ConnectionState::Live))) => {
                got_live = true;
                eprintln!("smoke: connection Live (+{:?})", phase_start.elapsed());
            }
            Ok(Some(TransportEvent::ConnectionState(s))) => {
                eprintln!("smoke: connection state {s:?}");
            }
            Ok(Some(TransportEvent::RpcResult(LifecycleResult::SessionOpen(r)))) => {
                got_open_result = true;
                eprintln!(
                    "smoke: session/open result session_id={:?} profile={:?}",
                    r.opened.session_id, r.opened.active_profile_id
                );
            }
            Ok(Some(TransportEvent::CapabilityNegotiated(c))) => {
                eprintln!("smoke: caps negotiated typed_approvals={} pane_snapshots={}",
                    c.typed_approvals, c.pane_snapshots);
            }
            Ok(Some(TransportEvent::RpcError { method, error, .. })) => {
                panic!("smoke: rpc error during open ({method}): {} {}", error.code, error.message);
            }
            Ok(Some(other)) => eprintln!("smoke: pre-live event {other:?}"),
            Ok(None) => panic!("smoke: event channel closed during open"),
            Err(_) => break,
        }
        if got_live && got_open_result {
            break;
        }
    }
    assert!(got_live, "expected ConnectionState::Live within {SESSION_OPEN_TIMEOUT:?}");
    assert!(got_open_result, "expected LifecycleResult::SessionOpen within {SESSION_OPEN_TIMEOUT:?}");

    let turn_id = TurnId::new();
    let prompt = "Reply with the single word 'pong'.";
    eprintln!("smoke: starting turn {} prompt={prompt:?}", turn_id.0);
    cmd_tx
        .send(OutboundCommand::StartTurn(TurnStartParams {
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            input: vec![InputItem::Text { text: prompt.to_owned() }],
            media: Vec::new(),
            topic: None,
            rewrite_for: None,
            reasoning_effort: None,
            tool_context: None,
            live_video: false,
        }))
        .await
        .expect("send StartTurn");

    let turn_started = Instant::now();
    let mut delta_buf = String::new();
    let mut delta_count: usize = 0;
    let mut completed = false;
    let mut last_event_at: Option<Instant> = None;

    while turn_started.elapsed() < TURN_TIMEOUT {
        let remaining = TURN_TIMEOUT.saturating_sub(turn_started.elapsed());
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Some(TransportEvent::EphemeralNotification { payload })) => {
                if let UiNotification::MessageDelta(d) = payload {
                    delta_count += 1;
                    delta_buf.push_str(&d.text);
                    last_event_at = Some(Instant::now());
                }
            }
            Ok(Some(TransportEvent::DurableNotification { payload, .. })) => {
                last_event_at = Some(Instant::now());
                let m = payload.method();
                eprintln!("smoke: durable notification {m}");
                match payload {
                    UiNotification::TurnCompleted(_) => {
                        completed = true;
                        break;
                    }
                    UiNotification::TurnError(e) => {
                        panic!("smoke: turn/error code={} message={}", e.code, e.message);
                    }
                    _ => {}
                }
            }
            Ok(Some(TransportEvent::RpcError { method, error, .. })) => {
                panic!("smoke: RpcError during turn ({method}): {} {}", error.code, error.message);
            }
            Ok(Some(TransportEvent::RpcResult(LifecycleResult::TurnStart(r)))) => {
                eprintln!("smoke: turn/start accepted={}", r.accepted);
                last_event_at = Some(Instant::now());
            }
            Ok(Some(TransportEvent::ConnectionState(s))) => {
                eprintln!("smoke: connection state {s:?}");
                if matches!(s, ConnectionState::Failed) {
                    panic!("smoke: transport reached Failed state during turn");
                }
            }
            Ok(Some(other)) => eprintln!("smoke: turn event {other:?}"),
            Ok(None) => panic!("smoke: event channel closed during turn"),
            Err(_) => break,
        }
    }

    let turn_elapsed = turn_started.elapsed();
    eprintln!(
        "smoke: turn done completed={} delta_count={} delta_len={} elapsed={:?} last_event_age={:?}",
        completed,
        delta_count,
        delta_buf.len(),
        turn_elapsed,
        last_event_at.map(|t| t.elapsed()),
    );
    eprintln!("smoke: streaming text >>>{delta_buf}<<<");

    // Always try to disconnect cleanly, even if the assertions below would fail.
    let _ = cmd_tx.send(OutboundCommand::Disconnect).await;

    assert!(completed, "expected turn/completed within {TURN_TIMEOUT:?}");
    assert!(delta_count > 0, "expected at least one MessageDelta");
    assert!(!delta_buf.trim().is_empty(), "expected non-empty streamed text");
    // Sanity-check the method constants we rely on.
    assert_eq!(methods::TURN_COMPLETED, "turn/completed");
    assert_eq!(methods::MESSAGE_DELTA, "message/delta");
}

/// 8-char monotonic-ish suffix (uuid v7 truncated) — purely for log readability.
fn uuid_short() -> String {
    let id = TurnId::new().0.to_string();
    id.chars().take(8).collect()
}
