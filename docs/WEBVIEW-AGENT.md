# Webview app agent — markdown-only app cards (design)

> **STATUS (2026-07-17): implemented and demonstrated on the OnePlus 6T.**
> The ```runhtml pipeline (markdown fence → `WebCard` widget → SystemBrowser
> ops → WKWebView overlay on macOS / android.webkit.WebView overlay on Android
> via `loadDataWithBaseURL`) is built, the `web` domain agent is routed in the
> app, `a2app/apps/web/app.md` (contract-only, no exemplar) is written, and a
> seeded web card plays a **live YouTube stream** on the 6T. See §8 for the
> device findings uncovered on the way — including a **software-rendering
> manifest bug that affected every octos-one build** (fixed).

**Goal.** Add a **web app agent** whose cards are HTML/CSS/JS rendered in a native
WebView — defined by the app-card principle taken to its logical end: **no
exemplar code at all, only a markdown contract**, letting the LLM *compose*
arbitrary apps (todo, timer, calculator, dashboards, mini-games, …) on demand.

This is the counterpart to the Splash cards: Splash agents are **domain-locked
specialists** (weather/stock/news) taught by spec + exemplar; the web agent is the
**open-domain generalist** taught by contract only.

---

## 1. Why markdown-only works here (and not for Splash)

The existing docs call the exemplar "the highest-leverage file" — true for
Splash, because Splash is a low-resource DSL the base model has never seen; the
exemplar substitutes for missing training data, and each one costs a large slice
of the `max_inject_tokens` budget (the ⚠️ in ARCHITECTURE §3).

HTML/CSS/JS is the opposite: it is the **highest-resource UI language in any
model's training data**. The model doesn't need to be shown what a working app
looks like — it needs to be told the *contract*: how the card mounts, sizes,
fetches, persists, and talks to the agent shell. A contract is markdown. Hence:

| | Splash apps (today) | Web app (this design) |
|---|---|---|
| App knowledge | spec **+ canonical exemplar** | **contract md only** |
| Domain | one per app (closed set) | open — AMA fallback route |
| New data source | new native `sys.*` helper → **rebuild** | `fetch()` in JS → **no rebuild** |
| Memory cost | large (exemplars dominate MEMORY.md) | ~1–2 K tokens once |
| Composability | fixed card layout per domain | LLM composes app from web primitives |

The "two extension axes" collapse: for web cards, *both* the app package and the
data capability are content. The only native work is the **one-time web runtime**
below.

---

## 2. What already exists in the framework fork (verified)

- **Platform op trio** `CxOsOp::CreateWebView { id, area, texture, url } /
  UpdateWebView / CloseWebView` (`platform/src/cx_api.rs:391`).
- **Apple implementation** (`platform/src/os/apple/apple_webview.rs`, 396 lines):
  a native **WKWebView overlay** added as a subview above the Makepad GL view at
  the op's `area` rect (the `texture` field is currently unused — overlay, not
  composition). macOS + iOS.
- **A dormant widget slot**: `widgets/src/lib.rs:128` has `// pub mod web_view;`
  commented out — the widget was planned, never landed.
- **The fence dispatch extension point**: `widgets/src/markdown.rs` (~line 1263)
  dispatches fenced blocks by language — ` ```runsplash ` → `splash_block`
  template, ` ```mermaid `, ` ```diagram `. A ` ```runhtml ` hook slots in
  identically, including the streaming-accumulate path splash uses.
- **Missing piece: Android.** `platform/src/os/linux/android/` has **no webview
  implementation** — and Android is the primary octos-one target. This is the
  main framework gap.

---

## 3. Framework work (one-time native "web runtime", rebuild required)

Think of this as the web equivalent of the `sys.*` standard library: built once,
then every future app is pure content.

### 3.1 Android WebView backend for the existing op trio

Mirror the Apple overlay pattern with `android.webkit.WebView`:

- JNI through the cargo-makepad composer Java (buildtool branch): on
  `CreateWebView`, instantiate a `WebView` on the UI thread and add it to the
  Activity's content view **above** the Makepad `SurfaceView`, positioned/sized
  from the op's `area` (device-pixel rect, same math as the Apple impl).
- `UpdateWebView` repositions on relayout/scroll; `CloseWebView` detaches.
- Settings: `javaScriptEnabled=true`, `domStorageEnabled=true` (for
  `localStorage` state), `allowFileAccess=false`, block external navigation in
  `shouldOverrideUrlLoading` (deny, or hand off to the system browser).

### 3.2 Content loading: HTML string, not URL

The op today carries `url: String`. LLM cards are documents, not addresses — add
a variant/field for inline HTML and load with `loadDataWithBaseURL(null, html,
"text/html", "utf-8", null)` (Android) / `loadHTMLString:` (WebKit). A `data:`
URL also works short-term but hits size and encoding edges; a real `html` field
is cleaner.

### 3.3 The `WebView` widget (`widgets/src/web_view.rs`, un-comment the slot)

A small widget that walks its rect and drives the op trio:

- `set_html(html)` — (re)load content; keep the id stable per card **name** so a
  refined card reuses its WebView (state survives, no flash).
- Emits `Create` on first draw, `Update` on rect change (scroll/resize/
  foreground switch), `Close` on drop and on background (overlays float above
  the GL scene, so a backgrounded app's webview must be closed or hidden — same
  reason the foreground guard exists for `CHAT_DATA`).
- While the card is still streaming: draw a skeleton card ("composing app…")
  and do **not** load partial HTML; load once the fence closes.

### 3.4 Markdown hook: ` ```runhtml ` → `webview_block`

Copy the `runsplash` dispatch: accumulate the fence body, hand the completed
string to the `webview_block` template (the WebView widget). ~30 lines by
symmetry with `splash_block`.

### 3.5 The JS bridge (the `agent.notify` / `{{state.*}}` analog)

Expose one tiny object via `addJavascriptInterface` (Android) /
`WKScriptMessageHandler` (Apple), and inject state at load:

```js
window.octos.notify(event, payloadJson)   // → same path as Splash agent.notify
window.octos.state                        // injected snapshot of a2app_state[card]
```

Card-local state that the *agent* doesn't need should just use `localStorage`
(free with `domStorageEnabled`); `octos.notify` is only for round-trips the
shell should see (mirrors the Splash `inc`/`set` events into `a2app_state`,
keyed by card name).

---

## 4. Content + routing work (markdown-only, no rebuild after 3.x)

### 4.1 `a2app/apps/web/app.md` — the contract (NO exemplars directory)

The spec is the whole app definition. Mandatory sections, mirroring the tone of
the existing app.md files but describing a contract instead of a layout:

1. **Output shape** — EXACTLY ONE ` ```runhtml ` fence; first line inside:
   `<!-- name: <kebab-slug> -->` (stable id, reused on refinement — same rule as
   Splash cards).
2. **Document rules** — one complete self-contained HTML document: inline
   `<style>` and `<script>`, no external CDNs/fonts (system font stack), no
   frameworks; works offline except for data `fetch()`es.
3. **Layout rules** — fills the viewport (`margin:0`, `height:100vh` — see the
   web baseline below), top padding `54px` clears the status bar, dark theme by
   default (matches the shell), touch-friendly hit targets (≥44px).
3b. **Web baseline** — target **Chromium ≥ 92** (the fleet's oldest System
   WebView — see §6): flexbox/grid, `vh`/`%`, ES2020 (optional chaining,
   nullish) are fine; do NOT use `dvh`/`svh`, `:has()`, container queries, or
   top-level `await` in inline scripts.
4. **Live data** — the app-card principle restated for JS: *bind, never bake*.
   Use `fetch()` on keyless CORS-irrelevant JSON APIs (WebView has no CORS for
   top-level fetches to permissive APIs; same sources philosophy as `sys.*` —
   open-meteo, Yahoo chart, HN Algolia, CoinGecko…). Render skeletons/`—` while
   loading; visible error fallback on failure. NEVER hardcode a live number.
5. **State** — `localStorage` for persistence across refinements;
   `octos.notify("set"/"inc", {key, value})` only when the shell should know;
   read `octos.state` on load if present.
6. **Composability** — build the app as small view functions over one state
   object; multi-screen apps navigate in-card (tabs / stack in JS). This is what
   replaces per-domain layouts: the LLM composes any app from these primitives.
7. **Hard don'ts** — no `alert/confirm/prompt`, no `window.open`, no external
   navigation, no `eval` of fetched content, degrade gracefully with JS errors
   (wrap init in try/catch and show the error in-card).

Explicitly: **no `exemplars/` folder for this app.** If generation quality
falls short, the fix is a better contract sentence, not a sample app — that
discipline is the experiment this design exists to run.

### 4.2 Routing (small `app/src/main.rs` edits, one rebuild ride-along)

- `clear_chat`: add `AppRecord::with_domain(web, "Web", "web")`.
- `AMA_SYSTEM_PROMPT`: add `web = any app/tool/game/utility request that is not
  weather/stock/news ("make me a todo app", "pomodoro timer", "tip calculator")`
  — and make `web` the **fallback for `none`**: the AMA stops rejecting intents
  and the system becomes open-domain.
- `APP_SPLASH_ROUTER`: for the `web` domain, instruct one ` ```runhtml ` block
  per `apps/web/app.md` (rename-agnostic: the constant routes both card types).
- `a2app/framework.md`: add the `web` app type; note it outputs `runhtml`, not
  `runsplash`.
- `scripts/build_memory.py`: `FILES += a2app/apps/web/app.md` (~1–2 K tokens —
  re-run `--check`).

### 4.3 When to still prefer a Splash card

Splash stays the right tool where it is stronger: GPU-native visuals, the
`sys.*` epoch-driven re-eval, tight integration with shell widgets, and the
curated per-domain layouts. The AMA keeps routing those domains to the
specialists; `web` takes the long tail. (Later, specialists could *fall back*
to web when a request inside their domain doesn't fit their card spec.)

---

## 5. Phased plan

| Phase | Work | Rebuild? | Proves |
|---|---|---|---|
| 0 | ` ```runhtml ` hook + WebView widget on **desktop macOS** (Apple overlay already works) | fw only | contract-only generation quality, bridge design |
| 1 | Android WebView backend (JNI overlay + `loadDataWithBaseURL`) | fw + APK | the real target device |
| 2 | `apps/web/app.md` + AMA `web` fallback route + memory rebuild | app once, then content-only | open-domain composable apps |
| 3 | JS bridge state round-trip (`octos.notify` ↔ `a2app_state`) | fw | refinement flows ("add a dark mode toggle") |

Phase 0 is deliberately first: it needs **zero new platform code** and answers
the central question — *is a markdown contract alone enough for reliable card
generation?* — before any Android work is spent.

## 6. Validation targets (measured 2026-07-17)

Primary device: **OnePlus 6T** (`adb -s bf0a4730`), with a OnePlus 6 as the
second device.

| Fact | OnePlus 6T | Consequence |
|---|---|---|
| Android | 11 (SDK 30) | `addJavascriptInterface`, `loadDataWithBaseURL`, `domStorageEnabled` all fully supported |
| System WebView | Chromium **92.0.4515.131** (com.google.android.webview) | sets the contract's web baseline (§4.1-3b): no `dvh`, `:has()`, container queries |
| Display | 1080×2340 @ 450 dpi | ~2.8x density; the 54 px safe-area inset is in **CSS px** (WebView handles dpi) |

Two notes from prior device testing of the Splash path:

- The **emulator's GL translation layer** silently drops frames that Splash
  cards trigger (physical devices are fine). The WebView overlay path is
  **native, not GL** — so web cards should render correctly *even on the
  emulator*, which conveniently gives the web agent a working emulator story
  the Splash cards don't have.
- Validate on the 6T first (oldest WebView of the two phones = worst case);
  the OnePlus 6 should then follow for free.

## 7. Risks / open points

- **Overlay z-order**: native views float above ALL GL content — the webview
  must close/hide when its app is backgrounded or the shell draws chrome above
  the card area (badges, keyboard).
- **Security**: LLM-generated JS executes in the WebView. Mitigations in 3.1
  (no file access, navigation blocked, minimal bridge surface). The bridge must
  validate `notify` payloads exactly like the Splash event path does.
- **Streaming UX**: HTML cannot render mid-stream; the skeleton-until-close
  behavior (3.3) must feel intentional (spinner + app name from the first line).
- **`texture` field**: true GL composition (webview → SurfaceTexture, like the
  video path) would fix z-order properly but is much heavier; overlay first,
  composition later if chrome conflicts hurt.
- **Contract drift**: with no exemplar, the app.md IS the quality lever —
  failures should be triaged into contract amendments (a "contract changelog"
  section at its bottom keeps the why).

## 8. Implementation log + device findings (2026-07-17, OnePlus 6T)

### What was built

| Layer | Change |
|---|---|
| framework fork `aichat/` | `CxOsOp::SetSystemBrowserHtml` + `CxSystemBrowser::set_html` (cx_api.rs); macOS/iOS `set_html` staging temp-file + `loadFileURL` (apple_webview.rs, dispatch arms in macos.rs/ios.rs); **Android SystemBrowser backend** (op arms in android.rs + 6 JNI fns in android_jni.rs); `widgets/src/web_card.rs` (**WebCard** widget: streaming settle 0.35s, fixed overlay id `octos_web_card`, URLTEST debug hook); ```runhtml fence in markdown.rs → `web_block` template; `examples/webcard` desktop test app |
| buildtool `makepad/` | `MakepadActivity.java`: `mSystemBrowserOverlay` + spawn/update/detach/close/setUrl/**setHtml(loadDataWithBaseURL)** WebView methods, JS+DOM-storage enabled, WebContentsDebugging on; **manifest templates now set `android:hardwareAccelerated="true"` explicitly** |
| app `app/` | `web_block` templates (chat + card surface); 4th domain agent `web`; AMA fallback routes unknown-but-actionable → `web`; web-specific runhtml generation prompt; overlay detach on app-switch/clear; SEED_CARD_FILE seeds ```runhtml for HTML files |
| content `a2app/` | `apps/web/app.md` — the contract (no exemplar), incl. Chromium-92 baseline, YouTube embed pattern, bind-never-bake fetch rules; `framework.md` app list + build_memory.py FILES (MEMORY.md now ~20.4K tokens — raise `max_inject_tokens` ≥ 24000 on deploy) |

### The debugging journey (each was real and had to be found in order)

1. **`screencapture -l` (window-id) misses WKWebView's out-of-process layer** on
   macOS — content looked "white" while actually rendering. Full-screen captures
   tell the truth.
2. Bare (non-bundled) WKWebView **silently never commits `loadHTMLString`/
   `data:` documents** (and ATS blocks plaintext-loopback http). Network and
   `file://` loads work → macOS `set_html` stages a temp file. file:// origin
   sends no Referer → YouTube error 153 **on desktop only**; Android's
   `loadDataWithBaseURL(https-base)` supplies a real origin (verified: embed
   error UIs and the player load on device).
3. The **NextFrame "not drawn → detach" watchdog was wrong**: makepad draws on
   demand, so idle ≠ gone. First WebView appeared at the right rect, then went
   GONE. Overlay teardown belongs to the app shell (route/clear) only.
4. OnePlus 6T ships **System WebView Chromium 92 (2021)**; installed Google's
   **WebView Beta 151** (`apkeep -d apk-pure`, `adb install`, `cmd webviewupdate
   set-webview-implementation com.google.android.webview.beta`).
5. **THE BIG ONE:** cargo-makepad's generated manifest emits `<uses-sdk>` AFTER
   `</application>`; Android's PackageParser resolves the
   `hardwareAccelerated` default WHILE parsing `<application>` (targetSdk still
   0 → default false) → **every octos-one window rendered in software**
   (HWUI "Total frames rendered: 0", chromium "tile memory limits exceeded",
   and video—which requires HW composition—permanently black while HTML still
   painted). CDP proved the player was PLAYING (t=31s, frames decoded) with no
   pixels on screen. Fix: explicit `android:hardwareAccelerated="true"` in the
   templates. This also speeds up every native overlay in the app.
6. **YouTube live-stream IDs rotate**: the famous lofi id now returns "live
   stream recording is not available". Resolve the current one via
   `https://www.youtube.com/@Channel/live` (redirect carries `"videoId"`), or
   let cards fall back to a regular video. The contract's guidance was updated
   by this lesson.

### Device state changed on the 6T (for reproducibility / cleanup)

- `com.google.android.webview.beta` 151.0.7922.29 installed and set as WebView
  implementation (revert: `cmd webviewupdate set-webview-implementation
  com.google.android.webview`).
- WebView DevTools flag `WebViewSurfaceControl = Disabled` (set while
  diagnosing; with the HW-acceleration fix it is likely unnecessary — reset via
  WebView DevTools → Flags → Reset flags).
- `settings global http_proxy 127.0.0.1:8899` + `adb reverse tcp:8899 tcp:8899`
  so WebView traffic rides the host CONNECT proxy (the phone has no direct
  internet). Clear with `adb shell settings put global http_proxy :0`.
- Demo card at `/data/local/tmp/yt_card.html`, seeded via
  `--es makepad.SEED_CARD_FILE`.

### Remaining for the full LLM flow (not blocking the card runtime)

The 6T is unrooted and the installed build non-debuggable, so
`MEMORY.md`/`_main.json` (memory injection + `max_inject_tokens`) could not be
deployed from the host. Options: build once with `--debug` (run-as works),
or provision on a rooted device per BUILDING-ANDROID.md. The AMA `web` routing,
generation prompt, and contract are already in place — with memory deployed,
"play a lofi video" routes to the web agent and the LLM composes the same card
the seed demonstrated (glm-5.2 is already provisioned on-device).

## 9. The youtube agent (2026-07-17) — first contract-only domain agent

**Shipped:** a 5th domain agent `youtube` (AMA routes every video/music/live
intent) generating a full-featured YouTube player card — tabs
(Home/Favorites/History), autoplaying player, now-playing + ♥ favorites,
localStorage history, tile sections, fullscreen — from
**`a2app/apps/youtube/app.md` alone (contract-only, NO exemplar)**, per the
app-card principle. Verified end-to-end on the OnePlus 6T: `AUTO_PROMPT "play
some lofi music"` → AMA → youtube agent (glm-5.2 on device) → card → **live
Lofi Girl radio playing**.

Contract-iteration lessons (each failure was fixed in the CONTRACT or runtime,
never with an exemplar):

1. **Output truncation**: first generation died mid-`<script>` (~10.4KB doc).
   Fix: a hard **7,000-char size budget** ("compactness is a correctness
   requirement"), no code comments, and **resilience ordering** — player
   `src` + now-playing INLINE in HTML, tiles inline, JS only enhances; script
   ordered so early truncation degrades gracefully.
2. **Dead live ids**: models "remember" retired live-stream ids (they rotate),
   and YouTube's `embed/live_stream?channel=` endpoint is unreliable. Fix at
   THREE levels: (a) contract forbids memorized live ids; (b) the octos
   `web_fetch` tool procedure resolves `/@handle/live`; (c) **deterministic
   app-runtime resolution** — `refresh_youtube_live_ids()` in `app/src/main.rs`
   fetches the 4 curated channels' current ids (through the OCTOS proxy,
   3 retries, warmed at boot + prompt-submit + routing with a bounded wait)
   and injects them into the generation prompt as
   "CURRENT LIVE VIDEO IDS (ground truth … USE THESE EXACT IDS)". This is the
   `sys.*` data-binding idea applied at GENERATION time.
3. The contract is embedded into the binary via
   `include_str!("../../../a2app/apps/youtube/app.md")` — single source of
   truth in a2app, no device memory deployment needed for this domain.

Runtime facts the card relies on (probed on device): noembed.com is CORS-open
(tile title/author enrichment + dead-id cleanup); `i.ytimg.com` thumbnails are
keyless; the WebView runtime allows sound-on autoplay
(`setMediaPlaybackRequiresUserGesture(false)`); fullscreen works via the
custom-view WebChromeClient.

## 10. YouTube player card — real-app fidelity (2026-07-17, score 9/10)

Iterated the youtube card to a real-YouTube-app-grade watch/home/library
experience, validated on the OnePlus 6T by strict Claude visual-UX judges
across a v2→v7 loop (6 → 8 → 8.5 → 8.5 → 8.5 → **9/10**). Reference:
`docs/youtube-player-reference.html` (the seed that scored 9; NOT injected as
memory — the contract `a2app/apps/youtube/app.md` is the spec).

Real features implemented (all functional, localStorage-backed): Like/Dislike
(outline↔filled glyph toggle on dark pills), Subscribe (→ bell + "Subscribed"),
Share (copies link), Save (Watch later), Remix/Report; a device-local Comments
section (add/persist per video); Up-next + a Home feed with functional filter
chips (All/Music/Live/News) and per-item kebab → a Material bottom sheet
(Save/Share/Not-interested/Don't-recommend-channel/Report with distinct icons);
a You/library page (profile row, horizontal History carousel + View all, Liked,
Watch later, Subscriptions as avatar circles); a docked **miniplayer** when you
leave a playing video; real channel avatars via `unavatar.io/youtube/@handle`
(monogram+gradient fallback); **Captions** (CC via `cc_load_policy=1`) and
**Translate** — a 12-language "Translate captions" sheet driving
`cc_lang_pref` so YouTube auto-translates subtitles (verified on Despacito:
English "Dididiri Daddy, go!" → Chinese).

Loop lessons worth keeping:
- **Rating loop with a strict sub-agent judge** (SCORE/ISSUES/VERDICT, fixed
  calibration) is a fast, objective way to drive UI fidelity; each round fixed
  the judge's top-8 concretely.
- **Watch has NO top app bar** (real YT); Home/You do. Sticky player needs a
  solid seam (`box-shadow:0 8px 0 #0f0f0f,...`) or the row beneath bleeds.
- **Host-composer docking**: the chat composer overlays the bottom ~52 CSS px
  of the WebView (measured with an on-screen pixel ruler). Dock miniplayer/
  sheet at `bottom:0` with `padding-bottom:52px` so their solid bg seals behind
  the composer — floating them with a gap leaks un-scrimmed content (the single
  thing that kept it at 8.5 until fixed).
- **Emoji from fetched titles render as boxed sprite tiles** — strip them.
- CDP over `adb forward … webview_devtools_remote_<pid>` (with
  `suppress_origin=True`) drives/inspects the card headlessly for capture.
