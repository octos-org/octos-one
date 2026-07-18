# octos-one

An **agent-OS phone client**: a native Android app where a routing brain (the
**AMA**) dispatches every request to a concurrent **app agent** that generates a
live, full-screen interactive card. Cards come in **two substrates** — native
**Splash DSL** cards and **webview** (HTML/JS) cards — both binding **real data at
render time** (open-meteo, Yahoo Finance, Hacker News, OpenStreetMap, the YouTube
IFrame API) — the LLM writes the *layout and the data bindings*, never the numbers.
When a request spans domains that no app covers, the AMA **composes a new app on
the fly** — writing its spec into memory and spinning up a fresh peer agent to
build it.

<p align="center"><em>You type "TSLA" → the AMA routes <code>stock</code> → the stock
agent takes the screen with a live quote + chart. You type "Shanghai" → the weather
agent takes over. You ask for "weather plus today's headlines in one card" → no app
covers that, so the AMA <b>writes</b> a <code>weather-news</code> app and a peer agent
builds it. One OS, many app agents, one routing brain that can grow the app set.</em></p>

## What's here

```
octos-one/
  app/          The Android client (Makepad + Rust). The AMA (router + composer),
                the multi-agent routing (decision → activation → composition), the
                Splash card renderer + post-generation validator, and the shared
                WebView overlay that hosts webview (runhtml) cards.
  a2app/        The "app-card memory" — the ONLY thing that teaches an agent an app.
                Requirements-only specs, reusable widget patterns, live-data helper
                docs, a global design system, and per-app lint rules. NO exemplars:
                every app is ASSEMBLED from the widget patterns, not copied from a
                template. octos assembles this tree at inject time (deployed as
                `app-cards/` under the profile memory dir) — no build step, no artifact.
  tools/        llm-qr/ — Rust dev tool: encode an LLM config as a QR to scan.
  docs/
    ARCHITECTURE.md            How it all fits together (read this first).
    ADDING-AN-APP-CARD.md      Add a new app type end-to-end (e.g. crypto, sports).
    BUILDING-ANDROID.md        Build the APK + deploy + run on a device.
    PROVISIONING-LLM.md        Bring-your-own-key: encode an LLM config as a QR.
    WEBVIEW-AGENT.md           The webview (runhtml) card pipeline, end to end.
    OCTOS-WIDGETS.md           octos.* web-widget kit API (the web `glass.*`).
    APP-CARD-COMPONENTIZATION.md  Layered, data-bound, cross-substrate components.
    weather-styles/            The 4 selectable weather styles + real-glass cards.
```

## Dependent projects (referenced, not vendored)

The app compiles against a Makepad fork and is built with the `cargo-makepad`
tool. These are large and live in their own repos:

| Dependency | Repo / branch | Why |
|---|---|---|
| **Framework fork** (the Splash engine + `sys.*` live-data helpers, the vendored plot widget, Android JNI) | [`octos-org/makepad`](https://github.com/octos-org/makepad) branch **`octos-one-framework`** | `app/` path-deps `../aichat` — this is that crate tree. |
| **Build tool** (`cargo-makepad`, native composer Java) | [`octos-org/makepad`](https://github.com/octos-org/makepad) branch **`octos-one-buildtool`** | Builds/signs the APK; bakes the Android SDK/NDK. |
| **octos kernel** (`liboctos.so serve --stdio`) | [`octos-org/octos`](https://github.com/octos-org/octos) | The agent runtime, bundled into the APK. |

See **[docs/BUILDING-ANDROID.md](docs/BUILDING-ANDROID.md)** for exactly where to
clone each and how to build.

## The idea in one diagram

```
 user intent ─▶ AMA (router + composer)
                    │  reads the injected routing list of apps
                    │
        ┌───────────┴────────── does an app cover this intent? ──────────┐
        ▼ yes                                                            ▼ no (multi-domain)
   route_to_app(id)                                          AMA COMPOSES a new app:
        │ activate + foreground                              writes apps/<a>-<b>/app.md
        ▼                                                     + lint.json into memory,
  ┌─────────────┬─────────────┬──────────┬──────────────┐    replies `compose <a>-<b>`
  ▼             ▼             ▼          ▼              ▼           │
weather      stock          news     activity   weather-activity  ▼
agent        agent          agent    agent      agent      spawn a NEW peer agent
  │            │              │         │           │       (fresh session injects
  └── runsplash DSL ── sys.weather / sys.stock / sys.news / sys.places ─┘  the new spec)
                              │                                            │
                   live fetch at render                                   ▼
                              ▼                                    it builds the app
                    full-screen live card  ◀── card validator (lint → one-shot repair)
```

Each app agent is its own octos session (dedicated context); the AMA's decision
picks which one takes the screen — or authors a new one. A composed app persists
as ordinary app-card memory, so the next matching request routes to it directly.
See **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)**.

## Two card substrates

A card is emitted as a fenced block; the substrate is chosen per card:

- **Splash** (` ```runsplash `) — a native Makepad widget tree. Fast, GPU-rendered,
  and the home of effects the web can't cheaply match: shader-animated weather
  icons and **real glass** (`glass.Panel` = gaussian backdrop blur + lensing).
  Splash cards can load bundled fonts (e.g. Roboto weights) via `crate_resource`.
- **Webview** (` ```runhtml `) — an HTML/JS document in a shared native WebView
  overlay (`octos_web_card`). All cards share one origin, so `localStorage` state
  composes across them. This is how the **YouTube** card runs a real IFrame-API
  player (captions, translate, PiP, swipe-to-minimize) — see *Building the YouTube
  card* below.

**The `octos.*` web-widget kit** is the web counterpart of Splash's `glass.*`:
`aichat/widgets/src/octos_{core,media,finance,weather}.js`, auto-injected into
every runhtml card. Cards **compose** from it instead of hand-rolling — including
**data-bound components** that fetch their own live data and render in one call:
`octos.stock("AAPL")`, `octos.forecast("Tokyo")`. See
**[docs/OCTOS-WIDGETS.md](docs/OCTOS-WIDGETS.md)**.

## Building the YouTube card (a webview app-card walkthrough)

The YouTube agent is the worked example of a *rich* webview card — a real
IFrame-API player with captions, translate, PiP, and swipe-to-minimize, composed
from the `octos.media` kit. How it's built, end to end:

1. **Write the contract** (`a2app/apps/youtube/app.md`) — the spec, *no exemplar*:
   it names the screens (home / watch / search / library), the components to
   compose (`octos.player`, `octos.tile`, `octos.actionBar`, `octos.channelRow`,
   `octos.comments`, …), and the rules (bind live data, emit exactly one `runhtml`
   block, never truncate). Contract-only means the LLM composes from the kit — no
   code to copy-paste.

2. **Wire the agent** — the contract is baked into the binary and injected into the
   youtube agent's prompt (`YOUTUBE_CARD_CONTRACT` + the `domain == "youtube"`
   branch in `app/app/src/main.rs`). The AMA routes any video / music / live-stream
   intent ("play despacito", "lofi", "watch news live", "放点音乐") to it.

3. **Compose the card** — the agent emits ONE `runhtml` document composing the
   `octos.media` widgets over the **YouTube IFrame Player API**: `octos.player`
   mounts the player and drives `.captions({on,lang,size})` (translate to ~30
   languages), `.mini(bool)` (square PiP), and swipe-down-to-minimize gestures.
   The kit is framework-injected, so the card writes *layout + which widgets +
   data*, not the player plumbing.

4. **Ground the live data** — live video ids rotate, so the app runtime resolves
   the current ids just before generation (`refresh_youtube_live_ids()` fetches
   `youtube.com/@handle/live`) and injects them into the prompt; the card pulls
   titles via keyless oEmbed (noembed.com). No hardcoded ids.

5. **Render** — the `runhtml` block loads into the shared WebView overlay
   (`octos_web_card`) against `https://octos-one.app/`, so playback + `localStorage`
   (history, subscriptions) persist across cards.

Deep dive: **[docs/WEBVIEW-AGENT.md](docs/WEBVIEW-AGENT.md)** (the pipeline),
**[docs/YOUTUBE-CARD-COMPOSITION.md](docs/YOUTUBE-CARD-COMPOSITION.md)** (splitting
the monolith into per-screen cards), **[docs/OCTOS-WIDGETS.md](docs/OCTOS-WIDGETS.md)**
(the `octos.media` API), and the reference card
[`docs/youtube-player-reference.html`](docs/youtube-player-reference.html).

## Selectable card styles (weather)

The weather card ships **four styles** — `dark` · `immersive` (photo) · `light` ·
`glass` — selected by request keyword (`glass weather tokyo`, `dark weather`, …);
same live `sys.weather` data, different skin. The **glass** style uses real
`glass.Panel` blur with the tint matched to the sky (gray = overcast/fog, blue =
clear, slate = rain …), and a city list taps through to an instant, LLM-free glass
detail. See **[docs/weather-styles/](docs/weather-styles/README.md)**.

## What works (verified on-device, OnePlus 6/6T)

- **Built-in apps** — weather (immersive photo card: conditions, 7-day forecast,
  satellite + air-quality maps, detail grid), stock (top-movers list → tap →
  detail with a real line/area **chart** and client-side range switching), news
  (Hacker News list → tap → detail), activity (nearby places), the composed
  **weather-activity**, plus the webview **YouTube** app. Data is live and matches
  the source APIs to the cent/point.
- **Two substrates.** Native **Splash** cards (GPU, shader weather icons, real
  `glass.Panel` blur, bundled fonts) and **webview** (runhtml) cards in a shared
  WebView overlay + the auto-injected `octos.*` kit — see *Two card substrates*
  and *Building the YouTube card* above.
- **Weather styles.** Four selectable styles (dark / immersive / light / glass)
  chosen by request keyword; real-glass detail cards with the tint matched to the
  sky, and a city list that taps through to an instant, LLM-free glass detail.
- **Assembled, not templated.** Every card is generated by the on-device model
  (glm-5.2) from a requirements spec + shared widget patterns — no full-card
  exemplars in memory. The model composes novel apps from the same pieces.
- **Dynamic composition.** A multi-domain intent no app covers makes the AMA
  author a new app spec (merging the parents' named design blocks, inheriting the
  primary parent's visual identity) into the app-cards tree; a fresh peer agent
  then builds it, and it persists for future requests.
- **A self-correcting pipeline.** Each app ships machine-checkable `lint.json`
  rules; a completed card that violates them triggers one automatic repair turn.
- **Live-data plane.** `sys.weather`/`sys.airquality`/`sys.stock`/`sys.stockbar`/
  `sys.stockrange`/`sys.movers`/`sys.news`/`sys.places` (+ numeric `sys.weathernum`/
  `sys.aqinum` so cards can *branch* on live values), plus a vendored **StockPlot**
  chart widget — all sharing one deduped fetch cache with bounded retries.
- **Guardrails.** A security gate refuses cards that use the low-level `net.*`
  API (cards may only read via `sys.*` + image `http_resource`); the AMA's
  spec-authoring is confined to the `apps/` subtree.

The AMA routes weather / stock / news / activity / youtube / web correctly in
English and Chinese (including `<style> weather <place>` → weather) and composes
new apps for combined requests.
