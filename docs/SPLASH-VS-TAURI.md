# Splash vs Tauri — Architecture Direction Memo

A decision-oriented record of our discussion: **should Splash-on-Android drop its
self-rendering engine and lean on native components / a system WebView the way
React Native or Tauri do?** Conclusion first, then the reasoning.

For the code-level mechanics behind the claims here (how the WebView is wrapped,
the two integration strategies, the JNI bridge, the uniform widget registry), see
the companion reference [`SPLASH-NATIVE-INTEGRATION.md`](./SPLASH-NATIVE-INTEGRATION.md).
This memo is the *"what should we do about it"*; that doc is the *"how it works."*

---

## TL;DR

- **Splash is architecturally Flutter, not React Native or Tauri.** It creates one
  `SurfaceView` + GL/EGL context and paints every pixel of its normal UI itself.
- It has **Flutter's exact two escape hatches**: *platform views* (overlay a real
  native view — this is the WebView) and *external textures* (composite decoded
  frames into the scene — this is video).
- **Going full-native/Tauri optimizes for ONE real benefit — native maturity —
  by spending three assets you already own:** Rust performance, cross-platform
  *uniformity*, and the animation/perf ceiling.
- **Recommendation: stay hybrid.** Keep Makepad self-draw for the bulk; expand the
  native-overlay hatch only where a native component's *behavior* beats pixels.
- If your real goal is "lean on a mature cross-platform substrate," the honest name
  for that is **Tauri (a WebView)** — and you've **already built ~most of it** as
  the `runhtml` card path. Promote that per-panel; don't rip out Makepad.
- You **can migrate to the new Splash VM (`ymote/Splash`) and keep every hybrid UI
  benefit** — the language is already decoupled from the renderer.

---

## 1. Where Splash sits on the spectrum

| Capability | Flutter | React Native | Tauri | **Splash** |
| --- | --- | --- | --- | --- |
| Renders all normal UI | Skia / Impeller (self-draw) | native OS widgets | one system WebView | **own GLES shader engine (self-draw)** |
| Host a real native view | Platform Views (`AndroidView`) | *is* the whole tree | n/a (all web) | **overlay stack** (the WebView, camera, composer `EditText`) |
| Composite foreign frames | External Textures | n/a | n/a | **`SurfaceTexture` → OES → shader** (video) |
| Cross-platform model | one canvas everywhere | native per-OS (2 platforms) | one WebView everywhere | **one canvas everywhere** |

**Splash is in the Flutter camp**, unambiguously. It is *not* React Native (whose
entire tree is native views) and *not* Tauri (whose entire UI is one WebView).

## 2. "Can we say Splash is both Flutter AND Tauri?"

Your intuition — "you get self-rendering **and** system-component hosting in one
app" — is right *in spirit*, but the relationship is **hierarchical, not
co-equal**. The GPU engine is always the host; native views and web documents are
*guests embedded in rectangles*.

So the accurate one-liner is: **Splash is a Flutter-class self-rendering engine
that can rent out a rectangle to the system WebView.** Because one of the things
the platform-view hatch can host is a *full* WebView, you can opt any single panel
into **Tauri-style web UI** while the rest of the app stays native-GPU. It is never
symmetric: the default and the bulk is always self-drawn.

## 3. Should we drop Makepad drawing to go native/Tauri?

The stated goal was **"Rust performance + native maturity + native performance +
cross-platform."** Pulled apart, three of the four are already yours or argue the
*other* way — only one is a real, un-had benefit.

| Stated goal | Reality |
| --- | --- |
| **Rust performance** | ✅ **Already have it** — the core is Rust; the drawing layer is orthogonal. Dropping the renderer gains *zero* Rust perf. |
| **Cross-platform** | ⚠️ **Already have it — native *hurts* it.** Makepad self-draws one surface everywhere (Android + macOS off one tree today). Native widgets are the *enemy* of cross-platform uniformity: each is a per-platform divergence you now own. RN is cross-platform *despite* native widgets, over just two platforms. |
| **Native performance** | ⚠️ **Nuanced, and backwards for this workload.** 2026 data: native (RN) wins cold-start / battery / memory; self-render (Flutter/Makepad) wins animation, sustained 60/120fps, custom UI. AI-composed animated cards + glass + charts is squarely the self-render-wins column. |
| **Native maturity** | ✅ **The one genuine benefit** — accessibility, text/IME/i18n, system widgets, automatic OS-design updates. |

**Translation:** you want *native maturity*; the other three are already yours or
push against going native. The real question is the cheapest way to get maturity
*where it matters* without throwing away the self-render assets you already own.

## 4. The three backend options, and how feasible each is

| Option | Verdict | Why |
| --- | --- | --- |
| **A · OS-native widgets per platform** (React Native model) | ❌ **Don't** | The 30-year leaky-abstraction graveyard (AWT, SWT, wxWidgets, MAUI). No mature unifying Rust binding — you'd hand-build the reconciler + bridge + per-platform component libraries over `objc2`/`jni`/`windows-rs`/`gtk-rs`. Even **Dioxus** (the leading Rust declarative UI) declined this and went webview + its own `Blitz` renderer. Multi-year, and you *still* lose cross-platform uniformity and the fps ceiling. |
| **B · System WebView** (Tauri model) | ✅ **Feasible — ~60% already built** | The `runhtml → WebCard → system WebView` path plus the `octos.*` web kit is a working on-device proof. To "go Tauri," make the webview the *default* substrate and compile the Splash DSL → HTML/CSS/JS. Cross-platform + a hyper-mature engine (layout, text, IME, a11y, i18n) for free. **Costs:** it's *web* maturity, not native-widget look; you lose GPU shaders / continuity / the fps ceiling (glass blur, animated icons, 120fps custom UI all go away); you inherit web weight + reload/settle seams + compositing gotchas (the black-video saga was a Tauri-class problem). |
| **C · A different Rust self-drawn toolkit** (Slint / Blitz / egui) | ⚠️ **Orthogonal** | Still self-drawn → *zero* native maturity gained. Only rational if you dislike Makepad specifically. Irrelevant to "get native." |

## 5. Recommendation — the hybrid (what you're already running)

Not a compromise; it's what every serious self-render engine converges on
(Flutter itself is exactly this).

- **Keep Makepad self-draw for the ~85%** where it wins: custom AI-composed UI,
  animation, data-viz, glass, raw perf, and cross-platform *uniformity*.
- **Expand the Strategy-A native-overlay hatch for the ~15%** where a native
  component's **behavior** beats self-rendered pixels: text/IME (already done — the
  chat composer is a real `EditText`), and system widgets (web / map / video /
  camera / pickers / share).
- **For accessibility — native's single strongest card — adopt
  [AccessKit](https://github.com/AccessKit/accesskit)**, which feeds the OS a11y
  APIs (UIA, NSAccessibility, AT-SPI) from a *self-drawn* tree. It neutralizes the
  biggest reason to go native without abandoning self-render. (Caveat: its
  Android/mobile support is younger than desktop — worth a spike, since Android is
  the primary target.)

**The deciding principle** for each component: *do you need the OS widget's
behavior, or just its pixels?* Buttons/labels/lists → self-draw (pixels are easy,
self-render wins on continuity + shaders). A full-IME text field or a web document
→ host it (behavior is worth more than pixels). That judgment is what keeps most of
the UI self-drawn while the escape hatch stays a surgical tool, not a widget system.

## 6. Migrating to the new Splash VM (`ymote/Splash`) — keep the hybrid

`ymote/Splash` is not the widget layer — it's the Splash **VM/language**,
restructured into a capability-secure, bounded, **UI-optional** runtime. Its own
words: *"starts from the Makepad Splash VM and keeps UI support optional rather than
making UI the language boundary."* Makepad survives only as a `vendor/makepad`
parser-compat fixture; UI hosts install their own bindings via
`check_vm_compatibility_named`.

**The key consequence: the language is already decoupled from Makepad drawing.**
So migration does not hand you the hybrid UI — you *keep* it — and it does not force
the native/webview decision either. The slogan is **"migrate the language, re-bind
the UI."**

| | What you gain (VM-level) | What it costs (one seam) |
| --- | --- | --- |
| Migration | Deny-by-default tool host (ideal for LLM-generated cards); bounded, auditable execution; durable workflows; LSP; sandboxing. `sys.*` helpers become registered capability-bound tools. | Re-wire `register_widget` / `mod.widgets.*` and `sys.*` onto the new capability model; invert control so the Makepad host owns the loop and drives the VM; re-test cards against the new runtime limits (parsing is preserved; semantics must be verified). |

The WebView overlay, the native `EditText`, the video texture path, and the Makepad
renderer all come along intact — the port is concentrated at the **VM↔host binding
boundary**, and it's independent of the Tauri-vs-hybrid decision above.

---

## Bottom line

"Drop all Makepad drawing and go native like RN/Tauri" optimizes for one real
benefit — native **maturity** — by spending three assets you already own: Rust
performance, cross-platform **uniformity**, and the animation/perf ceiling. The
all-native (RN) path is the hardest, *least* cross-platform, worst-ROI route, and no
Rust stack does it. If your true driver is "a mature cross-platform substrate," the
honest name is **Tauri/WebView** — and you've already built most of it as the
`runhtml` path, so the move is to *promote that to the default substrate per panel*,
not to rip out Makepad. But the strongest answer stays the **hybrid**: Makepad for
the frame, native overlays for the rectangles where behavior wins, AccessKit for
a11y — the one path that keeps all four goals instead of trading three away to chase
the fourth. And you can migrate to the new Splash VM without giving any of it up.

---

*Documented 2026-07-18 from a working session on octos-one. Landscape claims
(RN/Flutter 2026 perf, the Rust-UI ecosystem, `ymote/Splash`) were cross-checked
against current sources at the time. See `SPLASH-NATIVE-INTEGRATION.md` for the
verified code-level detail.*
