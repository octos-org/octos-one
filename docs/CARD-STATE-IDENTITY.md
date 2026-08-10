# Card state and widget identity — what exists, what's missing

*2026-07-30. Steps 9–10 of the semantic-plan refactor. This is an INVESTIGATION and
DESIGN note, not shipped code: the change it describes cannot be validated without
running the nav card on a device, and the test phone was unavailable. Everything
below is read from the current source and cited.*

## The headline: the hard part is already built

Earlier design notes — including my own amendment to
[`APP-CARD-RESTRUCTURE.md`](APP-CARD-RESTRUCTURE.md) — described identity-preserving
update as the remaining project. That overstated it. octos-one already has a
zero-rebuild update path, in production, with 63 call sites in the nav trip planner
alone.

What is actually missing is much smaller: **the two update paths are disjoint**, and
the one a stateful card uses is the rebuilding one.

## Path A — in-place, identity-preserving (exists, used by nav)

A card defines `fn tick()`, which mutates NAMED widgets through handles:

```
fn tick() { ui.eta.set_text(…)  ui.map.set_car(…) }
```

- `Splash::handle_event` fires the tick timer and calls `tick()` in the card's own
  scope (`aichat/widgets/src/splash.rs`, the `tick_timer` branch).
- `splash_mark_tick_changed()` is called by each in-place setter, and the view
  repaints ONLY if a setter actually changed something — a static card was
  otherwise forcing a surface swap every second, which flickered the native
  composer over the GL surface.
- `sys.navsecs(period)` exists *specifically* to drive a tick **without** arming
  the re-eval pump, and its comment states the reason outright: *"For `fn tick()`
  cards that update named widgets in place (ui.<id>.set_text) and must NEVER
  rebuild — e.g. the live-navigation card, where a rebuild would tear the map
  widget down."*

So the contract already exists: stable names (`ui.<id>`), targeted mutation, no
tree reconstruction, and a repaint only when something changed. That is the
substance of a delta protocol.

## Path B — state write, full rebuild (what stateful cards get)

```
Button{ on_click: || agent.notify("set", {key: "selected", value: "AAPL"}) }
let selected = "{{state.selected}}"
```

- The `set` branch in `app/app/src/main.rs` writes the key and calls
  `refresh_a2app_templates` — the only call site.
- That function TEXTUALLY substitutes `{{state.k}}` into the card body and pushes
  the whole thing through `splash_view.set_text(cx, &resolved)`.
- `set_text` re-evaluates the body, so the widget tree is reconstructed.

Identity is lost here not through any subtlety but because the tree is rebuilt from
source text. Scroll position, focus, a `MapView`'s camera and tile cache, and any
retained instance state go with it.

## Why this matters now

The stocks plan in [`prototype-semantic-plan/`](prototype-semantic-plan/) is the
first plan that needs Path B: a selected ticker and a chart range that the user
changes, and two views chosen by that state. Weather never did — it is a pure
function of (place, locale).

Lowering `onTap: select` to `agent.notify("set", …)` works and renders. But a
stocks DETAIL view containing a `StockPlot` will rebuild that plot on every range
chip tap, which is exactly the class of problem `sys.navsecs` was added to avoid
for `MapView`.

## The change, precisely

Not a new architecture — a bridge between two existing paths.

1. **Declare the dependency.** A plan already declares its state keys. Extend the
   lowering so a block that consumes a key emits a named widget plus a
   key→widget-setter binding, rather than an interpolated literal. Concretely,
   `{{state.range}}` inside a `TextStat` becomes `ui.range_label` plus a binding
   that says "on `range` change, call `set_text`".

2. **Route the write through Path A.** In the `set` handler, when every widget
   bound to the changed key has an in-place setter, call those setters instead of
   `refresh_a2app_templates`. Fall back to the full re-eval when any binding
   cannot be satisfied — and log which one, so the fallback is visible rather than
   silent.

3. **Views are the exception.** Changing `selected` switches WHICH view renders, so
   the tree genuinely differs and a rebuild is correct. The gain is confining
   rebuilds to view switches, where they are expected, instead of every value
   change.

4. **Keys, not paths.** Widget identity must come from a declared name, not tree
   position. `ui.<id>` already provides this; the lowering must emit stable ids
   derived from the plan (e.g. `movers_row_3`), never from iteration order, or a
   list whose contents shift will rebind the wrong widget.

## Step 10 — nav is the acceptance test, and it is already the proof

Nav is where a rebuild is most expensive and where Path A was invented. So it is
not the last thing to migrate; it is the existing evidence that Path A works at
scale — 63 in-place setters, a live map, a 1 Hz tick, no rebuilds.

The migration order that follows: bind stocks (two keys, one retained widget)
through the bridge above and confirm the plot survives a range change. Only then
consider expressing nav as a plan, and expect the nav card to keep its hand-written
`tick()` for the map regardless — a plan can name a `NavigationMap` block, but the
camera, tile lifecycle and route ribbon stay backend code.

## Status

- Investigated and cited: **done**.
- Design: **above**.
- Implementation: **not started**, deliberately. The bridge touches the state
  handler and the lowering together, and its whole purpose is preserving a live
  `MapView`/`StockPlot` across an update — which is only observable on a device.
  Landing it unverified would be the wrong trade.
