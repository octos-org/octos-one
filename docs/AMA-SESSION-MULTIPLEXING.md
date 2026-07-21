# AMA Session Multiplexing

How the octos-one AppUI runs six concurrent agent sessions on one kernel
connection, and how the Activity Management Agent (AMA) routes a single user
intent to exactly one of them.

This document is grounded in the code. Every claim carries a `file:line`
citation. Where the prose comments and the code disagree, the code wins and the
disagreement is called out.

| File | Role |
|---|---|
| `app/app/src/main.rs` | Makepad UI: session creation, the AMA hold-then-route state machine, the streaming guards, the streaming-surface arbitration |
| `app/app/src/backend/octos_ui.rs` | `OctosUiAgent`: the one tokio runtime, one transport task, sessions-as-HashMap-entries |
| `aichat/libs/makepad_ai/src/agent.rs` | `SessionConfig` — the per-session knobs (cwd, system_prompt, model, tools) |
| `octos/crates/octos-core/src/session_scope.rs` | The on-disk storage contract: shared `episodes.redb`, per-session transcripts, shared zones |
| `octos/crates/octos-llm/src/anthropic.rs` | The three provider prompt-cache breakpoints |
| `octos/crates/octos-memory/src/memory_store.rs` | `assemble_app_cards` — how an app's `app.md` becomes injected memory |

---

## 1. The six-session model: one agent, six logical lanes

At boot the app creates **six sessions**, all on a **single shared
`OctosUiAgent`**: five domain app agents (weather, stock, news, web, youtube)
plus one AMA.

`app/app/src/main.rs:5111-5139` (`clear_chat`, the boot/reset path):

```rust
let weather = agent.create_session(cx, app_cfg());
let stock   = agent.create_session(cx, app_cfg());
let news    = agent.create_session(cx, app_cfg());
let web     = agent.create_session(cx, app_cfg());
let youtube = agent.create_session(cx, app_cfg());
self.apps = vec![
    AppRecord::with_domain(weather, "Weather", "weather"),
    AppRecord::with_domain(stock,   "Stock",   "stock"),
    AppRecord::with_domain(news,    "News",    "news"),
    AppRecord::with_domain(web,     "Web",     "web"),
    AppRecord::with_domain(youtube, "YouTube", "youtube"),
];
self.foreground = 0;
self.pending_intent = None;
...
self.ama_session = Some(agent.create_session(cx, ama_config));
```

The comment at `main.rs:5110-5114` states the model: *"ONE app agent PER
DOMAIN, all live concurrently; each is its own octos session so its context
stays dedicated to its domain."* So six sessions exist and hold state
concurrently, but — as §3 shows — only one generates at a time.

### Sessions are lanes, not processes or tasks

`OctosUiAgent` owns **one** tokio runtime with **one** worker thread and **one**
transport task. Sessions are **entries in a `HashMap`**, not independently
scheduled units.

`app/app/src/backend/octos_ui.rs:116-117`:

```rust
let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(1)
    .enable_all()
    .build()
```

There is exactly one `(cmd_tx, evt_rx)` channel pair to the transport
(`octos_ui.rs:58-60`), created once in `OctosUiAgent::new` via
`ws::spawn_with_waker` / `stdio::spawn_with_waker` (`octos_ui.rs:121-133`). All
six sessions share it.

Each session is a pair of `HashMap` entries, not a task (`octos_ui.rs:62-65`):

```rust
session_keys: HashMap<SessionId, SessionKey>,
session_ids: HashMap<SessionKey, SessionId>,
```

`create_session` (`octos_ui.rs:585-607`) just mints a local `SessionId`, derives
a `SessionKey`, inserts both into the maps, and posts one
`OutboundCommand::OpenSession` on the shared `cmd_tx`. No thread or task is
spawned per session. Turns are demultiplexed back to sessions by id maps
(`turn_ids`, `prompt_ids`, `prompt_sessions` — `octos_ui.rs:69-75`). `send_prompt`
(`octos_ui.rs:650`) looks the session key up in `session_keys` and posts a
`StartTurn` on the same shared channel.

So "six concurrent sessions" means six rows of state on one event loop — the
multiplexing is *logical*, resolved by id lookup, not by the OS or the tokio
scheduler.

---

## 2. Hold-then-route: decision-gated, NOT a broadcast

The core flow: an intent goes to the **AMA only**, is **held** as
`pending_intent`, the AMA emits a one-line decision, and only then is the held
intent dispatched to the **one** matching app session.

> **Docs-vs-code divergence.** The `ama_session` field's own doc-comment says
> *"Every user intent is broadcast to both the AMA and the app agents"*
> (`main.rs:4232-4234`). **That is not what the code does.** The submit path
> sends to the AMA only; no app agent sees the intent until `route_to_app` runs
> after the AMA's `TurnComplete`. "Broadcast" here is stale comment prose; the
> actual mechanism is hold-then-route.

### The four AMA state fields

`main.rs:4236-4256`:

```rust
ama_session:    Option<SessionId>,   // 4236 — the router brain's session
ama_prompt:     Option<PromptId>,    // 4240 — the in-flight classification turn
cancelled_ama:  Option<PromptId>,    // 4248 — a cancelled pid whose late deltas must be dropped
ama_text:       String,              // 4251 — accumulates the streamed routing decision
pending_intent: Option<String>,      // 4256 — the held user intent
```

`pending_intent`'s doc (`main.rs:4253-4255`) is the contract in one sentence:
*"The user intent captured at submit, held while the AMA classifies it. On the
AMA's TurnComplete we dispatch this to the routed domain agent."*

### Submit → AMA only, hold the intent

`main.rs:5418-5437`. In splash mode with an AMA present, the prompt goes to the
AMA session, and the raw intent is stored, not forwarded:

```rust
let ama_msg = format!(
    "{AMA_SYSTEM_PROMPT}\n\nUser message: {text}\n\nYour one-line routing decision:"
);
(Some(agent.send_prompt(cx, ama, &ama_msg)), None)   // → AMA only; direct_pid = None
...
self.ama_prompt = Some(ama_pid);
self.ama_text.clear();
self.pending_intent = Some(text.clone());            // ← the HOLD (5437)
```

Note the second tuple element (`direct_pid`) is `None` on this branch
(`main.rs:5425`) — the foreground app agent is **not** prompted. The
`AMA_SYSTEM_PROMPT` (`main.rs:63`) instructs the model to *"reply EXACTLY ONE
short line: `<app-id> — <brief reason>`."*

### The AMA's stream is captured, never rendered

While the AMA turn runs, its deltas are appended to `ama_text` and explicitly
kept off the shared surface (`main.rs:7411-7415`):

```rust
if Some(prompt_id) == self.ama_prompt {
    self.ama_text.push_str(&text);
    continue;                       // routing metadata — never rendered
}
```

### TurnComplete → parse the decision → route

On the AMA's `AgentEvent::TurnComplete` (`main.rs:7486` ff.), the accumulated
`ama_text` is parsed by `parse_ama_decision` (`main.rs:4443`), which anchors on
the em-dash separator (`<id> — <reason>`), handles a leading `compose ` keyword,
and falls back to the last non-empty line. Then the decision is applied:
`compose` goes to `compose_app`; an unknown-but-well-formed id goes to
`compose_app` (fresh-injection); otherwise `route_to_app` (`main.rs:7517-7551`).

### route_to_app: dispatch the held intent to ONE session

`main.rs:4358-4421`. The first thing it does is **take** the held intent:

```rust
fn route_to_app(&mut self, cx: &mut Cx, app_id: &str, decision: &str) {
    let Some(intent) = self.pending_intent.take() else { return; };   // 4359
    let Some(idx) = self.apps.iter()
        .position(|a| a.domain.as_deref() == Some(app_id))            // 4363 — find the ONE matching app
    else { /* "none": render nothing, clear is_streaming */ return; };
    self.foreground = idx;                                            // that app takes the screen
    ...
    let sid = self.apps[idx].session_id;                              // 4417 — ONE session id
    let prompt = app_splash_router_for(app_id, &intent);              // 4418 — domain-specialised prompt
    let pid = self.agent.as_mut().unwrap().send_prompt(cx, sid, &prompt); // 4419 — ONE send_prompt
    self.apps[idx].current_prompt = Some(pid);
```

Only the single matching session receives `send_prompt`. The other four app
sessions are untouched. This is **decision-gated dispatch to one lane**, the
opposite of a parallel broadcast.

---

## 3. Time-multiplexing: concurrent sessions, serialized turns

Six sessions hold state concurrently, but **only one turn is ever in flight**.
Generation is serialized by a guard at the top of the submit handler.

`main.rs:5361-5372`:

```rust
// Reject a new submit while ANY turn is in flight — the AMA routing
// turn (singleton `ama_prompt`/`ama_text`/`pending_intent`) OR the
// routed app's generation turn. Both share the singleton streaming
// surface; a second submit mid-turn overwrites it and the first turn's
// late deltas leak in as foreground text. `is_streaming` is set for the
// whole window (submit → TurnComplete), so it covers both phases.
if self.ama_prompt.is_some() || CHAT_DATA.read().unwrap().is_streaming {
    log::info!("submit ignored: a turn is still in flight (Cancel to abort)");
    return;
}
```

Two conditions, one gate:

- `self.ama_prompt.is_some()` — an AMA routing turn is in flight.
- `CHAT_DATA…is_streaming` — set at submit (`main.rs:5385`) and cleared at
  `TurnComplete` / cancel, covering the routed app's generation turn.

Because the AMA turn and the routed app turn run **sequentially within one
submit** (AMA finishes, *then* `route_to_app` starts the app turn), and a second
submit is rejected for the whole window, at most one LLM generation is active
at any instant. This is the sense in which the design is
**time-multiplexed**: many live sessions (state), one active turn (compute).

Why serialize at all? The AMA stream, the routed app's stream, and the rendered
card all share a **single** `CHAT_DATA` surface (`streaming_text`,
`authoritative_text`, the message list). The guard's comment is explicit that a
second concurrent turn would corrupt that surface.

### The late-delta drop guard (cancel race)

Cancelling is synchronous on the client but the server interrupt is async, so a
delta already in flight can arrive *after* `ama_prompt` is cleared. Without a
guard it would fall through to the foreground and leak as card text.

`cancel_request` (`main.rs:5456-5465`) stashes the pid and releases the intent:

```rust
if let Some(ama_pid) = self.ama_prompt.take() {
    agent.cancel_prompt(cx, ama_pid);
    self.cancelled_ama = Some(ama_pid);   // remember it
    self.ama_text.clear();
    self.pending_intent = None;           // release the held intent
    ...
}
```

`TextDelta` drops anything for the cancelled pid (`main.rs:7407-7409`):

```rust
if Some(prompt_id) == self.cancelled_ama {
    continue;   // stale routing metadata — drop, don't stream as card text
}
```

and `TurnComplete` swallows the straggler and frees the slot (`main.rs:7490-7493`):

```rust
if Some(prompt_id) == self.cancelled_ama {
    self.cancelled_ama = None;
    continue;
}
```

The same belt-and-suspenders check is applied to `TextAuthoritative`
(`main.rs:7391-7394`).

---

## 4. Per-session isolation: own system prompt; only the AMA sets cwd

Each session is configured via `SessionConfig`
(`aichat/libs/makepad_ai/src/agent.rs:34-43`):

```rust
pub struct SessionConfig {
    pub cwd: Option<String>,            // working directory for the agent
    pub system_prompt: Option<String>,  // system prompt / instructions
    pub model: Option<String>,          // model to use (if selectable)
    pub tools: Vec<ToolDefinition>,     // tool definitions exposed to the backend
}
```

The five app agents get a shared placeholder system prompt and **no cwd**
(`main.rs:5105-5108`):

```rust
let app_cfg = || SessionConfig {
    system_prompt: Some(OCTOS_PLACEHOLDER_SYSTEM_PROMPT.to_string()),
    ..Default::default()        // cwd = None, model = None, tools = []
};
```

The AMA is the only session that overrides `cwd` (`main.rs:5134-5138`):

```rust
let ama_config = SessionConfig {
    cwd: Self::app_cards_memory_dir(),
    system_prompt: Some(AMA_SYSTEM_PROMPT.to_string()),
    ..Default::default()
};
```

`app_cards_memory_dir` (`main.rs:5023-5032`, Android-only) points the AMA's
workspace at the app-cards **`apps/`** subdirectory. The comment
(`main.rs:5016-5022`) explains this is blast-radius reduction: the kernel fences
writes to the session workspace, so rooting the AMA one level down means the
composer can only touch `apps/<id>/` and cannot corrupt `framework.md`,
`widgets/`, or `MEMORY.md` (which would poison *every* app's injected context).

The per-session `cwd` travels on the wire in `session/open`
(`octos_ui.rs:592-605`):

```rust
self.post(OutboundCommand::OpenSession(SessionOpenParams {
    session_id: key,
    ...
    cwd: config.cwd.or_else(|| self.workspace_cwd.clone()),  // per-session override wins
    ...
}));
```

So the AMA's routing persona and the app agents' domain persona are distinct
system prompts on distinct sessions, and only the AMA is sandboxed into the
shared card tree.

---

## 5. Storage: shared `episodes.redb`, separate transcripts, shared zones

The on-disk layout is the multi-tenant `SessionScope` contract
(`octos/crates/octos-core/src/session_scope.rs:70-81`):

```text
<config_dir>/profiles/<tenant_id>/
├── data/                         ← SessionScope.root
│   ├── users/<session_id>/
│   │   └── workspace/            ← SessionScope.workspace (per-session, ephemeral)
│   ├── research/                 ← shared_zones[0]  (cross-session)
│   ├── skills/                   ← shared_zones[1]  (cross-session, persistent)
│   └── episodes.redb             ← OutOfScope (memory store, via API)
```

Three facts matter for multiplexing:

1. **`episodes.redb` is ONE per profile and shared** across all six sessions
   (`session_scope.rs:77`). It is *not* a per-session file. The scope marks it
   `OutOfScope` — memory is accessed via an API, never as a CWD or raw path
   (the test at `session_scope.rs:1373-1377` asserts resolving
   `episodes.redb` is rejected as system internals). Because there is a single
   process (one `octos` kernel) holding one redb handle, there is **no
   cross-process lock contention**; redb's file lock is per-process and only one
   process exists.

2. **Per-session transcripts are separate.** Each session's state lives under
   `users/<session_id>/` (`session_scope.rs:73-74`, and the resolver constant at
   `session_scope.rs:141-147`). The workspace is *per-session and ephemeral*.
   The validator actively rejects any shared zone that overlaps the per-session
   `users/` subtree (`session_scope.rs:248`: *"cross-session isolation requires
   zones live outside `users/`"*).

3. **`research/` and `skills/` are shared cross-session zones**
   (`session_scope.rs:75-76`, and `DEFAULT_MULTI_TENANT_SHARED_ZONE_NAMES =
   &["research", "skills"]` at `session_scope.rs:413`). Skill installs and the
   research cache are intentionally common to all sessions.

A kernel-config caveat ties storage to the AMA's cwd hint: the app force-sets
`appui.sessions_in_cwd: false` at boot (`main.rs:4939-4940`). The comment
(`main.rs:4928-4932`) explains that otherwise the kernel would relocate the
cwd-hinted AMA session's **transcripts into the card tree**. Keeping the flag
`false` keeps transcripts in the profile's per-session store and out of
`apps/`.

---

## 6. LLM-side prompt caching: six conversations, three breakpoints

### Six distinct LLM conversations, not one

Because each session is a separate server-side octos session with its own
system prompt and its own message history (§1, §4), the provider sees **six
independent conversations**, not a single interleaved one. The cache key prefix
differs per session because the system prompt differs (AMA's router/composer
prompt vs. each app agent's domain persona), so each session gets its **own**
prefix-cache entry. They do not share a conversation prefix and cannot thrash
each other's history cache.

### The three `cache_control` breakpoints

The Anthropic provider emits up to three ephemeral breakpoints
(`octos/crates/octos-llm/src/anthropic.rs:107-111`):

> *"the request carries three ephemeral `cache_control` breakpoints (Anthropic
> allows up to 4): the system-prompt block, the LAST tool definition, and the
> last content block of the LAST user-role message — caching the stable prefix
> (tools + system) plus the rolling conversation history across loop
> iterations."*

Concretely, in `build_request`:

1. **System prompt** (`anthropic.rs:156-164`): when caching is on and the system
   text is non-empty, it's sent as a block array carrying
   `cache_control: Some(cc)` — the only wire shape that can hold a breakpoint.
2. **Last tool** (`anthropic.rs:180-186`): a breakpoint is placed on the final
   tool only — *"One breakpoint on the LAST tool caches the whole
   (deterministically ordered) tool array."* Because the tool array order is
   deterministic and **identical across sessions**, this tool-prefix cache entry
   is the one win that is **shared across all six sessions**: any session's
   request reuses the same cached tools+prefix.
3. **Last user message** (`anthropic.rs:457-477`,
   `apply_message_cache_breakpoint`): a rolling breakpoint on the last content
   block of the last user-role message. The comment notes the marker *"moves
   forward each round … advancing the marker EXTENDS the cache rather than
   invalidating it."* This one is per-session (each session has its own
   conversation tail).

### Why interleaving doesn't thrash the cache

If all six sessions took turns against one shared prefix, each turn would
invalidate the previous session's rolling-history breakpoint. That doesn't
happen, for two reasons grounded above:

- **Serialized turns (§3):** only one session generates at a time, so a
  session's rolling breakpoint isn't invalidated by a sibling mid-turn.
- **AMA + one app per intent (§2):** a single user intent touches at most two
  sessions — the AMA (routing) and the one routed app (generation). The other
  four sessions are idle, so their cache entries stay warm. Only the routed
  pair's prefixes are in play for any given intent.

---

## 7. How AMA-composed apps propagate: write to the shared tree, inject at session-open

When no existing app covers a multi-domain intent, the AMA **authors** a new
app rather than answering `none` (`AMA_SYSTEM_PROMPT`, `main.rs:63`): it writes
`apps/<a>-<b>/app.md` and `lint.json` into its cwd — which is the app-cards
`apps/` tree (§4) — then replies `compose <a>-<b> — <reason>`.

The client side, `compose_app` (`main.rs:4497`), is *"only plumbing"*:

- **Idempotency** (`main.rs:4500-4503`): if a peer session for the domain
  already exists, just `route_to_app`.
- **Hallucination guard** (`main.rs:4509-4518`): require the spec to exist on
  disk via `app_spec_exists` (`main.rs:4994-5006`, which checks
  `…/memory/app-cards/apps/<id>/app.md` and the `a2app/apps` fallback) before
  spinning up a session; otherwise fall back to `weather`.
- **Fresh peer session** (`main.rs:4525-4532`): create a **new** session with
  the same placeholder config, then `route_to_app` (`main.rs:4552`). The intent
  was deliberately left pending so it routes into the fresh session.

The propagation mechanism is the memory injector, `assemble_app_cards`
(`octos/crates/octos-memory/src/memory_store.rs:445-486`): it concatenates
`framework.md`, then `widgets/*`, then **each** `apps/<id>/app.md` (with
`===== <relpath> =====` delimiters) into one injectable manual. `ARCHITECTURE.md`
(§3) documents that this manual is injected as long-term memory.

**Crucially, injection happens at session-OPEN, not per-trigger.** The
`compose_app` comment states the guarantee: *"a fresh session gets the updated
memory injected on open"* (`main.rs:4525-4529`), and the boot AMA-session
comment repeats it: new specs *"land where every NEWLY OPENED app-agent session
injects them from"* (`main.rs:5130-5134`). That is precisely why `compose_app`
creates a **new** session for a composed app rather than reusing an open one:
an already-open session will not re-read the tree (`ARCHITECTURE.md` §3 warns
the memory fingerprint doesn't re-stat it). On **desktop** there is no
on-device tree (`app_spec_exists` returns `true` unconditionally,
`main.rs:5008-5011`), so composition isn't gated on disk there.

---

## 8. Sequence: boot → six sessions → submit → AMA turn → decision → routed turn → card → free

```
 BOOT (clear_chat, main.rs:5102-5140)
   AppUI                OctosUiAgent (1 runtime, 1 worker, 1 transport)      octos kernel
     | create_session(weather/stock/news/web/youtube)  x5                     |
     |--------------------------------------------------> OpenSession x5 --->| 5 session/open
     | create_session(AMA, cwd=app-cards/apps/, sys=AMA_SYSTEM_PROMPT)        |
     |--------------------------------------------------> OpenSession ------>| 1 session/open (cwd-hinted)
     |   apps=[5 AppRecords], foreground=0, pending_intent=None               |
     |   6 sessions now hold state concurrently; none generating              |

 SUBMIT (handle submit, main.rs:5361-5437)
   User --> AppUI
     | guard: ama_prompt.is_some() || is_streaming ?  -> reject (serialize turns)
     | is_streaming = true                                  (main.rs:5385)
     | send_prompt(AMA, AMA_SYSTEM_PROMPT + text + "routing decision")        |
     |--------------------------------------------------> StartTurn(AMA) --->|
     | ama_prompt = pid;  pending_intent = text  (HOLD, main.rs:5435-5437)    |

 AMA TURN (the ONLY turn in flight)
   kernel -- TextDelta(ama_pid) --> AppUI : ama_text += delta; NOT rendered   (main.rs:7411)
        ... (one-line decision streams into ama_text) ...
   kernel -- TurnComplete(ama_pid) -> AppUI                                  (main.rs:7486)
     | parse_ama_decision(ama_text) -> (is_compose, app_id)                   (main.rs:4443)
     | ama_prompt = None

 DECISION -> ACTIVATION
   if compose / unknown-id  -> compose_app(app_id)                           (main.rs:4497)
     |   app_spec_exists? -> create_session(NEW peer) -> session/open (fresh memory inject)
     |   route_to_app(app_id)
   else                      -> route_to_app(app_id)                         (main.rs:4358)
     |   intent = pending_intent.take()            (the held intent)
     |   idx = apps.position(domain == app_id)     (the ONE match)
     |   foreground = idx
     |   send_prompt(apps[idx].session, app_splash_router_for(app_id, intent)) (main.rs:4419)
     |--------------------------------------------------> StartTurn(app) --->|

 ROUTED APP TURN (now the only turn in flight)
   kernel -- TextDelta(app_pid) --> AppUI : streaming_text += delta          (main.rs:7437)
        ... (runsplash card DSL streams) ...
   kernel -- TurnComplete(app_pid) -> AppUI
     | prefer authoritative_text over streamed (delta-loss guard)            (main.rs:7576-7590)
     | extract_runsplash_body -> render card -> card takes the screen
     | is_streaming = false   -> surface FREES, next submit is accepted
```

Key invariants visible in the trace:

- At any instant **at most one** `StartTurn` is outstanding — first the AMA's,
  then (after `TurnComplete`) the routed app's.
- The intent is forwarded **once**, to **one** app session, and only after the
  AMA decides.
- The AMA's tokens never reach the card surface; only the routed app's do.
- `is_streaming` brackets the whole submit→TurnComplete window, freeing the
  surface for the next intent only when the routed card has rendered.

---

## Appendix: docs-vs-code divergences

| Where | Doc/comment says | Code does |
|---|---|---|
| `main.rs:4232-4234` (`ama_session` comment) | "Every user intent is **broadcast** to both the AMA and the app agents" | Intent goes to the **AMA only** (`main.rs:5421-5425`); app agents get it later via `route_to_app` after the AMA's `TurnComplete`. It is hold-then-route, not broadcast. |
| `main.rs:4233` / `main.rs:7496` | "AMA MVP: it renders nothing" | Still true — `ama_text` is collected for logs (`main.rs:7413`) and never rendered — but the AMA is no longer inert: its `TurnComplete` drives `route_to_app` / `compose_app`. |
| `ARCHITECTURE.md` §3 ("injected every turn") vs §3 note / `main.rs:4525` | "injected into its context as memory by the octos kernel **every turn**" | Re-read happens at **session-open**; an already-open session does not re-stat the tree (`main.rs:4525-4529`, `5130-5134`), which is the whole reason composed apps need a *fresh* session. |
