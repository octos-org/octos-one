# Splash DSL → native Android widgets: a deep dive

*Deep dive, 2026-07-28. Companion to
[`SPLASH-ANDROID-MIGRATION.md`](SPLASH-ANDROID-MIGRATION.md), which had this as
an optional Phase-1 backend. This document answers "how", and finds that the
Android answer is **structurally different** from Splash-OH's, not a port of it.*

Everything below is either read out of the NDK sysroot / the makepad tree on
this machine, or explicitly marked as reasoning. Nothing here has been measured
on device — §7 says what to measure first and why.

---

## 1. The finding that reshapes everything: Android has no NDK widget API

Splash-OH's entire renderer rests on one call:

```c
OH_ArkUI_GetModuleInterface(ARKUI_NATIVE_NODE, ArkUI_NativeNodeAPI_1, g_api);
g_api->createNode(ARKUI_NODE_TEXT);              // a real ArkUI widget, from C
```

`arkui/native_node.h` is a C header exposing 48 node types and a function-pointer
table (`createNode` / `setAttribute` / `addChild` / `registerNodeEvent`).
`shim.cpp` wraps it in ~15 flat `extern "C"` calls; `arkui/mod.rs` is safe Rust
over those. **No ArkTS is on the path.**

Android has no such thing. Checked against the NDK this repo actually builds
with (`android_33_macos_aarch64/ndk/28.2.13676358`) — **62 headers in
`sysroot/usr/include/android/`, and not one is a widget API**:

```
api-level asset_manager binder_* bitmap choreographer configuration crash_detail
data_space dlext fdsan file_descriptor_jni font font_matcher hardware_buffer*
hdr_metadata imagedecoder input input_transfer_token_jni keycodes legacy_*
log log_macros looper multinetwork native_activity native_window* NeuralNetworks*
obb performance_hint permission_manager persistable_bundle rect sensor
set_abort_message sharedmem* storage_manager surface_control* surface_texture*
sync system_fonts thermal trace versioning window
```

A `find` for any `*widget*` / `*view*` / `*ui*.h` across the whole sysroot
returns only libc++ and Linux UAPI noise. `native_window.h` and
`surface_control.h` are about **pixel surfaces** — a buffer you draw into — not
about widgets. That is precisely the API makepad already uses to get its GLES
surface.

> **`shim.cpp` has no Android translation.** There is no C entry point that
> creates an `android.widget.TextView`. Every Android widget is an ART object,
> and the only way to make one from Rust is JNI into the Java runtime.

This is confirmed by the absence of prior art: searching for Rust libraries that
build `android.widget.View` trees declaratively turns up nothing — the entire
Rust-on-Android ecosystem is *backend logic behind a Kotlin UI*, the inverse of
what we want.

---

## 2. What that does to Splash-OH's central argument

This is the part worth sitting with, because the OH conclusion **does not
transfer**, and the reason is structural rather than a matter of measurement.

Splash-OH measured Rust-NDK at **2.5–3× faster** than ArkTS-`typeNode`, and
identified exactly where the difference lives:

> `typeNode.createNode` does not just create the native node: it builds a JS
> wrapper, registers a finalizer, and wires up cross-language reference
> tracking. The collector then has to undo all of it.

So the win came from Rust being able to create the widget **without creating a
managed-language object at all**. ArkUI has two tiers — the native `TextPattern`
in libace, and an optional JS wrapper around it — and the NDK lets you use the
lower tier alone.

**Android has no lower tier.** A `TextView` *is* a Java object. `measure`,
`layout`, `onDraw`, the `Resources` lookup, the `Paint` — all of it lives in
ART. There is no native handle underneath it to address on its own.

That changes the comparison completely:

| | Splash-OH (ArkUI) | Splash-Android (JNI) |
|---|---|---|
| Rust path allocates | one native node | one **Java** object (via JNI) |
| Java/JS path allocates | one native node **+ a JS wrapper + finalizer** | one Java object |
| difference | the wrapper — real, and 2.5–3× | **none — same object, both paths** |
| Rust pays extra | nothing | **one JNI crossing per call** |

So the honest prediction — and it is a prediction, not a measurement — is that a
Rust-JNI Android widget backend lands **at best at parity with, and plausibly
slower than, building the identical tree in Java**. Both allocate exactly the
same Java objects; only the Rust path additionally pays a boundary crossing on
every `createNode` and every `setAttribute`.

Splash-OH's own retraction already points at this:

> **napi is not slow. Waiting behind a busy JS thread is slow.**
> …Bridge latency is a function of load, and building a widget tree *is* load.

On Android that lesson inverts into a design rule: **the thing to minimize is
crossings, not managed-language object creation** — because the managed objects
are not optional.

---

## 3. The constraint that actually decides the architecture: the UI thread

makepad's Rust does **not** run on the Android UI thread. `android.rs:1948`
spawns a render thread, attaches it to the JVM, and gives it its own EGL
context; `cx.os.render_thread_id` is tracked separately from
`activity_thread_id`.

Android `View`s are strictly UI-thread objects. That is why **`MakepadActivity.java`
carries 33 `runOnUiThread` marshals** — `spawnSystemBrowser`,
`updateSystemBrowser`, `expandComposer`, `collapseComposer`, the selection
handles, and the rest:

> Counted in `makepad/tools/cargo_makepad/…/MakepadActivity.java` (3 970 lines),
> which is **the tree that actually ships**: `strings $(which cargo-makepad)`
> resolves its baked `CARGO_MANIFEST_DIR` to
> `/Users/yuechen/home/octos-one/makepad/tools/cargo_makepad`. The `aichat/`
> checkout has a stale 2 865-line copy of the same file with 17 — reading that
> one understates the marshalling, and it is never compiled into an APK. This is
> the split-tree gotcha recorded in `BUILDING-ANDROID.md`. Some methods do also
> run directly when already on the UI thread; the marshal is what a
> render-thread caller pays.

```java
public void spawnSystemBrowser(final long browserId, final String url) {
    runOnUiThread(new Runnable() { @Override public void run() {
        WebView web = ensureSystemBrowser(browserId);
        ...
    }});
}
```

So "Rust builds the widget tree" cannot mean what it means on OpenHarmony, where
`mount()` runs on the ArkUI event thread and builds synchronously. Three ways to
resolve it, and they are the three architectures:

| | how | crossings for a 500-node tree | UI thread |
|---|---|---|---|
| **A** JNI-per-node | Rust holds `jobject`s, calls `new TextView` / `setText` / `addView` | **~3 000** | must marshal, or run the whole build inside one post |
| **B** serialize-once | Rust emits the `UiNode` tree as one buffer, Java walks it and builds | **1** | natural — Java builder runs on it |
| **C** overlay-only | makepad self-draws; native views only as overlays | ~5 | already how octos-one works |

---

## 4. Design A — the literal Splash-OH port, and its three sharp edges

Structurally this ports beautifully. `arkui/mod.rs`'s `Node` becomes a `jobject`
holder; `Node::new(ty)` becomes `new_object!`; `string_attr` becomes
`call_void_method!(… "setText" "(Ljava/lang/CharSequence;)V" …)`. makepad already
ships the substrate: `ndk_utils.rs`'s `call_method!` caches a `jmethodID` in a
per-call-site `static AtomicPtr`, so repeat calls skip `GetObjectClass`,
`CString` allocation and `GetMethodID` entirely.

Three things will bite, in order of how quickly they will:

**(1) The local reference table aborts at 512.** ART hard-crashes with
`JNI ERROR (app bug): local reference table overflow (max=512)`. Every
`new TextView(ctx)` and every `NewStringUTF` produces a local ref. A 500-node
tree with text produces well over 1 000 in a single native call — a guaranteed
abort, not a slowdown. Survivable only with disciplined `DeleteLocalRef`,
`PushLocalFrame`/`PopLocalFrame` per subtree, and promotion of retained handles
to global refs. Splash-OH's `Node` has no analogue of this problem because
`ArkUI_NodeHandle` is a raw pointer the GC knows nothing about.

**(2) `FindClass` from the render thread sees only the boot classloader.** The
render thread is attached with `AttachCurrentThread`, so `FindClass` resolves
against the system classloader, not the app's. `android.widget.*` is in the boot
classloader, so framework widgets work. **Any app-defined `View` subclass — which
is exactly what a themed widget kit is — will not resolve.** makepad sidesteps
this entirely today by only ever calling methods on the activity object it
already holds (`GetObjectClass` on an instance, never `FindClass` on a name).
Splash-Android would need to cache the app classloader at `JNI_OnLoad`, or route
class lookups through a Java helper.

**(3) Every call still has to reach the UI thread.** Marshalling per call is
absurd (a `Runnable` allocation per `setText`). The only workable form is one
`runOnUiThread` post that calls *back* into Rust to build the whole tree — at
which point Rust is running on the UI thread, and blocking it for the entire
build. That is exactly the failure Splash-OH catalogued as measurement defect #3:
*"benchmarking inside `mount()` held the JS thread long enough that its timer
queue stopped being serviced."*

---

## 5. Design B — serialize once, build in Java *(recommended)*

```
Splash DSL ─► splash-render ─► UiNode tree ─► flat buffer ─► ONE JNI call ─► Java builder ─► Views
             (VM, renderer-free)  (shared)     (Rust)                        (UI thread)
```

`splash-render`'s `UiNode` is a plain, backend-agnostic tree with a closed
attribute set, so it is *shaped* for serialization — but **it derives neither
`Serialize` nor `Deserialize` today** (`node.rs:137`), and an earlier draft's
"serializes trivially" overstated that into something that already exists. It
does not.

**What this design actually requires, none of which exists yet:**

- a wire format for `UiNode`;
- a **stable reconciliation key**. `Attrs::id` is documented as a *makepad widget
  name* (`node.rs:84`), not a universal identity, so it cannot carry diffing as-is;
- a mutation/delta protocol, and an event protocol in the return direction;
- rules for preserving focus, IME state, selection and scroll position across a
  delta;
- a Java-side ownership model for the built views.

And "one JNI call" is honest only for **initial construction**. Live cards still
cross for clicks, input, state changes, layout and overlay geometry, image
loads, errors and every incremental update. The claim is *one crossing per
build*, not one per card lifetime.

No line estimate is offered for the Java walker. A prior draft said "~300 lines";
that was invented. Mapping a tag to a class is not layout, measurement, scroll
behaviour, text styling, image loading, callbacks, state, theming or lifecycle —
`splash-makepad`'s translator is already 327 lines while covering a subset, and
it needed extra wrapper nodes merely to make container taps work.

Why this is the right shape on Android specifically:

- **1 crossing instead of ~3 000.** Given §2, crossings are the only cost the
  Rust path can actually control.
- **No local-ref pressure.** Java-created objects never enter the JNI local
  table.
- **Naturally on the UI thread**, with no per-call marshalling.
- **Diffing stays in Rust.** Reconcile two `UiNode` trees (cheap, no JNI) and
  ship only the delta — the thing that actually matters for octos-one, where
  nav rebuilds at 1 Hz.
- It is what Flutter's platform views and React Native's Fabric mounting layer
  both converged on, for the same reason.

What it gives up is the *headline* — "no Java in the loop". But §1 establishes
that headline was never available on Android. Trading an unattainable claim for
a 3 000× reduction in boundary traffic is not a compromise.

---

## 6. The widget mapping — and the build-system wall behind it

`splash-render`'s 23 `NodeKind`s against Android:

| NodeKind | Android widget | in `android.jar`? |
|---|---|---|
| Column / Row | `LinearLayout` (VERTICAL / HORIZONTAL) | ✅ |
| Stack | `FrameLayout` | ✅ |
| Scroll | `ScrollView` | ✅ |
| Grid | `GridLayout` | ✅ |
| Text | `TextView` | ✅ |
| Image | `ImageView` | ✅ |
| Button | `Button` | ✅ |
| Toggle | `Switch` | ✅ |
| Checkbox | `CheckBox` | ✅ |
| Radio | `RadioButton` | ✅ |
| Slider | `SeekBar` | ✅ |
| Progress / Loading | `ProgressBar` (determinate / indeterminate) | ✅ |
| Input / Textarea | `EditText` (`inputType` differs) | ✅ |
| DatePicker / TimePicker | `DatePicker` / `TimePicker` | ✅ |
| TextPicker | `NumberPicker` | ✅ |
| **List** | `RecyclerView` | ❌ androidx |
| **Waterflow** | `RecyclerView` + `StaggeredGridLayoutManager` | ❌ androidx |
| **Refresh** | `SwipeRefreshLayout` | ❌ androidx |
| **Swiper** | `ViewPager2` | ❌ androidx |

**The wall:** `cargo_makepad/src/android/compile.rs:1155-1180` runs `javac` with
`-classpath android.jar` and nothing else, then `d8`. **There is no Gradle, no
AAR resolution, no androidx, no Material Components.** (A prior draft said "11
hand-listed `.java` files"; the `java_sources` vec at `:1097` actually has **14**
entries — 12 named sources plus the generated `R.java` and the app's own
java/xr files. The count was wrong; the `-classpath android.jar` point that
carries this section is not.) So a v1 can cover 19 of 23 kinds on framework widgets alone; the
four scrolling/paging kinds and every Material-themed control require teaching
cargo-makepad to resolve and merge AARs (classes.jar + resources + R.txt) — a
real build-system project, not a flag.

Three further gaps with no clean answer:

- **Auto-sizing inverts.** Splash-OH depends on ArkUI nodes *not* auto-sizing:
  *"the DSL states every width and height explicitly … Rust knows the geometry at
  build time and does not have to wait for a layout pass."* That is what lets it
  cut a webview hole. Android views auto-size (`WRAP_CONTENT` + a measure pass on
  the UI thread), so hole geometry is only known *after* layout — a round trip
  the OH design never needs.
- **`List` is virtualized, `UiNode` is not.** `RecyclerView` needs an `Adapter`
  and a recycling contract; a static tree of children is the one thing it is
  built not to be. This needs a real design, not a mapping row.
- ~~**The custom widgets do not cross.**~~ **RETRACTED — this was wrong, and it
  was wrong because I reasoned from "there is no `android.widget.*` with this
  name" instead of reading the sources.** All three have now been ported to real
  Android views and are running on the device; see
  `scratchpad/cat/app/src/main/java/dev/splash/catalog/`.

  | makepad widget | Android port | how |
  |---|---|---|
  | `WeatherIcon` (142-line SDF shader) | `WeatherIconView` | every SDF primitive has an exact Canvas counterpart — `sdf.circle`→`drawCircle`, `sdf.box(x,y,w,h,r)`→`drawRoundRect`, `sdf.rotate(a,cx,cy)`→`Canvas.rotate`, `move_to/line_to/close_path`→`Path`, `sdf.stroke`→`Paint.STROKE`. All **8 conditions**, same geometry, same colours, animated off `postInvalidateOnAnimation` instead of `draw_pass.time`. |
  | `MapView` nav rendering | `NavMapView` | the pinhole ground-plane projection transcribed from the vertex shader and evaluated per-vertex on the CPU: `z_cam = a·cosP + h·sinP`, `y_cam = a·sinP − h·cosP`, `ndc = (cross/(z·tanH), y_cam/(z·tanV))`, plus the shader's own haze `pow(clamp(a/far,0,1), 2.6)·0.9`. Route ribbon, standing pins, vehicle puck and all three `nav_mode`s (flat / 3D chase / heading-up 2D). |
  | `glass.Panel` family (`AppleGlassRoundedView`) | `GlassPanelView` | one Canvas pass per uniform — tint+surface fill, lensing rim (`RadialGradient` stroke), specular sweep (`LinearGradient`), chromatic diffraction edge, hairline border, `setShadowLayer`. `blur_level` uses `RenderEffect` on API 31+; everything else renders on SDK 30. |

  **What is actually true**: a shader is not a widget, so there is no *widget* to
  map to — but the *drawing* is ordinary 2D geometry, and `Canvas` draws it. The
  real cost is per-widget porting work, not impossibility. Two things genuinely
  did not survive the port: the GPU does the SDF per-pixel where Canvas does it
  per-primitive (fine at these sizes, would matter at thousands), and the
  original's vertex shader **cannot drop a vertex**, so it snaps behind-camera
  points to an off-screen curtain — on the CPU I clip the segment at the near
  plane instead, which is strictly better and is what fixed the streaking
  horizon in the first attempt.

  Note separately that Splash-Makepad's fork-free `script_mod!` theming trick
  *is* makepad-specific — Android theming is XML styles/attrs, an unrelated
  mechanism. That part of the original claim stands.

---

## 7. What to build, and what to measure first

**Phase N1 — settle the question before building on it.** Build the smallest
honest benchmark: one screen, ~200 framework-widget nodes, three arms —
(A) JNI-per-node from Rust, (B) serialize-once + Java builder, (J) the same tree
built directly in Java. Assert node counts at runtime on every arm, every run.

Splash-OH's own list of six measurement defects is the checklist here, and two of
them are live risks in this exact experiment: *"two arms building different
things"* and *"a blocked event loop"* (arm A blocks the UI thread by
construction, so it will distort anything measured around it). Its conclusion
applies verbatim:

> **Every one was caught by a sanity check, not by the benchmark.**
> Build the sanity check. The number will look fine either way.

**What this benchmark cannot settle.** It compares boundary overhead on static
construction, and nothing else. It does not exercise streaming source, nav's
in-place `ui.*.set_*` tick updates (`splash.rs:2633` — the card deliberately
avoids rebuilding the map), IME/focus retention, WebView geometry, image
loading, activity recreation, memory, jank, or event latency. Node count alone
will also miss dropped properties and wrong interaction behaviour — so pair it
with a property-level diff, not just a count. **Treat its result as a bound on
one cost, not as validation of a product direction.**

The prediction from §2 is that **J ≈ B < A**. If that holds, Design A is
finished as a product direction and survives only as a benchmark artifact — and
the honest write-up is *"the OH result does not generalize, here is why"*, which
is a genuinely valuable companion to `Splash-OH/CONCLUSION.md`.

**Phase N2 — `splash-android-view` as a real backend (Design B).**

```
crates/splash-android-view/     UiNode -> flat buffer                rlib
android/SplashViewBuilder.java  buffer -> android.widget.* tree      ~300 lines
```

Same `UiNode` as `splash-makepad` and Splash-OH's ArkUI walker — three backends,
one core, which is the whole point of the split. Ship the 19 framework kinds
first; leave `list`/`waterflow`/`refresh`/`swiper` unmapped and **log what was
dropped** rather than silently substituting a `ScrollView`.

**Phase N3 — androidx, only if the four kinds justify it.** AAR resolution in
cargo-makepad. Scope this against actual card needs: today's cards use
`Label` `View` `RoundedView` `Button` `WeatherIcon` `SolidView` `Filler`
`CircleView` `MapView` `TextInput` `GradientYView` `Image` — **not one of them is
a `RecyclerView`-shaped list.** This phase may simply never be needed.

**Phase N4 — the hybrid stays.** Whatever happens above, `MapView`, the shader
widgets and the glass panels remain makepad-drawn, and the WebView remains an
overlay. Splash-Android ends up with three coexisting render paths, addressed
from one DSL — which is a more honest description of the platform than any
single-backend story.

---

## 7b. "Can we work with the NDK without Java overhead?"

Yes — completely. But what pure NDK gives you is a **surface**, not **widgets**,
and a self-drawn UI stack on a surface is precisely what makepad already is. So
the question is really *"how much native maturity is reachable with zero Java?"*
— and the answer is **more than octos-one currently takes**.

### The zero-Java stack that exists today (verified in the sysroot)

| capability | NDK API | since |
|---|---|---|
| surfaces + SurfaceFlinger composition | `ASurfaceControl` + 36 `ASurfaceTransaction_*` (position, scale, crop, z-order, alpha, buffer, visibility, frame rate, damage region, reparent — applied atomically) | 29 |
| vsync | `AChoreographer` | 24 |
| input | `AInputQueue` / `AMotionEvent` / `AKeyEvent` (`input.h`) | 9 |
| **system fonts + fallback** | `ASystemFontIterator`, `AFont_getFontFilePath`, `AFontMatcher_match` | 29 |
| image decode | `AImageDecoder` | 30 |
| assets, GPU buffers, sensors, tracing, perf hints | `AAssetManager`, `AHardwareBuffer`, `ASensor`, `ATrace`, `APerformanceHint` | — |

### What is structurally absent, and cannot be worked around

- **Widgets** — §1.
- **Accessibility.** Grepping all 62 headers for `accessibility` returns nothing.
  This is the single biggest item on §9's "native maturity" list, and it is
  Java-only. AccessKit remains the answer, and it goes through JNI.
- **The IME.** `ANativeActivity_showSoftInput` / `hideSoftInput` exist, but only
  on a `NativeActivity` (makepad uses a normal `Activity` +
  `MakepadInputConnection.java`), and the header says outright that they call
  `InputMethodManager.showSoftInput()` — Java underneath, merely wrapped.
- **Text layout.** `AFontMatcher` does *itemization* — which font covers this
  run, and how long the run is. Not shaping, not line breaking, not bidi.

### The actionable part: two maturity wins available with zero Java

makepad currently touches **four** NDK API families — `AAssetManager`,
`AChoreographer`, `AHardwareBuffer`, `ANativeWindow`. It uses neither
`ASurfaceControl`, nor `AFontMatcher`, nor `ASystemFontIterator`, nor
`AImageDecoder`.

**(1) System fonts and fallback (`AFontMatcher`, API 29 — works on the SDK-30
device).** makepad ships its own fonts: `widgets/resources/` carries IBMPlexSans,
LXGWWenKai (CJK), NotoColorEmoji, JetBrains Mono, Liberation Mono, fa-solid — and
the cards name them explicitly
(`crate_resource("makepad_widgets:resources/Roboto-Medium.ttf")`). `AFontMatcher`
hands over the OS's real font set *and* its fallback chain, including per-locale
disambiguation — the header's own example shows `zh-CN,ja-JP` and `ja-JP,zh-CN`
selecting different fonts for the *same* codepoint. That is exactly the thing
that makes a self-drawn UI read as foreign on a Chinese or Japanese phone, and it
is reachable without a single JNI call. It would also let the bundled CJK and
emoji faces come out of the APK.

**(2) Native composition (`ASurfaceControl`, API 29).** Every overlay reposition
today goes Rust → JNI → `runOnUiThread` → `setLayoutParams` (`updateSystemBrowser`
and its 32 siblings). For surfaces **makepad itself owns** — the video texture,
the camera preview, any additional drawn layer — `ASurfaceControl` composites
them natively: create, reparent, z-order, crop, per-surface frame rate, all
applied atomically in one transaction with no UI-thread hop. *This does not help
the WebView*, which is a Java `View` in the Java hierarchy and stays there; the
win is scoped to makepad-owned surfaces.

### Two caveats worth pinning down before relying on any of this

- **The device is SDK 30** (`ro.build.version.sdk` = 30 on `bf0a4730`).
  `AInputReceiver` — native input delivered straight to a `SurfaceControl`, which
  would be the last piece of a fully native input path — is **API 35**. Not
  available here.
- `ASurfaceTransaction_*` spans `__INTRODUCED_IN` 29 / 30 / 31 / 33 / 36. Any use
  needs per-level guards, not a blanket dependency.

**Net:** pure-NDK is not a route to native *widgets*. It is a route to **the
inputs** for two of the things native widgets were wanted for — system font
*selection* and cheaper compositing — with no Java on the path. Accessibility
still requires JNI.

**Do not read the font item as "correct system typography."** An earlier draft
did, and that contradicts this section's own limitation two paragraphs up:
`AFontMatcher` gives itemization only. Correct typography additionally needs
shaping, bidi, line breaking, and colour-emoji handling — and on the makepad
side, a font-loading path that can take arbitrary system font *file paths*, an
atlas/cache strategy for them, and an APK/resource plan for what gets dropped.
**None of that stack has been inspected**, so "a self-contained change to
makepad's font stack" was an assertion, not a finding. What is established is
that the *API* is available at API 29 and unused. Sizing the work needs a read
of makepad's font and shaping code first.

---

## 7c. The recommended approach, concretely

The review of §4–§6 established that the earlier framing asked the wrong
question. "How do I build a View tree from a `UiNode`" is the easy half.
**Construction is a one-time cost; the update path is the design.** Cards stream
as the LLM emits them, re-evaluate on `fn tick()`, and — in nav's case —
deliberately mutate in place via `ui.*.set_*` to avoid rebuilding the map. Any
approach that only knows how to build a tree from scratch cannot run the cards
octos-one already ships.

So the deliverable is a **protocol**, and the widget mapping is a detail hanging
off it.

### The decision that makes the whole thing tractable

**Java owns the `View` objects. Rust owns integer ids. Rust never holds a
`jobject`.**

Java keeps a `SparseArray<View>` keyed by `u32`; Rust keeps the `UiNode` tree and
the same ids. This single choice removes, at a stroke:

- the **512 local-reference abort** (§4.1) — no `jobject` ever enters a JNI local
  table on the Rust side;
- global-ref lifetime management and the leak/`DeleteGlobalRef` discipline;
- the **`FindClass` classloader trap** (§4.2) — Java resolves its own classes, so
  app-defined `View` subclasses (i.e. any themed kit) work normally;
- most of the UI-thread hazard, because the code touching Views is already Java.

Everything else in this section follows from it.

### The protocol

**Rust → Java: a batched mutation op-list, not a tree dump.**

```
Create(id, kind)              Insert(parent, child, index)
SetAttr(id, attr, value)      Remove(id)
                              Reorder(parent, order)
```

Rust diffs the previous `UiNode` tree against the new one and emits ops. This is
the React-Fabric / DOM-diff shape, and it is the part that makes streaming and
`tick()` cheap: a card whose temperature label changed emits **one** `SetAttr`,
not a rebuild.

**Identity** is the hard sub-problem, and `Attrs::id` cannot carry it — it is
documented as a makepad widget name (`node.rs:84`). Use **structural path plus an
optional explicit `key`**: the path handles the common case, and `key` is what
conditionals and list rows need in order not to be destroyed and rebuilt on every
re-eval. Adding `key` to `Attrs` is a small change to `splash-render` that the
ArkUI backend wants anyway.

**Encoding:** a flat binary op buffer in a `NewDirectByteBuffer`, with strings in
a side UTF-8 blob addressed by (offset, len) in the same buffer. Rust writes, Java
reads, no copy. JSON is fine for the first prototype and wrong for a per-frame
path.

**Scheduling:** one crossing per *frame*, not per op — and invert the direction to
avoid a `Runnable` allocation per frame. Register a `Choreographer.FrameCallback`
on the UI thread that calls into Rust to drain the pending op buffer (behind a
mutex-protected double buffer) and applies it. The initiating call is then already
on the UI thread, and the batch is naturally frame-synced.

**Java → Rust: individual calls, no batching needed.** `onClick(id)`,
`onTextChanged(id, text)`, `onScroll(id, y)`. These are low-frequency, and the
JS→native direction is the cheap one. This channel is also where `agent.notify`
has to land — it is the card event protocol
(`SPLASH-ANDROID-MIGRATION.md` §1), not an afterthought.

### Get there incrementally — starting where the value actually is

Do **not** start by reimplementing the whole tree. octos-one already hosts native
Android views over the GL surface, and that mechanism is proven in production:
`draw_walk` rect → `CxOsOp` → `runOnUiThread` → `setLayoutParams`, with a
`Spawn`/`Update`/`Detach`/`Close` lifecycle (`web_card.rs`, `cx_api.rs:390-425`).

**Step 1 — one hosted native node.** Add `{t:"native", widget:"…"}` to
`splash-render` and render it exactly like `WebCard`: makepad reserves the rect,
one Android `View` sits in it. No reconciler, no op protocol, no diffing.

Start with the widgets where native genuinely beats makepad, which is a much
shorter list than "all of them":

| widget | why native wins |
|---|---|
| `EditText` | real IME, i18n input, autofill, selection handles — makepad's `TextInput` is shader-drawn and adb cannot even type into it |
| `DatePicker` / `TimePicker` / `NumberPicker` | system behaviour and locale for free |
| `WebView` | already done — this is the existence proof |

That is a genuinely useful feature on its own, ships without any of §4–§6, and
teaches you the ownership and event plumbing on a one-view surface.

**Step 2 — a native subtree.** Let `{t:"native"}` carry children, and build them
with the op protocol above. Now you need identity, diffing and batching — but on a
bounded subtree, with the makepad path still rendering everything around it and
still available as a fallback.

**Step 3 — full backend, only if Step 2 pays.** `splash-android-view` as a peer
of `splash-makepad` behind the same `UiNode`, on the 19 framework kinds (§6).
Keep the three-arm benchmark honest about what it does *not* measure (§7).

The staging matters because each step is independently shippable and each one
answers the question the next one depends on. §4–§6 as originally written asked
you to build all three at once and find out at the end.

### Correction: Steps 1–2 and Step 3 are two architectures, not three steps

The list above implies a smooth progression. It is not one, and the difference is
**who hosts whom**:

| | host | native widgets are | makepad is |
|---|---|---|---|
| **Steps 1–2** | **makepad** | overlays in rects makepad's layout computed | on the critical path, drawing everything else |
| **Step 3** | **Android** | the entire tree | **absent from that card** |

So: *yes*, Steps 1–2 leverage makepad to wrap the native widget — that is exactly
the `WebCard` mechanism generalised from "one `WebView`" to "any `View` kind":
`draw_walk` reserves a rect → a `CxOsOp` carries `(id, kind, rect, attrs)` → Java
creates or repositions the View in the overlay stack → events return as actions.
Makepad owns layout; the native view is a guest in a rectangle.

Step 3 is the opposite arrangement and shares only the op protocol. A card
rendered by `splash-android-view` is Android Views all the way down, with real
`measure`/`layout` and no GL surface involved — the Splash-OH shape. The two do
not compose: you pick one per card.

**The op protocol, identity scheme and event channel are common to both.** The
hosting model, the layout owner, the coordinate system and the z-order story are
not. Budget them separately.

### Correction: makepad-free is feasible, and two projects were being conflated

The framing above steers toward the hybrid. That steer was partly produced by a
mistake: **objections to *migrating octos-one's existing cards* were used as
objections to *building a makepad-free Splash-Android***. They are different
projects, and the second is not blocked by the first.

Splash-OH is the proof. It does not render octos-one's cards either — it renders
its own `catalog.splash`, `weather.splash`, `youtube.splash`, authored in the
plain-data DSL against the widgets its backend supports. A Splash-Android built
the same way is feasible in exactly the same sense: demonstrably, because the
OpenHarmony sibling exists and ships.

**The stack, with no makepad renderer anywhere:**

```
.splash ─► splash-core (VM) ─► splash-render ─► splash-android-view ─► Java builder ─► android.widget.*
           vendored makepad-script  UiNode        op buffer              SparseArray<View>
```

`splash-render` depends on `makepad-script` and nothing else — "renderer-free by
construction", per its own crate doc — and `makepad-script`'s own dependencies are
`error_log`, `math`, `live_id`, `script-derive`, `smallvec`, `regex`, `html`. No
`makepad-platform`, no `makepad-draw`, no `makepad-widgets`. So "without makepad"
is accurate for the **renderer**; the **language VM** still comes from makepad's
lineage, exactly as it does in Splash-OH. Nothing about that is a compromise —
it is the same boundary that repo already runs on.

**Two of my stated obstacles dissolve in this arrangement:**

- **The androidx wall (§6) is an artifact of cargo-makepad**, whose `javac` runs
  with `-classpath android.jar` and no dependency resolution. A standalone
  Splash-Android is its own repo with its own **Gradle** project — the direct
  analogue of Splash-OH's `deveco/` — and there `androidx` and Material are one
  line in `build.gradle`. All 23 `NodeKind`s become mappable, including
  `RecyclerView`, `ViewPager2` and `SwipeRefreshLayout`.
- **`MapView` / `WeatherIcon` / the glass panels are a *migration* constraint,
  not a *feasibility* one.** They block porting octos-one's nav and weather cards
  to this backend. They do not block the backend existing, any more than the
  absence of `ARKUI_NODE_WEB` blocked Splash-OH.

**What genuinely does not transfer, and should not be claimed:**

- **The performance argument (§2).** Rust-JNI will not beat Java-direct; both
  allocate the same Java objects. Build this for what it renders, not for speed.
- **Shader widgets, custom vector-tile rendering, GPU animation** — gone, by
  construction. Anything wanting them stays on makepad.
- **Cross-platform uniformity** — this backend is Android-only.

**What is genuinely gained, and is not reachable any other way:**

- **Accessibility.** TalkBack works on real `View`s for free. This is the one
  item on §9's native-maturity list with no self-drawn substitute — AccessKit
  approximates it by *describing* a self-drawn tree; real Views simply *are* the
  tree. If accessibility is the goal, this approach is not merely defensible, it
  is the correct one.
- **Real IME, i18n text, autofill, selection** — the same argument, on the
  widgets makepad is weakest at.
- **A platform look that follows OS updates** without any work.
- **A much smaller artifact** than shipping a GPU engine.

**How to build it, given all of the above:** start with Splash-OH's own model —
build the whole tree per screen, no diffing — because that is what its cards do
and it is enough to get the thing on screen. Keep the ownership rule (Java owns
Views, Rust owns ids), and add the op-diff protocol only when a card needs
in-place updates. That inverts the earlier staging: the makepad-free backend is
the *first* thing built, not the third.

### Which one is the destination *for octos-one specifically*

Steps 1–2 look like a stepping stone and are probably the answer:

- They deliver the native-maturity wins that motivated the question at all — IME,
  i18n text input, system pickers — and those are precisely the widgets makepad
  is *worst* at.
- They compose with everything already shipping. `MapView`, `WeatherIcon`, the
  glass panels and the gradients keep working, because makepad still draws them.

Step 3's benefits need to be stated before it is worth building, and the obvious
candidates do not survive contact with §2 and §6:

| claimed benefit | status |
|---|---|
| performance | **no** — §2 predicts Java-direct ≥ Rust-JNI; both allocate the same Java objects |
| cross-platform | **no** — it is an Android-only backend by construction |
| the platform look | partly, and only for the 19 framework kinds; the four scrolling/paging kinds need the androidx work in §6 |
| **accessibility** | the one real answer — and AccessKit reaches it from the makepad tree without native widgets at all |

> **A third arrangement, noted but unverified.** Android can also host makepad —
> a `SurfaceView` as a *child* inside a native View tree — which would let
> `{t:"custom", widget:"MapView"}` be a small makepad surface embedded in a Step-3
> Android hierarchy, solving "MapView cannot cross". Caveats are serious and
> unmeasured: multiple GL surfaces are expensive, `SurfaceView` z-ordering inside
> a View tree is notoriously painful (`setZOrderOnTop` / punch-through), and
> makepad's architecture assumes a single window and surface. **Speculative** —
> do not plan on it without a spike.

### What has to change in `splash-render` either way

Small, well-scoped, and useful to the ArkUI backend too:

- a `Native`/`Custom` node kind that `NodeKind::from_tag` does **not** drop
  (`node.rs:37` currently discards unknown tags before any backend sees them);
- a `key` attribute for reconciliation identity;
- `Serialize`/`Deserialize` derives — absent today (`node.rs:137`);
- a documented delta + event contract, which is the actual gate from
  `SPLASH-ANDROID-MIGRATION.md` §6.

---

## 7d. Validated on device (2026-07-28, OnePlus 6T, SDK 30)

Everything above §7c was reasoning. This section is what a working APK actually
did. Probe source: `scratchpad/sap/` — `rust/src/lib.rs`, `java/dev/splash/probe/`.

**The stack under test was the real one**, not a mock:

```
CARD (.splash, with a fn + a while-loop)
  -> splash-render 0.1.0  (real crate, path dep on ~/Splash-Makepad)
  -> makepad-script e1c2164b  (the VM, upstream makepad dev)
  -> UiNode tree
  -> flat binary buffer  -> ONE JNI call (direct ByteBuffer)
  -> Java builder -> android.widget.*
```

No makepad-platform, no makepad-draw, no makepad-widgets, no GL surface, no
`Splash` widget, no androidx. `javac -classpath android.jar` only.

### Results

| claim | result |
|---|---|
| `splash-render` cross-compiles to `aarch64-linux-android` | ✅ clean, 8.35 s |
| the VM evaluates Splash DSL **on device** | ✅ `splash-render OK: 23 nodes, root=Scroll` |
| the tree is *computed*, not a literal | ✅ the `while` loop's "computed row 0/1/2" rendered |
| every node becomes a real `android.widget.*` | ✅ `built ok=23 failed=0` |
| one JNI crossing carries the whole tree | ✅ direct `ByteBuffer`, 240-byte string blob |
| Java owns Views, Rust holds only ids | ✅ `SparseArray<View>`; no `jobject` in Rust, so the 512-local-ref abort is structurally unreachable |
| framework widgets only, no androidx | ✅ nothing outside `android.jar` |

**Widgets confirmed rendering:** `LinearLayout` (V+H), `ScrollView`, `TextView`,
`Button`, `CheckBox`, `RadioButton`, `Switch`, `EditText`, `SeekBar`,
`ProgressBar` (determinate **and** indeterminate), `NumberPicker`, `DatePicker`
(full calendar, correct system date).

### The two claims that mattered, both confirmed

**Accessibility is real, and it is free.** `uiautomator dump` returns a fully
populated tree — genuine class names *and* genuine semantics:

```
class="android.widget.CheckBox"  checkable="true" checked="true" clickable="true" focusable="true"
class="android.widget.Switch"    checkable="true" checked="true" clickable="true" focusable="true"
class="android.widget.EditText"  focusable="true" clickable="true"
class="android.widget.SeekBar"   focusable="true"
```

That is exactly what TalkBack consumes. Nothing was written to produce it. This
is the benefit with no self-drawn substitute, and it is now demonstrated rather
than argued.

**The IME is real.** Tapping the `EditText` raised the system keyboard
(`mInputShown=true`), `adb shell input text` typed into it, and the text appeared
with autocorrect suggestions in the bar. Contrast the makepad path, where adb
cannot type into `TextInput` at all.

### Honest limits of this probe

- **Construction only.** No diffing, no updates, no `tick()`. The op protocol in
  §7c is still unbuilt and unvalidated.
- **`list` / `grid` / `waterflow` / `refresh` / `swiper` were not exercised** —
  they hit `default: return null` in the builder. The androidx gap is still an
  assumption here, not a measurement.
- **Layout mapping is crude**: height from the DSL, `MATCH_PARENT` width, and
  `spacing`/`margin`/`radius`/`align` unimplemented. Real layout is more work
  than this probe shows — the review's point stands.
- **No timing claim.** ~69 ms elapsed between the two log lines covering VM eval
  and 23-view construction, but that is one uninstrumented sample on one screen,
  not a benchmark. §7's three-arm comparison is still the thing to build.

### What this changes

The feasibility question is closed: **Splash DSL → native Android widgets, with
no makepad renderer, works.** A card authored in the plain-data DSL drives real
Android views with real accessibility and real text input, in a 1.0 MB APK.

What remains genuinely open is the *update path* (§7c) and the *layout
semantics* — not whether the approach is possible.

---

## 8. The one-paragraph answer

Splash-OH renders Splash to native ArkUI widgets because OpenHarmony ships a C
NDK for widget construction, and because ArkUI has a native tier beneath its
managed wrapper that the NDK can address alone. Android has neither: no widget
NDK exists, and `android.widget.*` has no sub-Java tier, so a Rust-built widget
tree must construct the same Java objects a Java builder would while
additionally paying a JNI crossing per call and a `runOnUiThread` marshal per
build. The Splash-OH architecture therefore does not port; what ports is the
`UiNode` seam. **Build the Android backend as serialize-once-then-build-in-Java
(Design B), on framework widgets, behind the same `UiNode` the ArkUI and makepad
backends already share — and run the three-arm benchmark first, because if
Java-direct beats Rust-JNI the way §2 predicts, that result is itself the most
useful thing this work produces.**
