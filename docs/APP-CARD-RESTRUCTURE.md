# Restructuring octos-one's app cards for one DSL, many platform backends

*2026-07-29. Supersedes the dialect sections of
[`SPLASH-ANDROID-MIGRATION.md`](SPLASH-ANDROID-MIGRATION.md), which sized this
work before the card vocabulary had been measured and before three backends
existed to check it against.*

The goal: a card is authored **once**, and renders as makepad widgets on the
phone today, `android.widget.*`/Material on Android, and ArkUI on OpenHarmony —
without the card knowing which.

---

> ## AMENDMENT — 2026-07-30
>
> Two claims below are **wrong**, and the approach has changed. Read this before
> acting on §2 or §5.
>
> **§2 "The mapping is 1:1" does not hold.** It is true for the layout and text
> nodes and false for everything that matters after that. Card evaluation ends at
> `View::script_from_value` *after* makepad's widget constructors have resolved
> through the widget registry, so the abstraction boundary has to move **before
> widget construction**, not after "VM output". And `ui.<id>` is a live handle
> into the real widget tree, `set_text` is a Rust widget method, and Button
> closures are retained in the widget and call back into the same VM. None of
> those have a plain-data equivalent. Shader uniforms are not a node mapping at
> all: `AqiContour` takes its 4x4 field as sixteen scalars, so the card text names
> a GPU ABI.
>
> **§5's S2 "a makepad-dialect frontend … a bounded transform" is not bounded.**
> Translating the existing dialect means growing a compatibility compiler for
> widget inheritance, resources, closures, shader uniforms and methods. Do not
> start there.
>
> **What replaced it.** Do not retrofit a generic `UiNode` under the current
> dialect. Instead the LLM emits a small **semantic plan** — a closed, typed,
> MCP-shaped vocabulary of blocks carrying intent only — and each backend
> **lowers** that plan to its own DSL. Modelled on octos's DOT pipeline, where the
> parser recognises a closed set of handler kinds, `dora_tool_map.json` binds
> abstract names to concrete tools, and the runtime owns the constraint language.
>
> Proven on device 2026-07-29/30: one 604-byte plan lowered to both makepad Splash
> DSL (OnePlus 6T) and ArkUI (Mate 70 Air) — two dialects sharing no syntax, no
> colour model, no attribute names and no data API — rendering the same city,
> temperatures, forecast rows, AQI and UV. Prototype and both expanders in the
> session scratchpad; the ArkUI side is committed as `planweather` in
> ymote/Splash-OH.
>
> **The measured porting cost is not the widgets — it is the host capability
> surface.** makepad exposes thirty-odd `sys.*` helpers; Splash-OH injects five
> (`fetch_num`, `fetch_fmt`, `fetch_weekday`, `invoke`, `sget`). That is why
> `SunMoon` is not degraded on ArkUI but *impossible* there. Good news, since
> "write N data helpers" is bounded work in a way "rewrite the UI" is not.
>
> **§4a needs correcting too, in the other direction.** I wrote that mutation needs
> a session/delta contract built from scratch, and that overstated it: octos-one
> ALREADY has a zero-rebuild in-place update path — `fn tick()` plus
> `ui.<id>.set_*`, with 63 call sites in the nav trip planner and `sys.navsecs`
> existing specifically so a tick does not arm the re-eval pump. The real gap is
> narrower: that path and the `agent.notify("set", …)` state path are DISJOINT, and
> a state write takes the rebuilding one. See
> [`CARD-STATE-IDENTITY.md`](CARD-STATE-IDENTITY.md).

---

## 1. What the cards actually are today (measured, not assumed)

Every `.splash` card in `a2app/` is **makepad widget dialect** — it resolves
through makepad's widget registry, which is exactly the coupling that prevents a
second backend:

```
SolidView{ width: Fill height: 1560 flow: Down draw_bg.color: #0f0f0f
    Label{ text: sys.weather(31.23,121.47,"current.temperature_2m") + "°"
           draw_text.color: #f1f1f1 draw_text.text_style.font_size: 37 } }
```

**But the vocabulary is tiny and mechanical.** Counted across every exemplar and
card in `a2app/`:

| widget | uses | | attribute | uses |
|---|---|---|---|---|
| `Label` | 242 | | `width` / `height` | 442 / 364 |
| `View` | 155 | | `text` | 300 |
| `RoundedView` | 63 | | `draw_text.color` | 239 |
| `Button` | 57 | | `…text_style.font_size` | 214 |
| `WeatherIcon` | 32 | | `flow` | 213 |
| `SolidView` | 27 | | `draw_bg.color` | 168 |
| `Filler` | 14 | | `align` | 138 |
| `CircleView` | 13 | | `spacing` | 112 |
| `MapView` | 6 | | `draw_bg.border_radius` | 99 |
| `TextInput` | 4 | | `padding` / `margin` | 63 / 33 |
| `GradientYView` | 4 | | `on_click` | 55 |
| `Card` | 2 | | `key` / `value` | 84 / 83 |
| `Image` | 1 | | `draw_bg.cond` | 32 |

**13 widget types and ~25 real attributes.** That is the whole surface. The
earlier plan treated the dialect migration as an open-ended rewrite; it is not —
it is a bounded, mostly mechanical substitution.

---

## 2. The mapping is 1:1

Every construct the cards use has an exact plain-data equivalent. This table is
the migration:

| today (makepad dialect) | canonical node |
|---|---|
| `View{flow: Down}` | `{t:"col"}` |
| `View{flow: Right}` | `{t:"row"}` |
| `View{flow: Overlay}` | `{t:"stack"}` |
| `SolidView{draw_bg.color: C}` | `{t:"col", bg: C}` |
| `RoundedView{…border_radius: R}` | `{t:"col", bg: C, radius: R}` |
| `Label{text: T, draw_text.color: C, …font_size: S}` | `{t:"text", text: T, color: C, size: S}` |
| `Button{text: T, on_click: …}` | `{t:"button", label: T, tap: 1, key: …}` |
| `Filler{}` | `{t:"spacer"}` |
| `CircleView{}` / `GradientYView{}` | `{t:"circle"}` / `{t:"gradient"}` |
| `Image{}` / `TextInput{}` / `Card{}` | `{t:"image"}` / `{t:"input"}` / `{t:"card"}` |
| `WeatherIcon{draw_bg.cond: N}` | `{t:"weathericon", value: N}` |
| `MapView{…}` | `{t:"navmap", …}` |
| `Align{x,y}` | `alignx` / `aligny` |
| `Inset{l,t,r,b}` | `pad` / `padx` / `pady` |

The three custom widgets in that list are no longer a blocker: `WeatherIcon`,
`MapView` and the glass panels have been ported to Android views and are running
(`ymote/Splash-Android`, `catalog/…/WeatherIconView|NavMapView|GlassPanelView`).

---

## 3. The layering

```
a2app/apps/*/app.md            authoring contract — emits CANONICAL nodes
        │
        ▼  LLM
card .splash  (plain data)
        │
        ▼  splash-core (ymote/Splash) — VM + bounds + capability host
node tree   { kind: String, attrs: [(String, Val)], children }
        │
        ├──► splash-makepad      ──► makepad widgets      (phone today)
        ├──► splash-android-view ──► android.widget.* / Material
        └──► splash-oh-native    ──► ArkUI
```

Four things belong to the **shared** layer and must be versioned as one contract:

1. **The node vocabulary** — the `t` tags and attribute names above.
2. **The capability registry** — 31 `sys.*` helpers **plus `agent.notify`**,
   which is a separate injected global (`splash.rs:114` vs `:1248`) and is the
   card *event protocol*, not a data helper. `trip-planner.splash` calls it 52
   times.
3. **The state contract** — today `{{state.key}}` is a host-side string
   substitution before eval, which means a missing key is a syntax-level hole.
   Replace it with injected accessors (`S(key)`, `N(key, default)`), as the
   Material catalog does: a missing key then returns a default instead of
   killing the evaluation.
4. **The custom-widget registry** — a `{t:"custom", widget:"MapView"}` escape
   hatch. Note `NodeKind::from_tag` currently *drops* unknown tags before any
   backend sees them, so this needs a schema change, not a lookup table.

### Use a generic attribute bag, not a fixed struct

`splash-render`'s `Attrs` is ~30 fixed fields. That was already tight for octos-one
and it did not survive contact with 43 Material components. The catalog uses
`Vec<(String, Val)>` against a declared ~56-name vocabulary and it scaled without
a Rust change per attribute. Adopt that shape in the shared core.

(LiveId keys are one-way hashes, so the vocabulary must be **declared**, not
discovered by iterating object keys. That constraint is real and shapes the design.)

---

## 4. The three genuinely hard parts

### 4a. `ui.*` in-place mutation has no backend-agnostic form

`trip-planner.splash` has **66** `fn tick` / `ui.<id>.set_*` sites. They exist to
mutate named widgets *without rebuilding the tree* — specifically so the map does
not get rebuilt every tick. That is imperative mutation of a makepad widget
handle. It cannot cross to another backend.

**The replacement is a diff, not a port.** Re-evaluate the card and diff the two
node trees; emit `Create` / `SetAttr` / `Insert` / `Remove` ops. The map node is
unchanged between ticks, so no op is emitted for it — the "don't rebuild the map"
property falls out of the diff instead of being hand-managed, and it works on
every backend.

This needs a stable identity for reconciliation. `Attrs::id` is documented as a
makepad widget name, so it cannot carry it; add an explicit `key` and use
structural path as the fallback.

### 4b. The security model only unlocks *after* the dialect moves

`splash_core::Runtime::eval` rejects non-canonical syntax; `eval_vm_compatibility`
is documented *"must not receive LLM-generated or otherwise untrusted source."*
octos-one's cards are **both** makepad-dialect and LLM-generated, so today there
is no legal `Runtime` path for them — only the raw `splash_core::vm` re-export,
which carries provenance but no capability model.

Canonical plain-data cards are accepted by `Runtime::eval`. **That is the payoff**:
`mod.tool` leases, bounded execution and the audit journal become reachable, and
`sys.*` stops being ambient. It is also why the dialect migration is a hard
prerequisite for the capability work rather than a parallel track.

### 4c. Live-host semantics are not in `splash-render`

The `Splash` widget also implements per-card isolate VMs (`splash.rs:2103`),
incremental streaming evaluation as the LLM emits (`:2256`), a 1M instruction
limit, scoped `ui` handles, eager widget-tree registration (`:2429`), `fn tick()`
(`:2597`) and animation pumping. `splash-render::build` creates a fresh VM and
calls `eval` once. **Inventory these before the cutover** — they are not covered
by the node contract and they are the part an earlier draft wrongly called "the
easy part."

---

## 5. Sequence

Each step is independently shippable and answers the question the next depends on.

**S1 — write the vocabulary down.** The tables in §2 as a versioned spec plus a
golden-file test per node kind. Nothing else can be checked until this exists.

**S2 — a makepad-dialect frontend in the shared core.** Lower `SolidView{…}` into
canonical nodes. Existing cards and existing LLM prompts keep working while the
backend swaps underneath — this is what de-risks everything after it. Given §1's
1:1 table this is a bounded transform, not a rewrite.

**S3 — port the apps, cheapest first.** `activity` → `news` → `stock` → `weather`
→ `weather-activity` → `nav` last (direct-served, 14 KB, carries `MapView` and all
66 `ui.*` sites). Rewrite each `app.md` and its `lint.json` together; the lint
rules pattern-match emitted source, so every rule needs a new pattern.

*Gate, per app:* a semantic differential — same card, old path vs new, identical
node count **and** identical emitted attributes, plus the interaction assertions
`lint.json` already encodes. Not "it renders."

**S4 — the delta/event contract** (§4a). Only now is `nav` portable.

**S5 — capabilities** (§4b): `sys.*` and `agent.notify` become `mod.tool` tools
with per-app leases. *Gate:* an undeclared tool call is **refused** and the
refusal is observable — test the refusal path, not the success path.

**S6 — cut over.** Replace the module registration in
`aichat/widgets/src/splash.rs` with a dependency on the shared core, keeping
makepad as one backend among three.

---

## 6. What is already proven, and what is still assumption

**Proven on device:**

- The VM evaluates plain-data DSL on Android and drives real native widgets —
  42 catalog screens, 0 placeholders, 0 exceptions (`ymote/Splash-Android`).
- A generic attribute bag scales to 43 Material components.
- The three "un-portable" custom widgets port: `WeatherIcon` (8 animated
  conditions), `MapView` (the pinhole ground-plane projection, 3 nav modes),
  glass panels.
- State round-trip through the VM: widget event → Rust state → re-eval → new
  tree → new views.

**Still assumption:**

- The **diff/delta protocol** (§4a) is designed but unbuilt. Everything shipped
  so far is full-rebuild.
- No estimate is offered for S3. The mapping is mechanical; re-tuning the LLM
  prompts for six app specs and re-validating generated cards on device is not,
  and I have no measurement for it.

### Three VM shapes that silently produce a wrong tree

Found while building the catalog, on `makepad-script` rev `e1c2164b`. None raises
an error, so the authoring contract in S1 must forbid them:

| shape | result | use instead |
|---|---|---|
| a top-level **function call** as the module result — `page([...])` | root has no `t` | end with a **literal object** |
| `let k = [ {…}, {…} ]` then `c: k` | array arrives **empty**, children dropped | inline the array, or `let k = []` + `k.push(…)` |
| `st.missing_key` | hard VM error, whole eval fails | injected `S()` / `N()` accessors |
