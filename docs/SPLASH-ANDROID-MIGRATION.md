# Migrating octos-one to ymote/Splash, and standing up `Splash-Android`

*Plan, 2026-07-28. Supersedes the forward-looking half of
[`SPLASH-NATIVE-INTEGRATION.md`](SPLASH-NATIVE-INTEGRATION.md) §8–§10, which
predates both `ymote/Splash-OH` and `ymote/Splash-Makepad`.*

---

## 1. What octos-one renders today

Eight app cards, two render paths, one dispatch point
(`aichat/widgets/src/markdown.rs:1285-1288` — the fence language picks the path).

| app | spec | source | render path |
|---|---|---|---|
| `weather` | `app.md` + 4 style exemplars | LLM-generated | `runsplash` → makepad |
| `stock` | `app.md` + `lint.json` | LLM-assembled | `runsplash` → makepad |
| `news` | `app.md` + `lint.json` | LLM-assembled | `runsplash` → makepad |
| `activity` | `app.md` + `lint.json` | LLM-assembled | `runsplash` → makepad |
| `weather-activity` | composed spec + `lint.json` | LLM-composed | `runsplash` → makepad |
| `nav` | `app.md` + `trip-planner.splash` (~14 KB) | **direct-serve, verbatim** | `runsplash` → makepad + native `MapView` |
| `youtube` | `app.md` (contract only) | LLM-generated HTML | `runhtml` → `WebCard` → Android WebView |
| `web` | `app.md` (contract only) | LLM-generated HTML | `runhtml` → `WebCard` → Android WebView |

**Path A — `runsplash`.** `Splash` widget → `vm.eval()` → `View::script_from_value(vm, value)`
→ makepad's own widget tree, self-drawn on GLES. Cards are written in **makepad
widget syntax**, resolved through makepad's widget registry:

```
SolidView{ width: Fill height: 1560 draw_bg.color: #0f0f0f
    Label{ text: sys.weather(31.23, 121.47, "current.temperature_2m") + "°"
           draw_text.text_style.font_size: 37 } }
```

Widget surface actually used across every card: `Label`(242) `View`(155)
`RoundedView`(63) `Button`(57) `WeatherIcon`(32) `SolidView`(27) `Filler`(14)
`CircleView`(13) `MapView`(6) `TextInput`(4) `GradientYView`(4) `Image` `Card`
— plus the value types `Align` `Inset` `TextStyle` `FontFamily` `FontMember`.

**Path B — `runhtml`.** `WebCard` (`aichat/widgets/src/web_card.rs`, 747 lines) →
`SystemBrowser` `CxOsOp` → a real `android.webkit.WebView` overlaid on the GL
surface. Cards get the `octos.*` JS kit injected (4 files: core / media /
finance / weather) and a `splash.invoke`-style bridge gated by a static
`HTTP_FETCH_ALLOWED_HOSTS` allowlist + SSRF guard.

**Host capability surface: 32 registered methods**, in
`aichat/widgets/src/splash.rs` (2612 of its 2705 lines) — but they are **two
separate globals**, and conflating them is a real planning error:

```
sys.*  (31)  airmap airquality aqinum basemap coord geocode geocodenum gps
             mappin maptile movers navroute navroutenum navsecs navstep
             navstepnum news photo places placesnum route satellite
             satellite_ir search searchnum simsecs stock stockbar stockrange
             weather weathernum
agent.* (1)  notify
```

`agent` is its own injected global (`set_injected_global(id!(agent))` at
`splash.rs:114`, against `id!(sys)` at `:1248`), and `agent.notify` is **not a
data helper — it is the card event protocol**. It posts `SplashAction::Notify`,
which the app dispatches into application behaviour. `trip-planner.splash` calls
it **52 times**; `navigate.splash` 3; every `lint.json` state rule
(`key: "selected"`, min 11) is asserting on it. Any capability plan that ports
`sys.*` and stops has silently dropped all card interactivity.

---

## 2. How Splash-OH uses ymote/Splash

One `.so` (ArkTS loads exactly one), dependency strictly one-directional.

> **Stale-source correction.** An earlier draft of this section described a
> two-crate workspace, read from a GitHub clone. The working tree at
> `~/Splash-OH` is at `67ae6e2` (2026-07-28) and has **four**:
> `splash-oh-native`, **`splash-oh-core`**, `splash-oh-plugin-demo`,
> `splash-oh`. The commit that added them is *"Split out splash-oh-core and a
> tool registry, so a capability can live outside the bridge"* — i.e. Splash-OH
> is already moving toward the registry model Phase 3 wants, which is worth
> tracking rather than re-deriving.

```
crates/splash-oh-native/       rlib     the renderer
crates/splash-oh-core/         rlib     the tool registry
crates/splash-oh-plugin-demo/           an out-of-bridge capability
crates/splash-oh/              cdylib   the bridge — napi-ohos
deveco/                                 the ArkTS shell
```

### It uses `splash-core` two different ways, deliberately

| caller | entry point | why |
|---|---|---|
| **trusted app DSL** (`dsl.rs`) | `splash_core::vm` — the raw `ScriptVm` | full VM speed, host globals via `set_injected_global`; `splash_core::check_syntax` used **only** to turn "evaluated to nil" into a real diagnostic |
| **untrusted page script** (`bridge.rs:1537`) | `splash_core::Runtime` with tightened `ExecutionLimits` | a web page is the least-trusted thing in the process; bounded source / heap / instructions / deadline, fresh `Runtime` per call |

`splash-core` re-exports the VM verbatim (`pub use makepad_script as vm;`), so
`dsl.rs` imports it as `use splash_core::vm as makepad_script` — *"what changes
is provenance, not API."*

### The DSL is plain data, not makepad widget syntax

```
{t: "column", bg: 0xFFF2F2F7, c: [
    {t: "text", text: "Components", size: 28, weight: 7},
    {t: "button", label: "Tap me", h: 44},
]}
```

`dsl.rs` says why in as many words: *"Deliberately plain data rather than
makepad's `Button{...}` component syntax: that syntax resolves through makepad's
widget registry, which is exactly the coupling this repo exists to avoid."*
Everything above that layer — `let`, `fn`, loops, conditionals, string building
— still works, because a real VM evaluates it.

### Webview: a hole cut in the native tree

There is no `ARKUI_NODE_WEB` (all 48 NDK node types checked), so Rust builds a
**transparent placeholder** of the exact size, records the geometry in
`webslot.rs`, and ArkTS puts a real `Web` at those coordinates in a `Stack`
above the `ContentSlot`. This works only because native ArkUI nodes don't
auto-size, so the DSL states every width/height explicitly and Rust knows the
geometry at build time.

Trust is derived from the **source**, not a flag: generated HTML gets the
bridge, a remote URL never does. `arkweb.rs` `dlopen`s
`OH_NativeArkWeb_RunJavaScript` so Rust can evaluate straight into the page —
probed at runtime, with the ArkTS relay as fallback (on the test device the
symbol resolves but the controller's web tag never binds).

> Note the direction of travel: `bridge.rs` opens with *"Ported from the
> substrate in octos-one (`makepad.ets` + `widgets/src/web_card.rs`), which
> reached this design first."* The split-call promise pattern, the
> stringified call ids, the capability gate — Splash-OH took all of it from
> here. This migration brings the refined version home.

One caveat worth recording: `{t:"web"}` is described in `webslot.rs`'s docs but
the DSL walker has **no** `"web"` case. Web slots are built by Rust app builders
(`webslot::web` / `web_html`, used by `browser.rs`, `files.rs`, `native.rs`,
`weather_web.rs`). Wiring it through the walker is small and not yet done.

---

## 3. The thing that changes this plan: `Splash-Makepad` already exists

`ymote/Splash-Makepad` (public, updated 2026-07-28) already ships the render
pipeline this migration would otherwise have to build:

```
Splash DSL ──► splash-render ──► UiNode tree ──► splash-makepad ──► makepad dialect string
{t:"column"}    (VM, renderer-free)  (backend-agnostic)   (pure translation)   View{…}/Label{…}
                                                                                    │
                                                          makepad `Splash`.set_text() ▼ native widgets
```

| crate | lines | what |
|---|---|---|
| `splash-render` | 392 | DSL → `UiNode`; **the only** makepad-script dependency in the render path |
| `splash-makepad` | 327 | `to_makepad_ui(&UiNode) -> String`; pure, unit-tested, needs no makepad-platform |
| `splash-widgets` | 181 | M3 native-control variants as external `script_mod!` — **fork-free theming** |
| `apps/kit-host` | 163 | desktop shell, builds against **upstream** makepad |
| `components/material/catalog.splash` | 825 | ~35 M3 components, pure data, hot-reloadable |

> **Do not read this as a production-equivalent renderer.** It is a partial
> translator plus a sample host. `splash-makepad` maps many kinds — the
> date/time/text pickers and the advanced containers among them — to a plain
> `View`, and silently omits several attributes `Attrs` declares. Its own docs
> put dynamic runtime mounting in the remaining "last-mile" work. Matching
> `NodeKind` counts establish tag coverage, **not** layout, interaction, state or
> rendering parity.

Its `NodeKind` enum is the same 23 tags Splash-OH's walker handles. Its status
list ends with: *"⏳ Next: … **Android build via `cargo-makepad`**."*

**So `Splash-Android` is not a from-scratch renderer. It is the Android host for
a pipeline that already exists**, plus the native-overlay escape hatches
octos-one has already proven (WebView, EditText/IME, video texture, camera).

Two things it is *not* yet, and both matter:

1. **`splash-render` does not depend on `splash-core`.** It pins
   `makepad-script = { git = "makepad/makepad", branch = "dev" }` directly.
   Splash-OH goes through `splash-core`; Splash-Makepad does not. Converging
   that is step one of "migrate to ymote/Splash".
2. **`UiNode`'s attribute set is small and closed** — no gradients, no per-corner
   radius, no font-family selection, no custom shader widgets. octos-one cards
   need `GradientYView`, `CircleView`, `WeatherIcon` (a shader widget) and
   `MapView` (a 7 800-line vector-tile renderer). `UiNode` needs a
   host-registered-custom-widget escape hatch before those can travel.

---

## 4. The four real gaps

### Gap 1 — three different VM revisions (blocks everything)

| tree | VM source |
|---|---|
| `octos-one/aichat/platform/script` | octos-org fork, **local patches** |
| `ymote/Splash vendor/makepad/platform/script` | pinned `makepad/makepad dev @ 4f9ce7a8` + Splash patches |
| `Splash-Makepad` | upstream `makepad-script` rev `e1c2164b` |

aichat vs. the Splash vendor tree: **3 129 changed lines across 34 files**
(`vm.rs` 621, `heap.rs` 348, `parser.rs` 311, `object_heap.rs` 299 …).

Two aichat-local patches are **not** in ymote/Splash and must be ported:

- **`180c420c` — the if/else `POP_TO_ME` fix.** In statement position the parser
  flagged `POP_TO_ME` on the last emitted opcode, which for if/else sits inside
  the final else branch; the true path's jump skips it, so the then-branch's
  widget is built but never attached. Fix adds `branch_merge_end`. Verified
  absent from `vendor/makepad/platform/script/src/parser.rs` — no
  `branch_merge_end` symbol. **Every sentinel-guarded card section depends on
  this** (`if sys.placesnum(…) >= -9998 { rows } else { loading }`), so without
  the port, cards vanish exactly when their data arrives.
- **`8f2a3775`** — cross-VM resource handle collision. **Not a VM-only patch.**
  It spans `platform/script/src/handle.rs` **and** `platform/src/script/res.rs`,
  `platform/src/script/std.rs`, `widgets/src/image.rs` — the last three being
  host-side global resource-table behaviour and image recovery. Porting the
  vendored VM alone **cannot reproduce this fix**; the host integration has to
  travel with it, and ymote/Splash has no host-side resource table to receive it.

Flowing the other way, ymote/Splash has patches aichat lacks: canonical
`try/catch` with cross-call unwinding, re-entrant-VM raw-pointer hardening, and
four fuzz-found parser/tokenizer bounds fixes. Those are strict upgrades.

### Gap 2 — two incompatible DSL dialects

|  | octos-one | Splash-OH / Splash-Makepad |
|---|---|---|
| card source | `SolidView{ draw_bg.color: #0f0f0f Label{…} }` | `{t:"column", bg:0xFF0F0F0F, c:[…]}` |
| resolution | makepad widget registry | `NodeKind::from_tag` → backend |
| portable | no | yes |

This is the expensive gap. Migrating means re-authoring, at minimum: 6 Splash
app specs, 5 exemplars (incl. the 14 KB nav card), `a2app/widgets/*.md`
(3 153 lines) and `framework/splash-manual.md` (2 567 lines) — and then
re-tuning every LLM generation prompt and re-testing every card on device.

Three ways to pay it:

- **(a) Rewrite the specs** to emit plain-data DSL. Cleanest end state, largest
  one-time cost, and every generated card must be re-validated.
- **(b) Add a makepad-syntax frontend to `splash-render`** that lowers
  `SolidView{…}` into `UiNode`. Cards and prompts survive untouched; you carry a
  compatibility layer. Cheapest to reach parity.
- **(c) Hybrid** — new apps in plain-data, existing cards keep the makepad path
  until individually ported. Two live paths for a while.

**Recommended: (b) then (c).** The frontend is a bounded piece of work in a
crate that is already pure and unit-tested, it de-risks the cutover completely
(cards keep rendering throughout), and it turns the dialect migration from a
blocking rewrite into per-app background work.

### Gap 3 — the capabilities are ambient globals, and the fix is a whole crate

All 32 methods are injected with `vm.set_injected_global`, so any card can call
any of them: GPS, network, geocoding, photo fetch. `web_card.rs` names the
intended destination in a comment:

> *"ymote/Splash `mod.tool` would formalize this with per-card leases + audit;
> until then it's a curated static allowlist plus an SSRF guard."*

**But that destination is not `splash-core`.** Its own crate doc is explicit:

> *"This crate masks the vendored VM down to the standalone Splash source
> surface, then owns runtime limits and diagnostic capture. **Effectful APIs
> belong to a separate host crate** and must be explicitly installed by trusted
> Rust code."*

The lease/audit machinery lives in **`splash-capabilities` — 10 354 lines** of
tool catalog, policy, lease lifecycle and audit views. Adopting it is not
flipping a switch on a crate we were already taking; it is taking a second, much
larger crate and adopting its whole model. The audit view is also bounded
in-memory by default — a *durable* journal is optional host work on top, not
something inherited.

### Gap 3b — **the blocker**: there is no legal `splash-core` entry point for today's cards

This is the finding that reorders the plan, and it is load-bearing.

`splash_core::Runtime::eval` runs `check_syntax` first and returns
`RuntimeError::SyntaxRejected` for anything outside canonical Splash. octos-one's
cards are makepad widget dialect, so they fail it. The compatibility door exists
— and is nailed shut for exactly our use case:

```rust
/// Evaluates the vendored Makepad parser's broader compatibility syntax.
///
/// This bypasses Splash's portable grammar contract and **must not receive
/// LLM-generated or otherwise untrusted source.** Prefer [`Self::eval`] for
/// all normal Splash execution.
pub fn eval_vm_compatibility(&mut self, source: &str) -> Result<Evaluation, RuntimeError>
```

octos-one's cards are **both** makepad-dialect (so `eval` refuses them) **and**
LLM-generated (so `eval_vm_compatibility` forbids them). There is no valid
`Runtime` path for them at all. The only thing that works today is what
Splash-OH actually does: the **raw** `splash_core::vm` re-export — which carries
the VM's provenance and `check_syntax` diagnostics but **none** of the security
profile, and no capability model whatsoever.

**Consequence: the dialect migration is a hard prerequisite for the capability
migration, not a parallel track.** The earlier plan claimed both at once.

### Gap 4 — the custom widgets have no `UiNode` representation

`MapView` (7 800 lines, native 2.5D vector tiles + route ribbon), `WeatherIcon`
(142-line shader widget), `GradientYView`, `CircleView`, `Card`, and the glass
panels. None map to a `NodeKind`. They need an escape hatch, and the sketch an
earlier draft gave — `{t:"custom", widget:"MapView", …}` against a
host-registered table — **cannot work as stated**: `NodeKind::from_tag`
(`node.rs:37`) returns `None` for any unknown tag and the walker drops the node
before a backend ever sees it. A lookup table alone changes nothing. This needs
a `NodeKind`/schema change, walker changes, serialization rules, event and
lifetime semantics, capability policy for what a custom widget may do, and a
separate answer for makepad-only widgets like `MapView` that no other backend
can ever resolve. It is a design item, not a hook.

---

## 5. The plan

### Phase 0 — converge the VM *(prerequisite; nothing else is safe first)*

1. Port `180c420c` (`branch_merge_end`) and the VM half of `8f2a3775` into
   `ymote/Splash`'s `vendor/makepad/platform/script`, each with a focused
   regression, and document both in `vendor/makepad/PATCHES.md` per that repo's
   stated upstream policy.
2. Rebase `aichat/platform/script` onto the Splash-pinned rev, or accept
   Splash's tree as the source of truth and re-apply octos-one's deltas on top.
   Pick **one** canonical VM.
3. Repoint `Splash-Makepad`'s `splash-render` from
   `makepad-script { git = makepad/makepad }` to `splash-core { git = ymote/Splash }`,
   importing the VM as `splash_core::vm` exactly as `Splash-OH/dsl.rs` does.
   Adopt `check_syntax` for diagnostics at the same time.

*Exit test:* all four weather exemplars, the nav card, and
`components/material/catalog.splash` evaluate identically on the old and new VM,
**including** a sentinel-guarded `if/else` section.

### Phase 1 — create `ymote/Splash-Android`

Mirror Splash-OH's shape and its one-directional dependency rule:

```
crates/splash-android/          the host + bridge     cdylib -> libsplash_android.so
crates/splash-android-native/   the render backend    rlib
android/                        the Java/Kotlin shell (Activity, WebView, EditText, Surface)
components/                     .splash component libraries (shared with Splash-Makepad)
```

- **`splash-android-native`** — pure translation only: `UiNode` → makepad
  dialect. It may depend on `splash-render` + `splash-makepad` and nothing else.
  **It cannot own `Splash.set_text()` or custom-widget resolution**, as an
  earlier draft claimed: whatever calls `set_text` must depend on
  `makepad-widgets`, initialise widget modules, own card state and perform the
  mount (compare `Splash-Makepad/apps/kit-host/Cargo.toml` and its
  `main.rs:83`), and resolving octos-one's `MapView` needs the octos widget
  crate. Those duties belong in the host crate, not here.
- **`splash-android`** owns the makepad host, the JNI surface, the capability
  registry, the web slots, and the `splash.invoke` bridge. It knows about
  `splash-android-native`; the reverse must never be true. (Splash-OH had to
  invert exactly one seam for this — `app::set_router` — expect the same.)
- **Web slots — a redesign, not a port.** An earlier draft said "port
  `webslot.rs` wholesale; Android is easier." That was unsupported.
  `WebCard` is explicitly **one** global overlay with a single owner
  (`web_card.rs:12`, `OVERLAY_OWNER`); `webslot.rs` manages a **per-build
  collection** of slots with stable ids and lifecycle rules. Going multi-slot on
  Android needs its own lifecycle, teardown, per-slot bridge routing, origin
  callbacks, and geometry that is only known *after* layout (Android views
  auto-size; ArkUI's do not — the property Splash-OH's hole-cutting depends on).
  Nor is the source-derived trust rule simply "stricter": octos-one already has
  an inline-document source gate *plus* owner checking (`web_card.rs:216`, which
  documents its own known holes for main-frame navigation and iframes). Source
  provenance and overlay ownership defend against different attacks; the Android
  design needs both, and a threat model that says so.
- Land the `Splash` **`isolate: false`** upstream PR that `Splash-Makepad`'s
  README already identifies as its single blocker; octos-one's fork already
  carries the field, so this is a port, not a design.

> **On the native-widget backend.** Splash-OH renders to *real* native ArkUI
> widgets, and the literal Android analogue is `android.widget.*` over JNI. That
> path has now been investigated in depth — see
> [`SPLASH-ANDROID-NATIVE-WIDGETS.md`](SPLASH-ANDROID-NATIVE-WIDGETS.md).
> Summary of what it found: **Android has no NDK widget API at all** (62 headers
> in the NDK's `android/` dir, none of them widgets), so `shim.cpp` has no
> Android translation; and unlike ArkUI, `android.widget.*` has no native tier
> beneath its managed object, which is precisely where Splash-OH's 2.5–3× came
> from. A Rust-JNI backend therefore builds the *same* Java objects a Java
> builder would, while additionally paying a JNI crossing per call and a
> `runOnUiThread` marshal per build.
>
> **Recommendation, unchanged in shape but now evidence-backed:** ship
> `Splash-Android` on `splash-makepad` with native views as overlays — the
> hybrid octos-one already runs — and add `splash-android-view` as a *second*
> backend behind the same `UiNode`, built as **serialize-once-then-build-in-Java**
> rather than as a JNI-per-node port. Run that document's three-arm benchmark
> before committing to it. Everything else in this plan is unchanged by the
> choice.

*Exit test:* `components/material/catalog.splash` renders on the OnePlus 6T
through `cargo-makepad android`, with one webview slot live in the tree.

### Phase 2 — migrate the dialect *(moved ahead of capabilities — see Gap 3b)*

1. Build the makepad-syntax frontend for `splash-render` (Gap 2, option b) so
   existing cards render unchanged through the new pipeline.
2. Add the custom-widget escape hatch and register `MapView`, `WeatherIcon`,
   `GradientYView`, `CircleView`, `Card`, glass panels (Gap 4).
3. Port apps to plain-data DSL **cheapest first**: `activity` → `news` →
   `stock` → `weather` → `weather-activity` → `nav` last (it is direct-served,
   14 KB, and carries `MapView`). Rewrite each `app.md` and its `lint.json` with
   the port; the lint rules are pattern-matches on the emitted source and will
   all need new patterns.
4. `youtube` and `web` stay `runhtml` — they gain the lease model in Phase 3 and
   are otherwise untouched.

*Exit gate:* a semantic differential suite, not "it renders". Same card, old path
vs new: identical node count **and** identical emitted properties, plus the
interaction assertions each `lint.json` already encodes.

### Phase 3 — port the capabilities *(only now legal)*

Once cards are canonical Splash, `Runtime`/`splash-capabilities` becomes
reachable:

1. Move the 31 `sys.*` helpers **and `agent.notify`** out of
   `aichat/widgets/src/splash.rs` into a `splash-android` capability module,
   registered as `mod.tool` tools. `agent.notify` is the harder half — it is an
   event protocol with an app-side dispatch, not a data call.
2. Give each app a declared lease set (`weather` gets `weather`/`airquality`/
   `photo`; `nav` gets `gps`/`route`/`search`/…). A card asking for anything it
   did not declare fails closed.
3. Replace `HTTP_FETCH_ALLOWED_HOSTS` with the same lease mechanism. Keep the
   SSRF guard — but **do not call it a boundary**: it string-splits an HTTPS URL
   and matches textual private-host patterns, with no resolve-and-bind, so it
   does not stop DNS rebinding. It needs its own threat model and adversarial
   tests, which are not in this plan.
4. Route untrusted page scripts through `splash_core::Runtime` with limits
   calibrated to the real workload — copy `bridge.rs:1537-1564`.

*Exit gate:* a card calling an undeclared tool is **refused**, and the refusal is
observable. Test the refusal path; a lease that is never checked looks exactly
like one that passes.

### Phase 4 — cut octos-one over

Replace `aichat/widgets/src/splash.rs`'s module registration with a dependency on
`splash-android`, and `web_card.rs` with an adapter over the ported `webslot`.

> **Correction to `SPLASH-NATIVE-INTEGRATION.md` §10.** That document predicted
> the port would "stay concentrated at the VM↔host binding boundary". An earlier
> draft of this plan repeated it. **It is false.** The 2 612 registration lines
> are the easy part. The `Splash` widget also implements: per-card **isolate
> VMs** (`splash.rs:2103`), **incremental streaming evaluation** as the LLM emits
> (`:2256`), a 1 M instruction limit, scoped `ui` handles, eager widget-tree
> registration (`:2429`), **`fn tick()` callbacks** (`:2597`), animation pumping,
> and async-data re-evaluation. `splash-render::build` does none of it — it
> creates a fresh raw VM and calls `eval` once.
>
> The nav card depends on this concretely: it uses in-place `ui.*.set_*` updates
> specifically to avoid rebuilding the map every tick. A backend that only knows
> how to build a tree from scratch cannot run it.
>
> **A complete host-semantics inventory is a prerequisite deliverable, not a
> cutover detail.**

---

## 6. Cost, risk, and what to do first

**The duration estimates an earlier draft carried here have been withdrawn.**
They were not derived from anything. "Phase 0 — days" rested on a line count with
no assessment of how much of the divergence is semantic; "Phase 1 — 1–2 weeks,
mostly assembly" assumed away incompatible execution profiles, the missing live-
host semantics, a multi-slot WebView redesign, and Android build integration.
Estimating before the gates below are met would be inventing numbers.

**Gates that must close before any phase gets a duration:**

| gate | why |
|---|---|
| One execution/security profile chosen for generated cards | Gap 3b — raw VM and canonical `Runtime` are not interchangeable, and only one is legal per card class |
| Complete host-semantics inventory | Phase 4 — isolates, streaming, `tick()`, `ui` handles, animation pumping are unaccounted for |
| A defined `UiNode` subset + semantic differential suite | the dialect frontend is not "low risk" without one |
| `UiNode` wire / event / delta contract | none exists; see the native-widgets doc §7b |
| Multi-WebView lifecycle + security design | `WebCard` is single-overlay; Splash-OH is multi-slot. These are different designs |
| Version-pinned Splash-Makepad + Splash dependency | three VM revs today; Splash-Makepad is a partial translator, not a production renderer |

**Relative risk, which is assessable now:**

| phase | risk |
|---|---|
| 0 — VM convergence | **high** — a missed patch is a silently-vanishing card |
| 1 — `Splash-Android` | medium |
| 2 — dialect | **high** — it is now a prerequisite, not background work |
| 3 — capabilities | medium-high — `splash-capabilities` is 10 354 lines, and `agent.notify` is a protocol port |
| 4 — cutover | **high, not low** — it removes the code that implements streaming, isolation and event behaviour |

**Three failure modes worth naming up front.** They are the ones that produce a
confident, plausible, wrong result:

- **Silent VM regressions.** The if/else bug rendered *nothing* — no error, no
  log. Phase 0's exit test must include a sentinel-guarded `if/else`, not just
  "the card renders."
- **A dialect port that quietly builds less.** Splash-OH asserts node counts at
  runtime, every screen, every run, precisely because *"the other one is faster
  because it quietly builds less"* is the obvious way for a comparison like this
  to be wrong. Do the same across the dialect boundary: assert the ported card
  produces the same node count as the original.
- **Capability regressions that only fail open.** A lease that is never checked
  looks exactly like a lease that passes. Test the refusal path, not the
  success path.

**Start with Phase 0, step 1** — porting `branch_merge_end` into `ymote/Splash`
is a few hours, unblocks everything downstream, and is worth doing even if the
rest of this plan is deferred: it is a real parser bug living in a repo that
does not know it has it.
