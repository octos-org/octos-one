# Nav → reusable, LLM-composable cards

**Goal:** turn the ~490-line monolithic `trip-planner.splash` into a few coarse,
reusable cards that an LLM composes into apps (ride-share, delivery, store
locator, "navigate to X", …) — instead of hand-serving one giant card.

**Status:** design + Phase 2 started. `cards/navigate.splash` extracted (the 3D
turn-by-turn card). Framework embed primitive (Phase 1) and on-device
verification still pending.

---

## 1. Why

The card is direct-served verbatim (`include_str!`) precisely because it is *too
large for the on-device model to generate* (REQUIREMENTS R1.2). Decomposing it
dissolves that constraint: the heavy pieces become pre-built, tested cards, and
the LLM only has to emit a small **host** that wires them — small enough to
generate reliably. Secondary wins: kills the ~200 lines of duplicated
search-result rows, and lets every other app reuse maps/routing/nav.

## 2. The seams that already exist

`trip-planner.splash` is five screens over one shared data core, switched by a
derived `scr` selector:

| `scr` | screen | reads | writes (its "output") |
|-------|--------|-------|-----------------------|
| `search`  | destination search      | `q`                         | `sel` (a picked place) |
| `find`    | origin/stop/dest editor | `find`, `q`                 | `orig` / `dest` / `wp1` / `wp2` |
| `preview` | route preview           | `q`, `sel`, origin          | `dest` (commit) |
| `plan`    | editable multi-stop trip| `orig,dest,wp1,wp2,mode`    | `go=1` (start) |
| `drive`   | 3D turn-by-turn         | `orig,dest,vias,mode,view`  | `go=0` (end) |

Two things are already the shared substrate:

- **`MapView`** (native widget): `nav_mode: plan|2d|3d`, `set_nav_polyline`,
  `set_route_markers`, `nav_zoom_by`, `nav_center_origin`, gestures.
- **`sys.*` geo helpers**: `search`/`searchnum` (Photon POI), `navroute`/
  `navroutenum`/`navstep` (OSRM), `coord` (packs/unpacks a place), `gps`.

## 3. Target architecture

```
Level 2  composable app cards (the LLM wires these)
         ┌───────────────┬──────────────────┬────────────────────┐
         │  nav.picker   │   nav.planner    │   nav.navigate     │
         │ search+preview│  editable trip   │  3D turn-by-turn   │
         └───────┬───────┴────────┬─────────┴─────────┬──────────┘
Level 1  data     │  sys.geo.* — search / route / step / coord / gps
Level 0  widget   └── MapView — nav_mode, polyline, markers, gestures
```

Shared value types (both already used in the card):

- **`Place`** = `"lat,lon|name"` — the `sys.coord` token; the atom passed between cards.
- **`Trip`** = `{ origin: Place, dest: Place, stops: [Place], mode: "drive|walk|bike" }`.

## 4. Card contracts

| Card | Props (in) | Events (out) | Uses |
|------|-----------|--------------|------|
| **`nav.picker`**   | `title`, `near?` (Place, for ranking), `preview?` (bool), `defaults?` ([Place]) | `pick(Place)`, `cancel` | `sys.geo.search`, MapView(plan) if `preview` |
| **`nav.planner`**  | `Trip` (all fields optional) | `start(Trip)`, `edit(field)` | MapView(plan), `sys.geo.*`, **embeds `nav.picker`** for field edits |
| **`nav.navigate`** | `Trip` + `view?` (`2d`\|`3d`) | `arrive`, `end` | MapView(3d/2d), `sys.geo.navstep/navroute` |

- `nav.navigate{ origin, dest, mode }` is the "call 3D nav directly" case.
- `nav.planner` is the standalone planner.
- `nav.picker` folds today's duplicate `search` + `find` screens into one field.

## 5. The one framework primitive to build (Phase 1)

Today a card is a singleton reading global `{{state.*}}`. Composition needs
**props + events** — three additive runtime features:

1. **Props scope** — a card reads `props.dest` instead of `{{state.dest}}`, so
   the same card embeds twice with different inputs (two `nav.picker`s for
   pickup + dropoff). Props re-resolve in `tick()` (keeps the R9.5 freeze/tick rule).
2. **`Card` embed widget** — instantiate a registered card by name:
   ```
   Card{ use: "nav.navigate"
         props: { origin: pickup, dest: drop, mode: "drive" }
         on: { end: || set("step", "done") } }
   ```
3. **`emit(event, value)`** — child raises a *scoped* event to the parent's
   `on:` handler, instead of writing global state (so events don't collide
   across instances).

Plus a **registry**: named cards + their prop/event schema (an `app.md` per card).
Everything else — MapView, `sys.*`, `agent.notify`, `tick`, direct-serve — is unchanged.

## 6. How the LLM composes apps

The LLM emits a ~25-line **host** that wires pre-built cards. Ride-share:

```
let step = "{{state.step}}"; let pickup = "{{state.pickup}}"; let drop = "{{state.drop}}"
if step == "0" { Card{ use:"nav.picker"   props:{title:"Pickup", near:"gps"} on:{pick:|p| { set("pickup",p); set("step","1") }} } }
if step == "1" { Card{ use:"nav.picker"   props:{title:"Dropoff"}            on:{pick:|p| { set("drop",p);  set("step","2") }} } }
if step == "2" { Card{ use:"nav.navigate" props:{origin:pickup, dest:drop, mode:"drive"} on:{end:|| set("step","0")} } }
```

### App scenarios (all reuse the same 3 cards)

| App intent | Composition |
|------------|-------------|
| "navigate to X" | `nav.navigate{origin:"gps", dest:X}` — one card |
| "plan a trip with stops" | `nav.planner` → `on start` → `nav.navigate` |
| store / POI locator | `nav.picker{near:gps, preview:true}` → `on pick` → `nav.navigate` |
| ride-share | `nav.picker`×2 → `nav.navigate` |
| delivery / dispatch | `nav.planner{stops:[…]}` → `nav.navigate` per leg |
| a location *field* in any app | embed `nav.picker`, take `pick` |

The current Trip Planner becomes the canonical host: `picker → preview → planner → navigate`.

## 7. Migration (each step verifiable on-device)

1. **Framework (Phase 1):** props-scope + `Card` embed + `emit` (additive).
2. **Extract `nav.navigate`** — most self-contained; the one you call directly. ← *started (`cards/navigate.splash`)*
3. **Extract `nav.picker`** — collapse `search` + `find` duplication.
4. **Extract `nav.planner`** (embeds `nav.picker` for field edits).
5. **Rewrite `trip-planner.splash`** as the thin host; verify end-to-end parity.
6. **Register + document** each card's schema; add `ride-share` / `locator` exemplar hosts.

## 8. Notes / risks

- **One live map at a time.** MapView is heavy (see the OOM work); the flow shows
  one card at a time, so only one MapView is ever live — keep it that way.
- **Freeze/tick.** Prop reads must happen in `tick()` too, or live-data (GPS,
  geocode) changes silently won't apply (R9.5).
- **Event scoping.** `emit` must route to the correct parent handler under nesting.
- Until Phase 1 lands, `cards/navigate.splash` reads its Trip from **state keys**
  (runnable via direct-serve + seeding); swapping `{{state.X}}` → `props.X` is
  mechanical once the embed primitive exists.
