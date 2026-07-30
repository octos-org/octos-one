# How octos-one generates apps: a2app + Splash + app cards

> Investigation notes (2026-07-23), synthesized from the code/docs at `main @ fb790c3`
> (aichat `dff28c40`, makepad `905d7615`, octos `594c2052`) and verified live on-device
> (OnePlus 6 / API 28). File:line references are from that tree. Untracked working
> notes — complements `docs/ARCHITECTURE.md` and `docs/ADDING-AN-APP-CARD.md`.

## 0. The core idea

octos-one is an **agent-OS phone client**: there is no hand-coded weather app, stock
app, or news app. There is one Makepad shell, an embedded octos kernel
(`liboctos.so serve --stdio`), and an LLM that **generates each app's UI on demand**
as a full-screen "app card." An app in this system is not code — it's:

1. a **requirements spec** (`a2app/apps/<id>/app.md`) telling the LLM what to assemble,
2. a **shared widget vocabulary** (`a2app/widgets/*.md`) it assembles from,
3. a **machine lint** (`a2app/apps/<id>/lint.json`) validating what came out, and
4. the **Splash runtime** that renders the emitted DSL as live, data-bound native UI.

The philosophy is *assembly, not exemplars*: "YOU generate this card by ASSEMBLING
the widget patterns — there is no exemplar to copy." (Weather's style skins and
nav's trip-planner are the two deliberate exceptions, shipped as `.splash` exemplars.)

## 1. The a2app tree — the source of truth

```
a2app/
  framework.md               ← global router (app list + triggers) + AMA-composer procedure + security rules
  framework/splash-manual.md ← the full Splash DSL reference (~86 KB)
  widgets/
    design-system.md         ← enforced stylesheet: color tokens, type scale, spacing
    interaction.md           ← state, tap→state, chips, Splash-local nav, closure rules
    containers.md            ← View/SolidView/RoundedView/GradientYView/Filler/Image/Label
    weather-icon.md          ← the animated WeatherIcon shader widget
    sys-helpers.md           ← every sys.* live-data helper + StockPlot
  apps/
    weather/   app.md + lint.json + exemplars/style-{light,glass,dark,immersive}.splash
    stock/     app.md + lint.json          ← the canonical "house style" spec
    news/      app.md + lint.json
    activity/  app.md + lint.json
    weather-activity/ app.md + lint.json   ← the canonical COMPOSED app
    nav/       app.md + exemplars/trip-planner.splash
    youtube/   app.md                      ← HTML contract (runhtml)
    web/       app.md                      ← HTML contract (runhtml), open-domain generalist
```

### How specs reach the LLM (three delivery paths)

- **Path A (designed, currently inert):** the octos kernel assembles the tree into
  injected agent memory (`octos-memory::assemble_app_cards`; replaced the removed
  `scripts/build_memory.py`, commit `83aa06b`). The octos main this branch builds
  against no longer injects app-cards as memory (`app/app/src/main.rs:422-426`).
- **Path B (the WORKING path, commit `844e87f`):** every spec + widget doc is
  **baked into the binary** via `include_str!` (`main.rs:448-475`), overridable by a
  deployed on-device tree, and **inlined into the generation prompt** by
  `app_card_docs(domain)` + `splash_gen_prompt()` (`main.rs:486-540`, call site
  `:615-625`). Self-contained — the agent is told "do NOT read or fetch files."
- **Path C (supporting):** the a2app dir is exported as a skill **read-zone**
  (`OCTOS_SKILLS_PATH`, `main.rs:4944-4951`) so a sub-agent's `read_file` can reach
  specs by absolute path.

## 2. The Splash language (the "CardDSL")

A card is one fenced block — the LLM's *entire answer*:

````
```runsplash
// name: weather-app
SolidView{ width: Fill height: 1400 flow: Overlay new_batch: true draw_bg.color: #2a3d66
  GradientYView{ width: Fill height: Fill draw_bg.color: #3a7bd5 draw_bg.color_2: #3a1c71 }
  View{ flow: Down padding: Inset{left:16 top:56 right:16 bottom:20} spacing: 14
    Label{ text: "Tokyo" draw_text.color: #ffffff draw_text.text_style.font_size: 24 }
    Label{ text: sys.weather(35.68, 139.65, "current.temperature_2m") + "°" ... font_size: 88 }
    WeatherIcon{ draw_bg.cond: 1.0 width: 30 height: 30 }
    Label{ text: "H:" + sys.weather(35.68, 139.65, "daily.temperature_2m_max.0") + "°" ... }
    ...
```
````

(Verbatim shape from the shipped glass exemplar, `docs/weather-styles/style-glass.splash`.)

Key language facts:

- **It's Makepad's live-design DSL as a script**: a widget tree (`View`, `SolidView`,
  `RoundedView`, `GradientYView`, `Label`, `Image`, `Button`, `TextInput`,
  `ScrollYView`, custom `WeatherIcon`, `StockPlot`, `MapView`, …) with layout props
  (`width/height: Fill|Fit|px`, `flow: Down|Right|Overlay`, `align`,
  `padding: Inset{…}`) and a deliberately tiny style surface
  (`draw_bg.color/color_2/border_radius/shadow_*`, `draw_text.color`, `font_size`).
- **Two body modes** (`aichat/widgets/src/splash.rs:2118-2137`): *view-children*
  (starts with widgets — the weather shape) and *full-script* (starts with
  `let`/`fn` — stateful cards like stock and news: `let state = {…}`,
  `fn show_story(i)`, named widgets `id := Widget{}` mutated via
  `ui.<id>.set_text(...)`).
- **Interactivity**: either Splash-local (`fn` helpers + named-widget mutation —
  news) or via the agent (`on_click: || agent.notify("set", {key:"selected",
  value:…})` + `{{state.*}}` template slots re-resolved by the host — stock).
  Closures must be expression-form (`|| f(x)`); the block form silently never fires.
- **Each card runs in its own isolated VM** with a 1,000,000-instruction cap,
  streaming-incremental parsing, and a corrupt-card brace guard
  (`splash.rs:2231-2352`).

## 3. `sys.*` — the grounded live-data layer

All helpers live in `register_agent_module()`
(`aichat/widgets/src/splash.rs:119-1223`); each builds a **keyless public API URL**,
does a URL-deduped cached fetch, and dot-path-plucks the JSON (`json_pluck`,
`splash.rs:1378`).

| Helper | Source |
|---|---|
| `sys.weather / weathernum`, `sys.airquality / aqinum` | open-meteo forecast + air-quality |
| `sys.stock`, `sys.stockbar/stockrange`, `sys.movers` | Yahoo Finance quote/chart/day_gainers |
| `sys.news(index, key)` | Hacker News front page (Algolia) |
| `sys.places/placesnum`, `sys.search`, `sys.geocode(num)` | OpenStreetMap Overpass / open-meteo geocoding |
| `sys.photo(query)` | pollinations.ai AI image |
| `sys.satellite(_ir)`, `sys.basemap`, `sys.airmap`, `sys.maptile/mappin` | NASA GIBS, Carto tiles, WAQI AQI overlay |
| `sys.route/navroute/navstep…`, `sys.navsecs/simsecs` | OSRM routing + sim clock |

**Binding model — poll-and-re-eval, not push:** first eval returns `"—"` (or
`-9999` for numeric twins) while fetches are in flight; each landed response bumps a
global `DATA_FETCH_EPOCH` and triggers redraw; the Splash widget's per-frame pump
notices the epoch change and **re-runs the whole card script**, which now finds the
loaded bytes (`splash.rs:2552-2596`, `aichat/platform/src/script/res.rs:566-593`,
`std.rs:217-264`). Images go the parallel `http_resource` texture path
(the `[IMGTRACE]` log lines). Numeric twins let cards **branch on live data**
(`if sys.weathernum(...) > 18 { outdoor } else { indoor }` — weather-activity).
Exception: `fn tick()` cards (nav) are suppressed from epoch re-eval (a rebuild
would tear down `MapView`) and push data in place instead.

**Security invariant** (framework.md:93-98, enforced in code): a card may touch the
network *only* through `sys.*` and `http_resource` GET images — never
`net.http_request`.

## 4. The generation pipeline: prompt → card

All in `app/app/src/main.rs` unless noted:

1. **Boot:** `clear_chat` (`:5197`) creates **7 sessions on one kernel agent** — the
   AMA (router brain, `AMA_SYSTEM_PROMPT` at `:63`) + 6 domain app agents
   (weather/stock/news/web/youtube/nav). "Concurrent" is logical multiplexing: one
   tokio loop, one stdio transport; sessions are HashMap rows demuxed by
   turn/prompt ids (`backend/octos_ui.rs:585`); only **one turn is ever in flight**
   (submit guard `:5492` — "submit ignored: a turn is still in flight").
2. **Submit → HOLD:** `submit_prompt` (`:5472`) sends the intent to the **AMA only**
   and stashes the raw text in `pending_intent` (`:5540-5559`). The AMA is forbidden
   to emit UI; it must answer exactly one line: `<app-id> — <reason>`,
   `none — <reason>`, or `compose <a>-<b> — <reason>`. Its stream is captured into
   `ama_text` (`:7534`) and never rendered.
3. **Decision → activation** (`route_to_app:4442`, `parse_ama_decision:4563`):
   - `weather — bare place name` → log `AMA → activate 'weather' app agent (idx 0)`,
     foreground that agent, dispatch the held intent wrapped in a
     domain-specialized generation prompt (`splash_gen_prompt` = Splash manual +
     widget docs + the app's `app.md`, all inlined).
   - `none — greeting` → clear streaming, render nothing.
   - `compose <new-id>` → AMA-composer path (`compose_app:4617`): write a fresh
     `apps/<id>/app.md` into the write-zone, spawn a new peer session.
   - Special cases: **youtube** (`:4464`) and **nav** (`:4492`) are served
     **directly** (reference `runhtml` card with live-resolved video ids / canonical
     nav `runsplash`) — no LLM, because the ~14 KB apps exceed what the phone-routed
     model reliably generates.
4. **Generation:** the domain agent emits exactly one ```` ```runsplash ```` block
   per its spec, streaming into the card surface; a card is persisted the moment
   its closing fence arrives (`save_completed_stream_cards:1198`).
5. **Gates:** on turn complete (`:7690-7846`) the app prefers the kernel's
   **persisted text** over the delta stream (lost-delta protection —
   "stream/persisted MISMATCH" log), then:
   - **Security gate** (`runsplash_body_forbidden:1411`,
     `neutralize_forbidden_cards:1431`): normalization-resistant scan for
     `net.http_request`/sockets → unsafe cards are never saved; the rendered block
     is replaced with a "⚠ card was blocked" notice.
   - **Card lint** (`app/app/src/app/card_lint.rs` — `load_rules:28`, `lint:74`):
     substring-count rules from the **owning app's** `lint.json` (never the
     foreground app's — a stock card must not be judged by weather rules; commit
     `547959e`). Orphan prompts skip lint.
   - **One-shot auto-repair** (`:7857-7872`): any security/lint failure sends a
     single repair prompt back to the same session ("Card failed validation —
     auto-repairing…"), capped at one retry (`repair_attempted`).
6. **Archive** (commit `c0fd6bb`): every card is saved to the on-device
   **`$HOME/a2app_cards/` store** (`save_card_artifact:1147`) — `<name>.splash`
   (latest revision, keyed by the mandatory `// name:` first line) +
   `<name>.meta.json` + an append-only **`index.jsonl` revision ledger**; saved
   cards are re-injected into later prompts as "YOUR SAVED CARDS" for refinement
   (`load_a2app_cards:1252`). Debug: the DSL is dumped to logcat in 600-byte chunks
   as `CARDDSL[i]…` (`:7763-7765`).
7. **Render:** the Markdown widget sees the fence tag and feeds the body to the
   `Splash` widget (`aichat/widgets/src/markdown.rs:1277-1329`) → isolated VM eval
   (`splash.rs:2231`) → live Makepad widget tree in the GL surface, newest card
   full-screen, `{{state.*}}` slots resolved (`resolve_a2app_card`), render cache
   keyed by `(item, text, state)`.

## 5. Two substrates: `runsplash` vs `runhtml`

| | `runsplash` (Splash native) | `runhtml` (WebView) |
|---|---|---|
| Renderer | Makepad GPU widget tree in the shared GL surface (`splash.rs`) | Real `WebView` overlay pinned over the GL surface (`web_card.rs`) |
| Data | native `sys.*` + epoch re-eval | JS `fetch` + the injected **`octos.*` JS kit** (web twins: `octos.stock`, `octos.forecast`, `octos.player`…; `web_card.rs:37-44`) |
| Used by | weather, stock, news, activity, nav, composed apps | youtube, web (things needing a mature web engine) |

Same `Widget` interface, "two physics" (`docs/SPLASH-NATIVE-INTEGRATION.md:248-287`).
The WebView occlusion issues (video hardware surface over HTML controls; the
composer over card bottoms) live on the `runhtml` side of this split.

## 6. The three worked examples

**Weather** (`a2app/apps/weather/app.md`): immersive full-screen iOS-style card.
`// name: weather-app`; photo backdrop `sys.photo("<city> <weather>")` under a
scrim; hero temp `sys.weather(LAT,LON,"current.temperature_2m")+"°"` at font 60-88;
animated `WeatherIcon{draw_bg.cond:N}`; H/L/Feels line; a 7-row forecast strip
binding `daily.temperature_2m_max/min.N` for N=0..6; **two live map panes** (NASA
satellite + WAQI air-quality over a Carto basemap); 6 frosted detail tiles
(AQI/UV/sunrise/sunset/humidity/wind). Keyword-selected skins
(dark/minimal/glass/photo). Lint: ≥22 `sys.weather(` calls, ≥7 forecast rows, all
four image layers.

**Stock** (`a2app/apps/stock/app.md`): one card containing **both** a top-gainers
list *and* per-ticker detail with client-side navigation — a full-script body
branching on `{{state.selected}}`. List: 10 tappable rows of
`sys.movers(i, "symbol"/"price"/"changepct")`; each tap does
`agent.notify("set",{key:"selected",…})`. Detail: back button, live quote
`sys.stock(sel,…)`, a native `StockPlot{symbol range}` chart, `1D/1W/1M/6M/1Y`
range chips writing `{{state.range}}`, and a 3×2 stat grid. Lint: ≥30
`sys.movers(`, ≥11 `key:"selected"`, ≥15 `on_click`.

**News** (`a2app/apps/news/app.md`): dark iOS-News-style Hacker News reader.
Full-script with `let news_app = {detail:false selected:0}` and **Splash-local
navigation** — `fn show_story(i)` / `fn show_list()` mutate named widgets in place
(no `agent.notify` at all). Fixed masthead + lead card + a `ScrollYView` of 7
`StoryRow` templates, everything bound via
`sys.news(i,"title"/"author"/"points"/"comments"/"url")`. Lint: ≥7 `StoryRow{`,
≥20 `sys.news(`.

**Composition showcase — weather-activity** (`a2app/apps/weather-activity/app.md`):
weather's `BLOCK: CURRENT` on top, then a *live* branch — `sys.weathernum` /
`sys.aqinum` thresholds (18°, AQI 100, rain 40%) choosing OUTDOOR vs INDOOR
`sys.places` rows, with a verdict line citing the actual numbers. It's the template
the AMA composer imitates when it invents a brand-new app (framework.md:43-90).

## 7. One-sentence summary

The a2app tree turns "an app" into a promptable spec; the AMA turns an utterance
into a routing decision; a domain agent turns the spec + `sys.*` vocabulary into a
`runsplash` program; and the Splash VM turns that program into live,
self-refreshing, real-data native UI — with lint + a security gate + a revision
ledger making the whole loop safe and repeatable.

## 8. File:line reference index

| Concern | Location |
|---|---|
| AMA + router constants | `app/app/src/main.rs:63` (`AMA_SYSTEM_PROMPT`), `:65` (`APP_SPLASH_ROUTER`) |
| Baked specs / prompt inlining | `main.rs:448-475`, `486-515` (`app_card_docs`), `522-540` (`splash_gen_prompt`) |
| Card name / save / archive | `main.rs:1101` (`extract_card_name`), `1147` (`save_card_artifact`), `1252` (`load_a2app_cards`) |
| Security gate | `main.rs:1372` (normalize), `1411` (`runsplash_body_forbidden`), `1431` (neutralize) |
| Routing | `main.rs:4442` (`route_to_app`), `4563` (`parse_ama_decision`), `4617` (`compose_app`) |
| Boot sessions | `main.rs:5197` (`clear_chat`), `backend/octos_ui.rs:585` (`create_session`) |
| Submit / hold | `main.rs:5472` (`submit_prompt`), `5492` (serialization guard) |
| Turn-complete handling | `main.rs:7608` (AMA), `7690` (app agent), `7763` (CARDDSL dump) |
| Card lint | `app/app/src/app/card_lint.rs:28,74,89` |
| Splash widget / sys.* | `aichat/widgets/src/splash.rs:119-1223` (helpers), `2118` (body modes), `2231` (`eval_body`), `2552` (epoch pump) |
| Fetch engine | `aichat/platform/src/script/res.rs:566-593`, `std.rs:217-264` |
| Fence dispatch | `aichat/widgets/src/markdown.rs:1277-1329` |
| WebCard / octos.* kit | `aichat/widgets/src/web_card.rs:37-44,329` |
| Docs | `docs/ARCHITECTURE.md`, `docs/ADDING-AN-APP-CARD.md`, `docs/SPLASH-NATIVE-INTEGRATION.md`, `docs/OCTOS-WIDGETS.md`, `docs/AMA-SESSION-MULTIPLEXING.md`, `aichat/splash.md` |
