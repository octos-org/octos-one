# Splash on Android: how it wraps native, and where it can go

*octos-one engineering note — Markdown companion to the visual artifact version of
this deep-dive. Documented from a working session on 2026-07-18 against branch
`fix/current-octos-compat` (PR #5).*

A working-session record — how the Makepad Splash UI hosts native Android views
over a GPU surface, how that compares to Flutter, React Native and Tauri, and the
feasibility of going native or migrating to the new Splash VM.

| | |
|---|---|
| **Date** | 2026-07-18 |
| **Scope** | octos-one · aichat fork · makepad platform |
| **Device** | OnePlus 6T · Android 11 |
| **Video bug** | resolved · 18ed05b |

## Contents

- [1. The black-video fix that opened the session](#1-the-black-video-fix-that-opened-the-session)
- [2. Makepad's Android foundation](#2-makepads-android-foundation)
- [3. How the WebView is wrapped](#3-how-the-webview-is-wrapped)
- [4. Two integration strategies](#4-two-integration-strategies)
- [5. Strategy A is native-widget integration](#5-strategy-a-is-native-widget-integration)
- [6. One interface, two physics](#6-one-interface-two-physics)
- [7. Flutter vs React Native vs Tauri](#7-flutter-vs-react-native-vs-tauri)
- [8. The new Splash language (ymote/Splash)](#8-the-new-splash-language-ymotesplash)
- [9. Feasibility: dropping Makepad drawing for native](#9-feasibility-dropping-makepad-drawing-for-native)
- [10. Migration: new VM, same hybrid UI](#10-migration-new-vm-same-hybrid-ui)
- [11. Reference: files & line numbers](#11-reference-files--line-numbers)

---

## 1. The black-video fix that opened the session

The YouTube card played audio but showed a black frame. The root cause was not
compositing or z-order — it was a malformed Android manifest that silently
disabled hardware acceleration.

octos-one ships its *own* `AndroidManifest.xml.template` that shadows
cargo-makepad's default. A merge brought in a copy that was missing
`android:hardwareAccelerated="true"` on `<application>` and emitted `<uses-sdk>`
*after* `</application>`. Android then defaulted hardware acceleration to
**false**, so the whole app — WebView included — software-rendered. That produced
the Chromium `cc` error "tile memory limits exceeded" and a black video surface
while the surrounding HTML painted fine.

> **Resolution — committed 18ed05b (PR #5)**
>
> Add `android:hardwareAccelerated="true"` and move `<uses-sdk>` before
> `<application>`. On-device the "tile memory limits exceeded" count went
> **82 → 0** and the live video renders — visible even in a plain `screencap`,
> which could not capture it while it was software-composited.

Two lessons worth keeping: it "worked yesterday" because yesterday's build
predated the bad template; and when WebView video is black, the *first* thing to
check is the generated manifest's `hardwareAccelerated` flag and `<uses-sdk>`
placement — the z-order and SurfaceControl theories were red herrings.

---

## 2. Makepad's Android foundation

Makepad is a self-rendering GPU engine. It owns one surface and paints every
pixel of its normal UI itself — the same architectural family as Flutter.

The Activity hosts a single plain `SurfaceView` (`MakepadSurface`). When Android
creates the buffer, the raw `Surface` is handed across JNI to a dedicated Rust
render thread, which builds an **OpenGL ES 3 context via EGL** and swaps buffers
per vsync. There are *no* Android widgets for ordinary UI — text, buttons,
layout, scrolling and the glass blur are all GPU-drawn.

**`MakepadActivity.java:458` — the surface crosses into native code**

```java
public void surfaceCreated(SurfaceHolder holder) {
    Surface surface = holder.getSurface();
    MakepadNative.surfaceOnSurfaceCreated(surface);   // → ANativeWindow → EGL window surface
}
```

### The JNI bridge is symmetric

- **Java → Rust:** Java declares `native` methods in `MakepadNative.java`; each is
  implemented as a `Java_dev_makepad_android_MakepadNative_*` symbol that packages
  its args into a `FromJavaMessage` and pushes it onto an mpsc queue the render
  thread drains. This inbound side *is* a single generic queue.
- **Rust → Java:** 45 hand-written `to_java_*` functions attach the thread's
  `JNIEnv`, fetch a global `Activity` handle, and call an Activity method via the
  `call_void_method!` macro — which caches the resolved `jmethodID` per call-site.
  UI-touching methods hop to `runOnUiThread`.

### The view hierarchy is a stack of overlays

The content view is a root `FrameLayout`. The GL surface sits at the bottom; a
stack of sibling `FrameLayout` overlays is added on top. Z-order is
child-insertion order plus occasional `bringToFront()`. The main surface sets no
`setZOrderOnTop`, so it is always the bottom layer and every overlay composites
above it.

---

## 3. How the WebView is wrapped

A runhtml card becomes a real `android.webkit.WebView` that the Android window
system paints on top of the GL surface, glued frame-by-frame to the widget's
on-screen rectangle. It is an overlay, not a composite.

**the pipeline: runhtml fence → pixels in a WebView**

```
runhtml fence (LLM output)
  → WebCard widget            aichat/widgets/src/web_card.rs
       fixed id octos_web_card · 0.35s settle · injects octos.* JS kit
  → CxOsOp op enum             cx_api.rs — SetSystemBrowserHtml, UpdateSystemBrowser…
  → Android backend            android.rs:2505 — area → physical-pixel rect
  → JNI to_java_*              android_jni.rs:2422 — call_void_method!
  → Java on the UI thread      MakepadActivity.java
       ensureSystemBrowser()  new WebView → addView(mSystemBrowserOverlay)
       updateSystemBrowser()  LayoutParams(w,h) + left/topMargin + visibility
       setSystemBrowserHtml() loadDataWithBaseURL("https://octos-one.app/", html…)
  → WebView paints the card at the widget's exact rect, over the GL surface
```

Two load-bearing details. The card is positioned by **FrameLayout margins**
computed from the widget's layout rect and re-pushed every draw, so it tracks the
widget. And it loads via `loadDataWithBaseURL` with an **https base URL** so the
document has a real origin — referer-gated embeds like YouTube break under a
`file://` origin (exactly the macOS pain point, where a `WKWebView` stages a temp
file and uses `loadFileURL` instead).

> **One reused view**
>
> WebViews are kept in a `HashMap<Long, WebView>` keyed by browser id, and the
> card always uses the single `octos_web_card` id — so there is exactly one
> WebView, reused across cards. The widget deliberately does not self-destruct on
> "not drawn lately"; teardown is owned by the app shell on foreground-switch or
> chat-clear.

---

## 4. Two integration strategies

The central realization: there isn't one native-integration method, there are
two — and the WebView uses the weaker one.

```
                         ▲  overlays paint ABOVE the GL surface
┌────────────────────────────────────────────────────────────────┐
│  NATIVE OVERLAYS            Strategy A — OS-composited (amber) │
├────────────────────────────────────────────────────────────────┤
│  mComposerOverlay            native EditText pill · top        │
│  mSelectionHandleOverlay     text-selection handles            │
│  mSystemBrowserOverlay ★     android.webkit.WebView            │
│  mCameraPreviewOverlay       camera SurfaceView                │
└────────────────────────────────────────────────────────────────┘
───────────────────── — window compositor — ──────────────────────
┌────────────────────────────────────────────────────────────────┐
│  MakepadSurface (SurfaceView)   Strategy B — sampled in (cyan) │
├────────────────────────────────────────────────────────────────┤
│  EGL · OpenGL ES 3 · draws ALL normal UI itself with shaders   │
│                                                                │
│  ◄─ video frames sampled IN here                               │
│     SurfaceTexture → external OES texture                      │
└────────────────────────────────────────────────────────────────┘
```

*Legend: **amber** = Strategy A, OS-composited overlay · **cyan** = Strategy B,
sampled into the GL scene. Native overlays (amber) are real Android Views the
window system paints above the GL surface. Video (cyan) is the exception: it is
decoded into a texture Makepad samples inside its own shaders.*

### Strategy A — overlay-on-top

A real Android `View` in a sibling `FrameLayout`, positioned to a screen rect,
composited above the GL layer by Android. Used by the WebView, the camera
preview, the composer `EditText`, selection handles and the QR scanner.

### Strategy B — texture-into-scene

Video decodes via `MediaPlayer` into a `SurfaceTexture` created from a GL texture
handle; Makepad samples that external OES texture *inside its own shaders*. The
foreign content becomes a first-class Makepad texture — transformable, clippable,
shadeable, in any z-order, exactly like Makepad's own pixels.

> **The tell**
>
> A dormant `CxOsOp::CreateWebView` op still carries an unused `texture` field —
> an abandoned attempt to bring the WebView onto Strategy B. It's dead because
> WebView-to-texture is impractical on Android, and web-embedded video escapes to
> a further `SurfaceControl` hardware surface. The black-video saga was that
> overlay-within-an-overlay losing its compositing path.

### Is the overlay method generic?

Yes — and it's already proven. The same `to_java_* → runOnUiThread → addView`
idiom drives **five** native surfaces today. Adding a sixth is one `to_java_x`
wrapper plus one Java method. But it is generic only for a specific *shape*:
hosting a foreign rectangle wholesale. Five limits define that shape:

| Limit | Consequence |
|---|---|
| Rect-only, screen-aligned | No Makepad transform, rounded corners, shader or glass-blur on the content. This is why the "real glass" weather card had to be native — you can't blur a WebView with Makepad's backdrop shader. |
| Always on top | Overlays stack above the GL surface; you can't draw Makepad content over an overlay in the same layer. |
| Thread-hop per op | `runOnUiThread` is fine for coarse show/hide/resize, wrong for per-frame fine-grained widgets. |
| Input arbitration | Whichever view is physically on top eats the touches. |
| Scroll-sync lag | The rect is re-posted on the UI thread, one frame behind a GPU scroll. |

---

## 5. Strategy A *is* native-widget integration

It's not just for foreign content like web or maps. The chat composer is a real
`android.widget.EditText` — an interactive native widget, hosted as an overlay, in
an app whose UI is otherwise entirely self-drawn.

And it's two-way, not display-only. Rust drives the widget outward (show/hide,
position to a rect); the widget reports back *inward* through the same
`FromJavaMessage` queue: the `EditText` posts `onComposerSubmit(text)`, selection
handles post `onSelectionHandleDrag`, the IME posts `onImeTextStateChanged`. The
native widget's behavior feeds Makepad's logic.

> **Portal, not substrate — the distinction that survives**
>
> In **React Native**, native widgets *are* the tree: every node is a native
> view, nested arbitrarily deep, composited together by the OS. In Makepad's
> Strategy A, native widgets are *flat guests pinned onto a self-rendered scene* —
> Makepad computes a rectangle and pushes the widget onto it from outside. Same
> idea as Flutter's platform views; not RN's substrate.

### The design principle it reveals

Makepad makes the draw-it-yourself vs. host-the-native-widget choice **per
component**, deciding on one axis: *do you need the OS widget's behavior, or just
its pixels?*

- **Buttons, labels, lists, ordinary text** → self-drawn. Pixels are easy;
  self-rendering wins on continuity, shaders and one render thread. Even general
  text input is self-drawn — the SurfaceView is its own IME client via a custom
  `InputConnection`.
- **The chat composer** → a real `EditText`. Here you need *behavior*: full IME,
  multi-language compose, autocomplete, clipboard, cursor handling. Brutal to
  self-render; free from a native widget.
- **WebView, camera, map** → host, obviously — you can't self-draw a web engine.

---

## 6. One interface, two physics

From Splash's orchestrator, native and system widgets are literally the same kind
of thing — provably, from the registry. The uniformity is real, and exactly
skin-deep.

Every widget — self-drawn and system-hosted alike — registers into **one
namespace** (`mod.widgets.*`) via **one mechanism** (`register_widget(vm)`) and
implements **one trait** (`Widget`, whose hot methods are `handle_event` and
`draw_walk`).

**the WebView host sits in the same registry as the glass panel and Button**

```
mod.widgets.WebCard          = #(WebCard::register_widget(vm))        web_card.rs:61   ← hosts the OS WebView
mod.widgets.SplashBase       = #(Splash::register_widget(vm))         splash.rs:1257
mod.widgets.ButtonBase       = #(Button::register_widget(vm))         button.rs:16
mod.widgets.GaussRoundedView = #(GaussRoundedView::register_widget)   gauss_view.rs:98  ← the glass panel
mod.widgets.LineChartBase    = #(LineChart::register_widget(vm))      chart.rs:58
```

So the orchestrator holds a `WidgetRef`, calls `draw_walk(cx, scope, walk)`, and
cannot tell a glass panel from a WebView-card. That's a deliberate unification.
But the shared `draw_walk` call **forks into two different physics**:

| On the same `draw_walk` call… | Native widget (glass, button, chart) | WebCard |
|---|---|---|
| What it emits | `GPU commands` into the shared GL surface | `a rect` pushed over JNI — no card pixels |
| Who owns the pixels | Makepad's compositor | the Android window system |
| Can be shaded / z-interleaved / clipped | `yes` | `no — portal limits` |

It's a well-worn pattern: a uniform component interface over two rendering
backends. Java called it **lightweight vs. heavyweight** (Swing self-drawn vs. AWT
native peers — same `add()`, but heavyweights always paint on top and ignore
z-order). Flutter calls it `RenderObject` vs. `PlatformView`. Makepad's is
native-`Widget` vs. `WebCard`. Same handle, different physics.

*Note: the runsplash-vs-runhtml split you see at authoring time is a product
convention for which substrate a whole card uses — not a machinery boundary. The
VM treats `WebCard` as a first-class DSL widget either way.*

---

## 7. Flutter vs React Native vs Tauri

Splash is unambiguously in the Flutter camp — a self-rendering GPU engine — with
the same two escape hatches Flutter has. It is not React Native, and not Tauri.

| Capability | Flutter | Makepad / Splash |
|---|---|---|
| Self-renders all normal UI | Skia / Impeller | `its own GLES shader engine` |
| Host a real native view | Platform Views (`AndroidView`) | `overlay stack` — the WebView |
| Composite foreign frames | External Textures (`SurfaceTexture`) | `SurfaceTexture → OES → shader` — video |

So the accurate framing: **Splash is a Flutter-class self-rendering engine with
Flutter's two escape hatches** — platform views (overlay a WebView / camera / map)
and external textures (composite decoded video into the scene). It is *not* React
Native (whose whole tree is native views) and *not* Tauri (whose whole UI is one
WebView). But because one thing the platform-view hatch can host is a full
WebView, you can opt any single panel into Tauri-style web UI while the rest stays
native-GPU. The relationship is hierarchical: the GPU engine is always the host;
native views and web documents are guests in rectangles.

---

## 8. The new Splash language (ymote/Splash)

The repo at `github.com/ymote/Splash` is not the widget layer — it's the Splash
*VM/language*, restructured into a capability-secure, bounded, UI-optional
orchestration runtime.

In its own words, it "starts from the Makepad Splash VM and keeps UI support
optional rather than making UI the language boundary." The runtime does **not**
load Makepad widget modules or event loops by default; Makepad survives only as a
`vendor/makepad` parser-compatibility fixture. UI hosts install their own bindings
via `check_vm_compatibility_named`. It's a deny-by-default tool host — scripts
call only explicitly registered tools through `mod.tool` — with bounded, auditable
execution.

> **Why this matters here**
>
> The language is **already decoupled** from Makepad drawing. That turns "can we
> rip out Makepad?" into the far smaller "which UI backend do we bind?" The
> DSL/logic layer ports for free; the whole question lives in the render backend.

*Crate structure: splash-core · splash-capabilities · splash-schema ·
splash-storage · splash-protocol · splash-worker · splash-sandbox ·
splash-workflow · splash-cli · splash-lsp · vendor/makepad.*

---

## 9. Feasibility: dropping Makepad drawing for native

The goal was "Rust performance + native maturity + native performance +
cross-platform." Pulled apart, three of the four are already yours or point the
other way — only one is a real, un-had benefit.

| Stated goal | Reality |
|---|---|
| Rust performance | `already have it` — the core is Rust; the drawing layer is orthogonal. Dropping the renderer gains zero Rust perf. |
| Cross-platform | `already have it — native hurts it`. Makepad self-draws one surface everywhere. Native widgets are the *enemy* of cross-platform uniformity: each is a per-platform divergence you now own. |
| Native performance | `nuanced, and backwards for you`. 2026 data: native (RN) wins cold-start / battery / memory; self-render (Flutter/Makepad) wins animation, sustained fps, custom UI. Your workload — animated AI-composed cards, glass, charts — is the self-render column. |
| Native maturity | `the one genuine benefit` — accessibility, text/IME/i18n, system widgets, automatic OS-design updates. |

### Three backends, and how feasible each is

| Option | Verdict | Why |
|---|---|---|
| A · OS-native widgets per platform (`RN model`) | `don't` | The 30-year leaky-abstraction graveyard (AWT, SWT, wxWidgets, MAUI). No mature unifying Rust binding — you'd hand-build the reconciler + bridge + per-platform component libs over `objc2`/`jni`/`windows-rs`/`gtk-rs`. Even Dioxus declined this path (webview + Blitz instead). Multi-year, and you still lose cross-platform uniformity and the fps ceiling. |
| B · System WebView (`Tauri model`) | `feasible · ~60% built` | The `runhtml → WebCard → system WebView` path plus the `octos.*` web kit is a working proof. Make the webview the default substrate and compile the DSL → HTML/CSS/JS. Cross-platform + mature engine for free. Cost: web maturity, not native-widget look; lose GPU shaders and the fps ceiling; inherit web weight and reload seams. |
| C · Another Rust self-drawn toolkit (`Slint / Blitz`) | `orthogonal` | Still self-drawn → zero native maturity gained. Trades Makepad's shader/perf strengths for another engine's ergonomics. |

> **Recommendation — the hybrid you already run**
>
> Keep Makepad self-draw for the ~85% where it wins (custom UI, animation, viz,
> glass, perf, cross-platform uniformity). Expand Strategy-A native hosting for
> the ~15% where maturity beats pixels (text/IME — already done for the composer;
> web/map/video/camera/pickers). For accessibility — native's strongest card —
> adopt **AccessKit**, which feeds OS a11y APIs from a self-drawn tree without
> going native (caveat: its Android support is younger than desktop). This is
> exactly Flutter's model, and octos-one is already on it.

---

## 10. Migration: new VM, same hybrid UI

You can move to the new Splash language and keep every hybrid UI benefit — because
they live in different layers. The slogan is "migrate the language, re-bind the
UI."

| | What you gain (VM-level) | What it costs (one seam) |
|---|---|---|
| Migration | Deny-by-default tool host (ideal for LLM-generated cards); bounded, auditable execution; durable workflows; LSP; sandboxing. Your `sys.*` helpers become registered capability-bound tools. | Re-wire `register_widget` / `mod.widgets.*` and `sys.*` onto the new capability model; invert control so the Makepad host owns the loop and drives the VM; re-test cards against the new VM's runtime limits (parsing is preserved; semantics must be verified). |

The WebView overlay, the native `EditText`, the video texture path and the
Makepad renderer all come along intact — the port is concentrated at the VM↔host
binding boundary. And it's **independent** of the native/webview axis in §9: you
can migrate the VM and keep your exact current hybrid, then decide separately
whether to push any panel toward a heavier native overlay or a webview-default.
The same clean seam enables both.

---

## 11. Reference: files & line numbers

The concrete pointers behind the analysis, from three code-trace passes over the
octos-one tree. Rust compiles from the `aichat/` fork; the packaged Java is
`makepad/`'s.

| Location | What's there |
|---|---|
| `MakepadActivity.java:362,458,1319` | `MakepadSurface` (SurfaceView), `surfaceCreated`, root FrameLayout + overlay stack |
| `MakepadActivity.java:902` | Camera preview — a 2nd SurfaceView, `setZOrderMediaOverlay(true)` |
| `MakepadActivity.java:2498,3253,3332,3389` | Composer EditText; `ensureSystemBrowser`, `updateSystemBrowser`, `setSystemBrowserHtml` |
| `android.rs:1798–1980` | Render thread, EGL / OpenGL ES 3 context, `eglSwapBuffers` |
| `android.rs:2505–2545` | `CxOsOp` SystemBrowser dispatch → physical-pixel rect |
| `android_jni.rs:2334–2435` | `to_java_*` browser wrappers; `to_java_set_system_browser_html` |
| `ndk_utils.rs:33–95` | `call_void_method!` — cached `jmethodID` JNI macros |
| `web_card.rs:61,75,123,165` | WebCard registration, fixed id, `load_settled`, `draw_walk` rect push |
| `cx_api.rs:366–394` | `CxOsOp` enum — SystemBrowser ops + dormant `CreateWebView{texture}` |
| `VideoPlayer.java · android_video_playback.rs` | Strategy B — `MediaPlayer` → `SurfaceTexture` → external OES texture |
| `widget.rs:211,221,285` | `trait Widget` — `handle_event`, `draw_walk` |
| `apple_webview.rs` | macOS `WKWebView` — temp-file staging, `loadFileURL` |
| `app/app/resources/android/AndroidManifest.xml.template` | The video fix — `hardwareAccelerated` + `<uses-sdk>` placement |

---

## Provenance

Documented from a working session on 2026-07-18 against the octos-one repository
(branch `fix/current-octos-compat`, PR #5). Architecture facts verified by three
independent code-trace passes over the aichat fork, the makepad platform layer,
and the packaged Java; landscape claims cross-checked against current Rust-UI and
React Native / Flutter sources. Line numbers are anchors, not contracts — they
drift with edits.

**Open items:** the video fix is committed (18ed05b) and rides PR #5; the dev-only
`MAKEPAD_FORCE_DEBUGGABLE` gate in cargo-makepad can be reverted (a normal release
now plays video); a stale generated `MEMORY.md` sits untracked in the repo root.
