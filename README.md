# octos-one

An **agent-OS phone client**: a native Android app where a routing brain (the
**AMA**) dispatches every request to one of several concurrent **app agents**
(weather, stock, news, youtube, web), each of which generates a live, full-screen
interactive card. Cards come in **two substrates** — native **Splash DSL** cards
and **webview** (HTML/JS) cards — and both bind **real data at render time**
(open-meteo, Yahoo Finance, Hacker News, the YouTube IFrame API) — the LLM writes
the *layout and the data bindings*, never the numbers.

<p align="center"><em>You type "TSLA" → the AMA routes <code>stock</code> → the stock
agent takes the screen with a live quote. You type "Shanghai" → the weather agent
takes over. One OS, many app agents, one routing brain.</em></p>

## What's here

```
octos-one/
  app/          The Android client (Makepad + Rust). The AMA, the multi-agent
                routing (decision → activation), the Splash card renderer, and the
                shared WebView overlay that hosts webview (runhtml) cards.
  a2app/        The "app-card memory": the framework rules, widget docs, and one
                spec + exemplar per app (weather / stock / news / youtube / web).
                Assembled into the MEMORY.md that octos injects into each agent.
  scripts/      build_memory.py — assemble a2app/ → MEMORY.md (reproducible).
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
| **Framework fork** (the Splash engine + `sys.*` live-data helpers, Android JNI) | [`octos-org/makepad`](https://github.com/octos-org/makepad) branch **`octos-one-framework`** | `app/` path-deps `../aichat` — this is that crate tree. |
| **Build tool** (`cargo-makepad`, native composer Java) | [`octos-org/makepad`](https://github.com/octos-org/makepad) branch **`octos-one-buildtool`** | Builds/signs the APK; bakes the Android SDK/NDK. |
| **octos kernel** (`liboctos.so serve --stdio`) | [`octos-org/octos`](https://github.com/octos-org/octos) | The agent runtime, bundled into the APK. |

See **[docs/BUILDING-ANDROID.md](docs/BUILDING-ANDROID.md)** for exactly where to
clone each and how to build.

## The idea in one diagram

```
 user intent ─▶ AMA (router)  ── classifies domain ──▶  route_to_app()
                                                            │ activate + foreground
                        ┌───────────────┬───────────────────┴──────────┐
                        ▼               ▼                              ▼
                  weather agent    stock agent                    news agent
                  (own session)    (own session)                  (own session)
                        │               │                              │
                  runsplash DSL ── sys.weather/stock/news ─▶ live fetch at render
                        └───────────────┴──────────────────────────────┘
                                         ▼
                              full-screen live card
```

Each app agent is its own octos session (dedicated context); the AMA's decision
picks which one takes the screen. See **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)**.

## Two card substrates

A card is emitted as a fenced block; the substrate is chosen per card:

- **Splash** (` ```runsplash `) — a native Makepad widget tree. Fast, GPU-rendered,
  and the home of effects the web can't cheaply match: shader-animated weather
  icons and **real glass** (`glass.Panel` = gaussian backdrop blur + lensing).
  Splash cards can load bundled fonts (e.g. Roboto weights) via `crate_resource`.
- **Webview** (` ```runhtml `) — an HTML/JS document in a shared native WebView
  overlay (`octos_web_card`). All cards share one origin, so `localStorage` state
  composes across them. This is how the **YouTube** card runs a real IFrame-API
  player (captions, translate, PiP, swipe-to-minimize).

**The `octos.*` web-widget kit** is the web counterpart of Splash's `glass.*`:
`aichat/widgets/src/octos_{core,media,finance,weather}.js`, auto-injected into
every runhtml card. Cards **compose** from it instead of hand-rolling — including
**data-bound components** that fetch their own live data and render in one call:
`octos.stock("AAPL")`, `octos.forecast("Tokyo")`. See
**[docs/OCTOS-WIDGETS.md](docs/OCTOS-WIDGETS.md)**.

## Selectable card styles (weather)

The weather card ships **four styles** — `dark` · `immersive` (photo) · `light` ·
`glass` — selected by request keyword (`glass weather tokyo`, `dark weather`, …);
same live `sys.weather` data, different skin. The **glass** style uses real
`glass.Panel` blur with the tint matched to the sky (gray = overcast/fog, blue =
clear, slate = rain …), and a city list taps through to an instant, LLM-free glass
detail. See **[docs/weather-styles/](docs/weather-styles/README.md)**.

## Status

**Splash app cards** — weather, stock, and news are implemented and verified
on-device (OnePlus 6/6T); data is live (values match the source APIs to the
cent/point). Weather has 4 selectable styles + real-glass detail cards with
condition-matched tints and tap-to-detail navigation.

**Webview app cards** — the runhtml pipeline (WebCard widget + cross-platform
`SystemBrowser` `set_html` + shared WebView overlay + auto-injected `octos.*` kit)
is implemented; the **YouTube** agent runs a full IFrame-API player card on-device.

The AMA routes weather / stock / news / youtube / web correctly in English and
Chinese (including `<style> weather <place>` → weather) and activates the matching
agent.
