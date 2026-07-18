# Architecture

octos-one is an **agent operating system on a phone**. Instead of hand-coded
screens, a routing agent (the **AMA**) dispatches each user intent to a
concurrent **app agent** that *generates* a live, full-screen card as its
answer — and when no app covers the intent, the AMA **composes a new one**. This
doc explains the moving parts and how a single request flows through them.

- [1. The mental model](#1-the-mental-model)
- [2. Decision → activation → composition (the control plane)](#2-decision--activation--composition-the-control-plane)
- [3. The a2app memory (how an agent knows an app)](#3-the-a2app-memory-how-an-agent-knows-an-app)
- [4. Composition (how the AMA grows the app set)](#4-composition-how-the-ama-grows-the-app-set)
- [5. The Splash card + live data binding](#5-the-splash-card--live-data-binding)
- [6. The card validator (lint → repair)](#6-the-card-validator-lint--repair)
- [7. Security: cards render before they are validated](#7-security-cards-render-before-they-are-validated)
- [8. Transport: the embedded octos kernel](#8-transport-the-embedded-octos-kernel)
- [9. Component map (where things live)](#9-component-map-where-things-live)

---

## 1. The mental model

Two kinds of actor, all running at once as independent **octos sessions** (each
is a stateful server-side conversation with its own context window):

| Actor | Role | Renders UI? | Writes memory? |
|---|---|---|---|
| **AMA** (Activity Management Agent) | Classifies each intent, decides which app agent takes the screen, and **authors a new app spec** when nothing covers a multi-domain intent. | No — its answer is a one-line routing/compose decision. | Yes — only into `apps/` (composition). |
| **App agent** — one per app | Owns generation for its app. Emits a `runsplash` card as its streamed answer. | Yes — when activated, its card is the screen. | No. |

The built-in apps are **weather, stock, news, activity**, and the composed
**weather-activity**; the AMA can mint more at runtime (see §4). Boot creates a
domain agent for the first few; others (tree-declared or newly composed) get a
peer agent created on demand.

Why separate sessions per app? Each agent's context stays **dedicated** to its
app (the weather agent never sees stock chatter), which keeps generations
focused and lets the AMA route without cross-talk. They share one *display
surface* (the phone screen); the AMA's decision selects whose card is on it.

This lives on top of a **Layer-3 multi-session client** (`app/src/main.rs`:
`App.apps: Vec<AppRecord>`, `App.foreground`) — a small window-manager over N
sessions, where `foreground` indexes the visible one.

---

## 2. Decision → activation → composition (the control plane)

The end-to-end flow for one request (all identifiers are real code in
`app/src/main.rs`):

```
 clear_chat()
   └─ creates domain app agents (AppRecord{domain:"weather"/"stock"/"news"})
      + the AMA session (ama_session, cwd-hinted into the app-cards apps/ dir).

 submit_prompt(text)                                    // user hits send
   └─ REJECTED if a turn is already in flight (ama_prompt set OR is_streaming)
   └─ splash mode: send text ONLY to the AMA (router prompt),
      hold it in `pending_intent`. No app agent runs yet.

 AMA streams its decision  ──▶  ama_text accumulates
 AgentEvent::TurnComplete for ama_prompt
   └─ parse_ama_decision(ama_text) → (is_compose, app_id)   // robust parse, see below
   └─ is_compose  → compose_app(app_id): create a NEW peer agent for the
                    just-authored spec, then route the held intent to it.
   └─ else, app_id known (has a boot agent)     → route_to_app(app_id)
   └─ else, app_id has a spec on disk           → compose_app(app_id)   // create-if-missing
   └─ else (hallucinated / "none")              → fall back to weather

 route_to_app(app_id, decision):
   • foreground = i                            // this agent takes the screen
   • send app_splash_router_for(app_id, intent) to apps[i].session
   • apps[i].current_prompt = pid ; repair_attempted = false

 apps[i] streams its runsplash card  ──▶  CHAT_DATA (the shared surface)
   • the STREAMING render path defers an unclosed block and neutralizes any
     block that trips the security gate (§7)
 AgentEvent::TurnComplete for that pid
   • the card is validated (§6); a violation fires ONE repair turn
   • card committed (neutralized text stored), is_streaming = false
```

Key pieces:

- **`AMA_SYSTEM_PROMPT`** primes the AMA as a *router and composer*. Because
  `session/open` carries no per-session system-prompt field, the persona is
  **inlined into the AMA's message** each turn (`submit_prompt`).
- **`parse_ama_decision()`** is deliberately robust. The contract is a final
  `<app-id> — <reason>` / `compose <id> — <reason>` / `none`, but models narrate
  first and run the decision onto the same line with no newline, wrap it in
  markdown, and put em-dashes in the reason. The parser anchors on the `compose`
  keyword (requiring a multi-part kebab id) then the FIRST em-dash separator,
  strips markdown, and trims stray hyphens. (Unit-tested against the observed
  failure blobs.)
- **`route_to_app()`** is the activation hook. A domain WITHOUT a boot agent
  (tree-declared apps like `activity`, or a composed app after a restart) falls
  through to `compose_app`, which creates the peer session on demand — same
  fresh-injection guarantee as an explicit `compose`.
- **`app_splash_router_for(app_id, intent)`** is the app-locked generation prompt:
  "build the app you were routed to, follow `apps/<id>/app.md`, assemble it from
  the injected widget patterns, bind live data with the `sys.*` helpers ITS spec
  names." (It does NOT hardcode the app list — it defers to the routed id, so
  activity/composed apps get correct instructions.)
- **Foreground guard**: streaming `AgentEvent`s carry a `prompt_id`, not a session
  id. A *background* app's events are badged, never written to the visible
  `CHAT_DATA`. A cancelled AMA turn's late deltas are dropped (not leaked as card
  text).

> **Latency note.** This serializes: AMA (~7 s) then generation (~30 s;
> composition adds a spec-authoring turn). Roadmap cuts: route on the AMA's
> *first line*; send a *refinement* of the current card ("make it dark") straight
> to the foreground agent, skipping the AMA; a two-tier AMA (a cheap local
> classifier for the confident majority, LLM AMA only on ambiguous input).

---

## 3. The a2app memory (how an agent knows an app)

An app agent has **no app-specific code**. Everything it knows — the framework
rules, the reusable widget vocabulary, the design system, and each app's
requirements spec — is **injected into its context as memory** by the octos
kernel every turn.

- The source of truth is **`a2app/`** in this repo:
  ```
  a2app/
    framework.md                     global rules, the routing list, the composer
                                     section, and the card security rules
    widgets/
      design-system.md               the enforced global stylesheet (color tokens,
                                     type scale, spacing, layering, emphasis)
      interaction.md                 reusable patterns: state, tap→state, chip rows,
                                     Splash-local nav, ScrollYView tap targets
      containers.md, weather-icon.md widget vocabulary
      sys-helpers.md                 every sys.* live-data helper + StockPlot
      framework/splash-manual.md     the full DSL reference (read on demand)
    apps/<id>/app.md                 the app's REQUIREMENTS spec (mandatory sections,
                                     failure conditions) — NOT a card to copy
    apps/<id>/lint.json              machine-checkable rules for the validator (§6)
  ```
- **There are NO exemplars.** An earlier design shipped a known-good `.splash`
  card per app; the on-device model copied it verbatim (byte-identical
  reproductions) and couldn't generalize to new structure or new apps. Now every
  app is **assembled** from the widget patterns + its requirements spec. That is
  what makes composition (§4) possible: a novel app is just a new requirements
  spec pointing at the same shared pieces.
- **octos assembles the tree itself.** The kernel's memory store concatenates the
  tree (framework → widgets → each app's `app.md`, with `===== <relpath> =====`
  delimiters) into the injected long-term memory *at inject time*. No build step,
  no generated artifact; the `a2app/` tree is the on-disk source of truth. See
  `octos/crates/octos-memory/src/memory_store.rs` → `assemble_app_cards`.
- On the device, the tree is deployed to
  `…/octos-home/.octos/profiles/_main/data/memory/app-cards/`, and the model
  injects it up to `config.memory.max_inject_tokens` in the **kernel config**
  (`…/octos-home/.config/octos/config.json`, NOT `_main.json`). The app **sets
  this itself at boot** (`ensure_kernel_memory_budget`, 40000) — an absent or
  too-low value is upgraded; a higher operator value is respected.

> ⚠️ **Token budget.** The assembled tree grows with each app (composed apps add
> to it at runtime). If it exceeds `max_inject_tokens`, octos truncates the
> *tail* — silently dropping the last app. The boot auto-config keeps the cap
> comfortably above the tree. (Also delete a stale legacy `MEMORY.md` in the
> profile memory dir — the kernel appends it after the tree, which can blow the
> budget.)

---

## 4. Composition (how the AMA grows the app set)

When a request spans MORE THAN ONE domain and no app in the routing list
(composed apps included) covers it, the AMA does not answer `none` — it
**authors a new app** and hands it to a fresh peer agent:

```
 AMA (composer section of framework.md):
   1. pick the parent apps whose data covers the request (e.g. weather + news)
   2. write_file  <a>-<b>/app.md   — a requirements spec that MERGES the parents'
                                     named BLOCKS (e.g. weather's BLOCK: CURRENT,
                                     BLOCK: PHOTO-BACKDROP) and inherits the PRIMARY
                                     parent's visual identity; data ONLY via sys.*
   3. write_file  <a>-<b>/lint.json — the validator rules for the new app
   4. reply       `compose <a>-<b> — <reason>`

 client (compose_app):
   • create a NEW peer agent session for <a>-<b>
   • that fresh session injects the just-written spec at bootstrap
   • route the held intent → the peer builds the app → its card takes the screen
   • the spec persists: next matching request routes to <a>-<b> directly
```

Two design points make this safe and coherent:

- **Named blocks, not free redesign.** Parent specs expose reusable blocks by
  heading (`BLOCK: CURRENT`, `BLOCK: PHOTO-BACKDROP`, …). A composed spec
  references and reproduces those blocks, and inherits the primary parent's
  backdrop/frame — so `weather-news` looks like the weather app with news on its
  panels, not a generic gradient.
- **Confined writes.** The AMA session's workspace is cwd-hinted into the
  app-cards **`apps/`** subdir (`app_cards_memory_dir`), and the kernel fences
  file writes to the session workspace. So the composer can create
  `apps/<id>/…` but **cannot** touch `framework.md`, `widgets/`, or `MEMORY.md`
  (poisoning those would corrupt every app's context). Create-only on an existing
  sibling spec is not yet kernel-enforced — tracked.

> Freshly-authored specs are seen by NEWLY opened sessions (the composer's own
> already-open session won't re-read them — the memory fingerprint doesn't stat
> the app-cards tree, an upstream octos gap). The pipeline only ever hands a
> composed spec to a *new* peer agent, so this is correct.

---

## 5. The Splash card + live data binding

A card is a **`runsplash` fenced block** of Makepad **Splash DSL** — a
declarative widget tree (`View`, `RoundedView`, `Label`, `Image`,
`GradientYView`, `StockPlot`, …). The renderer is the `Splash` widget
(`aichat/widgets/src/splash.rs` in the framework fork), which evaluates the DSL
in an isolated script VM and builds live widgets.

### Live data is bound by the DSL, not written by the LLM

The card calls **data helpers** as a widget's text/property; the *runtime*
fetches the real value at render time:

```
Label{ text: "$" + sys.stock("AAPL", "price") }                    // Yahoo Finance
Label{ text: sys.weather(48.85, 2.35, "current.temperature_2m") + "°" }  // open-meteo
Label{ width: Fill text: sys.news(0, "title") }                    // Hacker News
Label{ text: sys.places(37.34, -121.89, "park", 0, "name") }       // OpenStreetMap
StockPlot{ width: Fill height: 160 symbol: sel range: rng }        // the price chart
```

The LLM writes the binding — it never types `317.31`. The value is correct to
the cent because it was fetched, not recalled. Numeric variants
(`sys.weathernum`, `sys.aqinum`, `sys.placesnum`) return the live value as a
**number** so a card can *branch* on it (`if temp >= 18 { …outdoor… }`) — the
enabling primitive for composed apps like weather-activity.

### How a helper works (framework: `platform/src/script/res.rs`, `widgets/src/splash.rs`)

1. The `sys.*` helper builds a URL (tickers pass through `sanitize_ticker`) and
   calls **`Cx::script_data_fetch(url)`**.
2. `script_data_fetch` is a **URL-keyed side-table** (`data_fetches`): loaded →
   return bytes; otherwise fire ONE async request (deduped by URL, host-aware
   `User-Agent`) and return `None`. Failures get **bounded, backed-off retries**;
   an empty/parse-failed 2xx or a permanent 4xx is terminal (no infinite retry).
3. The response is routed in `platform/src/script/std.rs`, which stores the bytes
   (or classifies the failure) and `redraw_all()`s.
4. Back in the helper, **`json_pluck(bytes, path)`** navigates the JSON and
   returns the field — or `"—"` while loading.

### Re-evaluation on data arrival

A `Label` bakes its text **once**, at eval time (first render bakes `"—"`), and a
plain `redraw_all()` won't change it. So a global **`DATA_FETCH_EPOCH`** is
bumped whenever a fetch newly loads (or fails); the `Splash` widget re-evaluates
its body once when the epoch changes, so `sys.*` re-runs and returns the loaded
value. `body_binds_live_data()` gates this to cards that call a `sys.*` helper —
**keep it in sync when you add a helper.** That's why a card shows `—` for a
moment, then fills in.

### StockPlot — a vendored chart widget

`StockPlot{ symbol range }` renders a real line/area price chart (from
[mofa-org/makepad-matplot](https://github.com/mofa-org/makepad-matplot), MIT,
vendored under `widgets/src/matplot/`). It fetches through the SAME deduped
Yahoo cache as `sys.stockbar`/`sys.stockrange` (one request per symbol×range
serves the plot, the bars, and every scalar), self-pumps until data lands, and
colors itself green/red by range direction. It is the *preferred* stock chart;
`sys.stockbar` (a bar-height helper) remains as a fallback.

### Two ways to extend — and only one needs a rebuild

| Axis | What it is | Rebuild? |
|---|---|---|
| **App package** — `a2app/apps/<id>/` (spec + lint) | Teaches an agent a new app *using capabilities that already exist*. Pure content. | **No.** Redeploy the `a2app/` tree. |
| **Data / widget capability** — a `sys.*` helper or widget in the framework fork | A shared native primitive that fetches a live source (`sys.stock`, `sys.places`, …) or draws (`StockPlot`). | **Yes** — native code compiled into the APK. |

A card that reuses existing helpers is content-only. A card that needs a *new*
live source (or a new shaping of one) is a **framework change**: add the helper,
keep `body_binds_live_data()` in sync, rebuild. Think of the `sys.*` helpers as a
**shared standard library**, not per-app glue. See
**[ADDING-AN-APP-CARD.md](ADDING-AN-APP-CARD.md)**.

---

## 6. The card validator (lint → repair)

One-shot generation occasionally drops a required piece (a tap overlay, the
`// name:` line, a state key). App-spec "failure conditions" are prose the model
can ignore, so each app ships **executable** rules — `apps/<id>/lint.json`, plain
`(pattern, min-count)` substring checks — and the client enforces them:

```
card completes → card_lint::load_rules(app_id) → lint(body, rules)
  → violations?  yes → ONE repair turn to the SAME agent (the violation list is
                       fed back as the prompt; the corrected card streams over
                       the imperfect one). Budget: one repair per routed intent.
```

`card_lint.rs` loads the rules from the deployed tree (falling through a
malformed preferred file to a valid shipped one). Substring lint can't express
OR / forbid / distinct-count, so a few spec rules stay teaching-level — those are
documented, not silently claimed. A geometry/vision **pre-display** gate (render
the card off-screen, judge it *before* it's shown) is scoped in the research
notes as the next tier.

---

## 7. Security: cards render before they are validated

Cards are LLM-generated (semi-trusted: prompt injection into the generating model
is the realistic threat) and render **live, mid-stream**, before the
completion-time lint runs. Two properties matter:

- **Cards may only read.** The Splash DSL exposes a low-level `net.*` HTTP API
  (POST/PUT/DELETE) that a hallucinated or injected card could use to exfiltrate
  data. Cards are supposed to use only `sys.*` helpers and image
  `http_resource` (GET). A **security gate** (`runsplash_body_forbidden` /
  `neutralize_forbidden_cards`) refuses any card that calls the low-level net
  API. It normalizes the body (strips comments + whitespace) and forbids the
  method names on any receiver, so `net . http_request`, `net./*x*/http_request`,
  and aliasing are all caught; it scans **every** block and neutralizes the fence
  at both the render seam and at store time (so a blocked card can't re-surface
  via history/hydrate). It is **not a hard boundary** — a substring scan can't
  fully sandbox a scripting language; the real fix is VM-level capability gating
  (cards shouldn't see `net.*` at all). Documented in-code.
- **The composer can only write under `apps/`** (see §4).

---

## 8. Transport: the embedded octos kernel

On Android there is **no separate server process**. The octos kernel is bundled
into the APK as `liboctos.so` and run **in-process over stdio**:

- `app/src/main.rs::stdio_spawn()` execs `liboctos.so serve --stdio` (NDJSON
  JSON-RPC on stdin/stdout) from the app's `nativeLibraryDir` (the only
  exec-able location on Android), with `HOME` = the app data dir. It also ensures
  the kernel config's memory budget and `appui.sessions_in_cwd` at boot.
- The app talks the **octos ui-protocol** (`session/open` with an optional per-
  session `cwd`, `turn/start`, streamed `UiNotification`s) via
  `crates/octos-app-transport` (stdio transport).
- Each app agent / the AMA is a `session/open` on this one kernel; concurrency is
  the kernel's (per-session turn actors). The AMA session's `cwd` is the
  app-cards `apps/` dir (its composition write-zone).
- The `app-cards/` memory tree, the per-profile config (`_main.json`, incl. the
  LLM provider + key), the kernel config, and the skill read-zone all live under
  `octos-home/` in the app's data dir.

Desktop builds instead talk to a normal `octos serve` over WebSocket (same
protocol); Android is stdio.

---

## 9. Component map (where things live)

| Concern | Location |
|---|---|
| AMA persona / routing + composer prompt | `app/src/main.rs` — `AMA_SYSTEM_PROMPT` |
| Decision parse (robust) | `app/src/main.rs` — `parse_ama_decision` |
| Activation + on-demand composition | `app/src/main.rs` — `route_to_app`, `compose_app`, `app_spec_exists`, `app_cards_memory_dir` |
| Multi-session window manager | `app/src/main.rs` — `App.apps`, `foreground`, `app_of_prompt`, `focus_session`, `switch_to_app` |
| App-agent generation prompt | `app/src/main.rs` — `APP_SPLASH_ROUTER`, `app_splash_router_for` |
| Card validator (lint → repair) | `app/src/app/card_lint.rs`; wired in `main.rs` (turn-complete) |
| Security gate | `app/src/main.rs` — `runsplash_body_forbidden`, `neutralize_forbidden_cards` |
| Kernel budget auto-config | `app/src/main.rs` — `ensure_kernel_memory_budget` |
| Transport (stdio/ws, ui-protocol, per-session cwd) | `app/crates/octos-app-transport`, `app/src/backend/octos_ui.rs` |
| Splash renderer + `sys.*` helpers | **framework fork** `widgets/src/splash.rs` |
| StockPlot + vendored plot engine | **framework fork** `widgets/src/matplot/` |
| Async data-fetch engine (dedup, retries) | **framework fork** `platform/src/script/res.rs`, `std.rs` |
| App-card definitions (memory) | `a2app/` → deployed as `app-cards/`; octos assembles it in `octos-memory/src/memory_store.rs` (`assemble_app_cards`) |
| Agent runtime | `liboctos.so` from [`octos-org/octos`](https://github.com/octos-org/octos) |

Next: **[ADDING-AN-APP-CARD.md](ADDING-AN-APP-CARD.md)** to add a new app type, or
**[BUILDING-ANDROID.md](BUILDING-ANDROID.md)** to build and run.
