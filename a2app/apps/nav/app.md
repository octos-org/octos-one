# Nav app — maps & directions

A Google-Maps-style, full-screen **trip planner + turn-by-turn navigation** card.
Routed here for any "get me there" intent: `directions to X`, `navigate to X`,
`route to X`, `how do I get to X`, `map to X`, `show me a map of X`, `导航`,
`路线`, `怎么去X`, `去X怎么走`. (A BARE place name with no travel verb is weather,
not nav; nearby-things-to-do is `activity`. Nav is specifically about GOING
somewhere.)

The card is a complete flow — **search a destination → preview it on a map →
plan the route (editable origin, multi-stop, drive/walk/bike) → drive it
turn-by-turn** over a native 2.5D map with a moving vehicle puck and standing
route pins. Every location, distance, ETA, and maneuver is LIVE (keyless OSM
APIs); nothing is hardcoded.

## How this app is served — DIRECTLY, no LLM (the youtube model)

This app is a **FIXED, self-contained card**. The runtime serves the canonical
card at the bottom of this spec **verbatim** — the same way the `youtube` app is
served — because the on-device model under-generates / truncates a card this
large (~14 KB, 430 lines) when asked to re-emit it. So there is **no
generation step and no lint** for nav: the AMA only ROUTES here, and the client
emits `apps/nav/exemplars/trip-planner.splash` directly. This document is the
app's contract + reference (like `apps/youtube/app.md`): it explains the flow
and the map/navigation `sys.*` helpers so the card can be maintained and so the
AMA composer can reuse those helpers in a composed app.

The card opens on the destination search screen — the user types a place
(writing `{{state.q}}`), `sys.search` fills the results, and the flow proceeds
(preview → plan → drive). Routes start from a San Jose default (`olat 37.3350 /
olon -121.8850 / oname "San Jose (downtown)"`), which the user replaces by
tapping the origin row. Every `{{state.x}}` placeholder and `agent.notify` is
part of the card's state machine — the render pipeline substitutes state and
tags the notify events to this card by its slot id.

## The screens & flow (so you can repair a lint failure)

One `SolidView{ … flow: Overlay }` root holds five mutually-exclusive screens,
chosen by the `scr` string computed at the top from the state placeholders:

- **search** — destination search. A `TextInput` writes `q`; `sys.search(q, i,
  …)` fills up to five tappable result rows; tapping row `i` sets `sel = i`.
- **preview** (`sel != 0`, no destination yet) — the picked place on a `plan`
  map with its name/category/address + ETA and a **Directions** button that
  sets `dest`.
- **find** (`find == "orig"` or `"stop"`) — a searchable overlay to REPLACE the
  origin or ADD a stop; a result tap packs `"lat,lon|Name"` into `orig`/`wp1`/
  `wp2` and clears `find`.
- **plan** (`dest != 0`) — map on top, a bottom panel with mode tabs (drive/
  walk/bike), the live ETA, the editable origin row, up to two removable stops,
  the destination, an **Add stop** button, and **Start** (sets `go = 1`).
- **drive** (`go == 1`) — full-screen 2.5D nav: a turn banner (arrow + upcoming
  instruction + distance), remaining ETA, a floating 2D/3D toggle, and **End**.

State keys (all read as `"{{state.key}}"`, all written via `agent.notify("set",
{key, value})`): `q` destination query, `sel` picked result index, `dest`
`"lat,lon|Name"`, `find` (`0`/`orig`/`stop`), `orig`/`wp1`/`wp2`
`"lat,lon|Name"`, `mode` (`drive`/`walk`/`bike`), `go` (`0`/`1`), `view`
(`2d`/`3d`). Unset placeholders arrive as the string `"0"`; the card treats
`"0"` as empty everywhere.

## The map & navigation `sys.*` helpers (this app's live data layer)

All keyless; each shares ONE cached fetch per identical call, so extra
index/field reads are free. All return "" / `-9999` while loading, then the card
fills in. **Never hardcode a place, coordinate, distance, ETA, or maneuver —
bind every one through these.**

### `sys.search("<free text>", index, "field")` → STRING — POI/address search (Photon)

The "search a location" step. `index` 0 = best hit, up to ~5. Fields
(case-insensitive): `name`, `label` (full address line, also `addr`), `cat`
(category), `lat`, `lon` (5-dp strings — use these to PACK `"lat,lon|Name"` into
a state key), `count`. `""` while loading / past the last hit.

```
sr0n := Label{ text: "" }          // then in fn tick(): ui.sr0n.set_text(sys.search(q, 0, "name"))
// pack a picked result into state:
on_click: || agent.notify("set", {key: "orig", value: sys.search(q, 0, "lat") + "," + sys.search(q, 0, "lon") + "|" + sys.search(q, 0, "name")})
```

### `sys.searchnum("<free text>", index, "field")` → NUMBER

Numeric twin of `sys.search` for `if`/routing math: `count` (index ignored),
`lat`, `lon` (feed straight into `sys.navroute`). `-9999` while loading / out of
range (guard `>= 0`); real `0` = no hits.

### `sys.coord("lat,lon|Name", "field")` → origin/stop/destination accessor

Reads back a place a card stored in ONE scalar state key (survives across
screens, feeds routing). Format `"lat,lon"` or `"lat,lon|Display Name"`.
Fields: `lat`/`lon` → NUMBER (feed `sys.navroute`), `latlon` → clean `"lat,lon"`
STRING (feed the `vias` arg), `name` → the display name STRING. `-9999` when
unparseable (guard `>= -900`).

### `sys.navroute(lat1, lon1, lat2, lon2, "field", vias)` → STRING — the route (OSRM)

`vias` (optional, last arg) is a `"lat,lon;lat,lon"` string of intermediate
stops. Fields: `polyline` (encoded route → feed `set_nav_polyline`), `km`
(`"12.3 km"`), `min` (`"18 min"` drive), `walk`/`bike` (formatted estimates).

```
ui.themap.set_nav_polyline(sys.navroute(olat, olon, dlat, dlon, "polyline", vias))
ui.eta.set_text(sys.navroute(olat, olon, dlat, dlon, "min", vias))
```

### `sys.navroutenum(lat1, lon1, lat2, lon2, "min"|"km", vias)` → NUMBER

Same cached route as `sys.navroute`, as a number for arithmetic (arrival math,
constant-speed clock). `-1` while loading.

### `sys.navstep(lat1, lon1, lat2, lon2, progress_m, "field", vias)` → STRING — the turn banner

The live turn-by-turn banner at `progress_m` meters into the route. Fields:
`instr` (upcoming maneuver, e.g. "Turn left onto South Market Street"), `dist`
(counting-down distance to it), `arrow` (its glyph) / `next_arrow`, `road`
(current street), `rem` (distance remaining), `remmin` (minutes remaining),
`lane0`..`lane5` + `lane0hot`..`lane5hot` (lane guidance). At the end: "Arrived
at destination" / 🏁.

### `sys.navstepnum(…, progress_m, "frac"|"dist")` → NUMBER

`frac` = trip fraction 0..1 (progress bars); `dist` = meters to the next
maneuver (gate lane strips on `< 320`). `-1` while loading.

### `sys.navsecs(period)` → NUMBER — the no-rebuild drive clock

A looping wall-ish clock (`seconds_since_start % period`) that — unlike
`sys.simsecs` — does **NOT** arm the 1 Hz rebuild pump. Use it ONLY inside a
`fn tick()` card that mutates named widgets in place (`ui.<id>.set_*`) and must
never rebuild (a rebuild would tear down the live `MapView`). Drive the vehicle
along the route at a constant ~34 mph (15.2 m/s) by passing the trip's duration
as the period and scaling back to meters:

```
let km = sys.navroutenum(olat, olon, dlat, dlon, "km", vias)
let d  = sys.navsecs(km * 1000 / 15.2) * 15.2     // 0 → total_m, then loops
ui.instr.set_text(sys.navstep(olat, olon, dlat, dlon, d, "instr", vias))
```

## MapView — navigation extensions

The `MapView{}` widget (see the SYNTAX MANUAL for the base map, batching, and the
**fixed-pixel-height** rule) gains four nav properties + two script methods here:

| Property | Values | Purpose |
|----------|--------|---------|
| `nav_mode` | `"plan"` / `"2d"` / `"3d"` / (unset = flat) | `plan` = top-down route overview with flat teardrop pins; `2d` = top-down follow-cam; `3d` = tilted 2.5D first-person chase with upright billboard pins + a vehicle puck |
| `nav_period` | number (e.g. `100`) | drive-camera loop period |
| `nav_route_width` | number | route ribbon width in the chosen mode (wide ≈ `40` for `plan`, ~`11`–`14` for driving) |

Script methods (call every frame from `fn tick()`):

- `ui.<id>.set_nav_polyline("<encoded polyline>")` — push the OSRM route
  (`sys.navroute(…, "polyline", …)`) into the map to draw the route ribbon.
- `ui.<id>.set_route_markers("lat,lon,kind;lat,lon,kind;…")` — annotate the map.
  `kind` **0 = origin (green), 1 = via/stop (blue), 2 = destination (red)**. In
  `plan` mode these render as flat map teardrops; in `3d` they stand up as
  billboard pins. Build the string once at the top (`mk` in the card) and reuse
  it for every screen's map.

## MANDATORY rules (do not violate — they are why the card works)

- **Root is `flow: Overlay`, `new_batch: true`, fixed `height: 812`.** Every
  `MapView` gets a **fixed pixel height** (`812`, `452`, `384` — NEVER `Fill`/
  `Fit`, which resolve to 0 and hide the map).
- **The `drive` screen is a `fn tick()` no-rebuild card.** It ONLY calls
  `ui.<id>.set_*` on named widgets and uses `sys.navsecs` (not `sys.simsecs`).
  Never introduce anything that forces a rebuild while driving — it would
  destroy the live map. (search/preview/plan may rebuild freely.)
- **Every tappable control is a transparent `Button` overlaid on a fixed-size
  or `Fill`-width parent** inside a `flow: Overlay`. A `Button{ width: Fill
  height: Fill }` inside a `width: Fit` parent gets NO hit area — keep the
  card's existing `View{ width: <fixed> … flow: Overlay }` / `View{ width: Fill
  … }` wrappers exactly.
- **State only ever changes via `agent.notify("set", {key, value})`;** block
  closures `|| { notify(...); notify(...) }` fire several at once (e.g. set the
  origin AND clear `find`). Keep the keys exactly: `q sel dest find orig wp1 wp2
  mode go view`.
- **Brace balance is exact.** The canonical card is `{`-balanced; reproduce it
  to the final `}`. A dropped brace renders blank.

## The canonical card (served verbatim by the runtime, from `exemplars/trip-planner.splash`)

```splash
// name: nav-app
let q = "{{state.q}}"
let find = "{{state.find}}"
let orig = "{{state.orig}}"
let dest = "{{state.dest}}"
let wp1 = "{{state.wp1}}"
let wp2 = "{{state.wp2}}"
let sel = "{{state.sel}}"
let ss = {{state.sel}}
let go = "{{state.go}}"
let vw = "{{state.view}}"
let md = "{{state.mode}}"
let oq = "{{state.oq}}"

// origin — defaults to a San Jose start. An intent-seeded origin QUERY (oq,
// e.g. "Saratoga High School" parsed from "from A to B") resolves to its top
// search hit; an explicitly picked origin (find == "orig") still wins.
let olat = 37.3350
let olon = -121.8850
let oname = "San Jose (downtown)"
// Prefer the DEVICE's real GPS fix as the start when the trip names no origin.
// sys.gps reads the last LocationListener fix synchronously ("ok" < 1 until the
// first fix lands, then this takes over from the SJ default). An intent origin
// (oq) or an explicitly picked origin (orig) below still overrides it.
if sys.gps("ok") >= 1 {
    olat = sys.gps("lat")
    olon = sys.gps("lon")
    oname = "Your location"
}
if oq != "0" {
    if sys.searchnum(oq, 0, "lat") >= -900 {
        olat = sys.searchnum(oq, 0, "lat")
        olon = sys.searchnum(oq, 0, "lon")
        oname = sys.search(oq, 0, "name")
    }
}
if orig != "0" {
    olat = sys.coord(orig, "lat")
    olon = sys.coord(orig, "lon")
    oname = sys.coord(orig, "name")
}
let dlat = sys.coord(dest, "lat")
let dlon = sys.coord(dest, "lon")

// selected search result (destination preview)
let plat = 0
let plon = 0
if sel != "0" {
    plat = sys.searchnum(q, ss - 1, "lat")
    plon = sys.searchnum(q, ss - 1, "lon")
}

// waypoints -> OSRM vias string ("lat,lon;lat,lon")
let vias = ""
if wp1 != "0" { vias = "" + sys.coord(wp1, "lat") + "," + sys.coord(wp1, "lon") }
if wp2 != "0" {
    if vias != "" { vias = vias + ";" }
    vias = vias + sys.coord(wp2, "lat") + "," + sys.coord(wp2, "lon")
}

// map annotation pins: origin(0);stop(1);stop(1);destination(2)
let mk = "" + olat + "," + olon + ",0"
if wp1 != "0" { mk = mk + ";" + sys.coord(wp1, "lat") + "," + sys.coord(wp1, "lon") + ",1" }
if wp2 != "0" { mk = mk + ";" + sys.coord(wp2, "lat") + "," + sys.coord(wp2, "lon") + ",1" }
mk = mk + ";" + dlat + "," + dlon + ",2"

// normalized travel mode (default drive)
let mdn = md
if md == "0" { mdn = "drive" }

// screen selector
let scr = "search"
if find == "orig" { scr = "find" }
if find == "stop" { scr = "find" }
if find == "dest" { scr = "find" }
if dest == "0" { if find == "0" { if sel != "0" { scr = "preview" } } }
if dest != "0" { if find == "0" { scr = "plan" } }
if go == "1" { scr = "drive" }

fn tick() {
    // A CHANGED destination (find -> "dest") sets `dest` = "lat,lon|name".
    // Re-resolve its coords here too (top-level dlat/dlon freeze at build) so the
    // route + end pin follow the new destination. sys.coord is synchronous.
    if dest != "0" {
        dlat = sys.coord(dest, "lat")
        dlon = sys.coord(dest, "lon")
    }
    // Re-resolve WAYPOINTS -> the OSRM `vias` string here too (top-level freezes
    // at build): otherwise ADDING A STOP never reroutes the trip through it.
    vias = ""
    if wp1 != "0" { vias = "" + sys.coord(wp1, "lat") + "," + sys.coord(wp1, "lon") }
    if wp2 != "0" {
        if vias != "" { vias = vias + ";" }
        vias = vias + sys.coord(wp2, "lat") + "," + sys.coord(wp2, "lon")
    }
    // The intent may seed an ORIGIN query (oq, parsed from "from A to B"). The
    // top-level origin resolution FREEZES at build time — before oq's search
    // lands — so re-resolve it here EVERY tick (tick re-runs on live data) and
    // rebuild the marker string, so the route starts from the requested origin
    // instead of the San Jose default. An explicitly picked origin still wins.
    if orig == "0" { if oq != "0" { if sys.searchnum(oq, 0, "lat") >= -900 {
        olat = sys.searchnum(oq, 0, "lat")
        olon = sys.searchnum(oq, 0, "lon")
        oname = sys.search(oq, 0, "name")
        mk = "" + olat + "," + olon + ",0"
        if wp1 != "0" { mk = mk + ";" + sys.coord(wp1, "lat") + "," + sys.coord(wp1, "lon") + ",1" }
        if wp2 != "0" { mk = mk + ";" + sys.coord(wp2, "lat") + "," + sys.coord(wp2, "lon") + ",1" }
        mk = mk + ";" + dlat + "," + dlon + ",2"
    } } }
    // No named origin (no picked `orig`, no intent `oq`) -> follow the DEVICE GPS.
    // GPS may land AFTER build, so (re)resolve it here each tick and rebuild the
    // marker string, exactly like the oq path above.
    if orig == "0" { if oq == "0" { if sys.gps("ok") >= 1 {
        olat = sys.gps("lat")
        olon = sys.gps("lon")
        oname = "Your location"
        mk = "" + olat + "," + olon + ",0"
        if wp1 != "0" { mk = mk + ";" + sys.coord(wp1, "lat") + "," + sys.coord(wp1, "lon") + ",1" }
        if wp2 != "0" { mk = mk + ";" + sys.coord(wp2, "lat") + "," + sys.coord(wp2, "lon") + ",1" }
        mk = mk + ";" + dlat + "," + dlon + ",2"
    } } }
    // An explicitly PICKED start point (find -> "orig") sets `orig` = "lat,lon|name".
    // Re-resolve it here too (not only at the top level, which freezes at build):
    // otherwise "Change" the start point has no effect and the origin stays at the
    // intent's oq / default. sys.coord is synchronous so this lands immediately.
    if orig != "0" {
        olat = sys.coord(orig, "lat")
        olon = sys.coord(orig, "lon")
        oname = sys.coord(orig, "name")
        mk = "" + olat + "," + olon + ",0"
        if wp1 != "0" { mk = mk + ";" + sys.coord(wp1, "lat") + "," + sys.coord(wp1, "lon") + ",1" }
        if wp2 != "0" { mk = mk + ";" + sys.coord(wp2, "lat") + "," + sys.coord(wp2, "lon") + ",1" }
        mk = mk + ";" + dlat + "," + dlon + ",2"
    }
    // ---- search / find overlay: live results for the typed query ----
    if scr == "search" { if q != "0" {
        // sys.searchnum "count" returns a large sentinel until results land (or
        // if the fetch fails) — that's the "9999+ results" you saw. A real Photon
        // count is small (<= the request limit), so show a loading label above it.
        let sc = sys.searchnum(q, 0, "count")
        if sc > 500 { ui.cnt.set_text("Searching…") }
        if sc <= 500 { ui.cnt.set_text("" + sc + " results") }
        ui.sr0n.set_text(sys.search(q, 0, "name"))
        ui.sr0c.set_text(sys.search(q, 0, "cat"))
        ui.sr0l.set_text(sys.search(q, 0, "label"))
        ui.sr1n.set_text(sys.search(q, 1, "name"))
        ui.sr1c.set_text(sys.search(q, 1, "cat"))
        ui.sr1l.set_text(sys.search(q, 1, "label"))
        ui.sr2n.set_text(sys.search(q, 2, "name"))
        ui.sr2c.set_text(sys.search(q, 2, "cat"))
        ui.sr2l.set_text(sys.search(q, 2, "label"))
        ui.sr3n.set_text(sys.search(q, 3, "name"))
        ui.sr3c.set_text(sys.search(q, 3, "cat"))
        ui.sr3l.set_text(sys.search(q, 3, "label"))
        ui.sr4n.set_text(sys.search(q, 4, "name"))
        ui.sr4c.set_text(sys.search(q, 4, "cat"))
        ui.sr4l.set_text(sys.search(q, 4, "label"))
    } }
    if scr == "find" {
        ui.ftot.set_text(sys.navroute(olat, olon, dlat, dlon, "min", vias) + "  ·  " + sys.navroute(olat, olon, dlat, dlon, "km", vias))
        // keep the origin name current (inline text: oname freezes before the
        // origin geocode lands; the tick has the resolved value).
        if find != "orig" { ui.ffromn.set_text(oname) }
        if q == "0" { ui.ytstart.set_text(oname) }
    }
    if scr == "find" { if q != "0" {
        let fc = sys.searchnum(q, 0, "count")
        if fc > 500 { ui.fcnt.set_text("Searching…") }
        if fc <= 500 { ui.fcnt.set_text("" + fc + " results") }
        ui.fr0n.set_text(sys.search(q, 0, "name"))
        ui.fr0l.set_text(sys.search(q, 0, "label"))
        ui.fr1n.set_text(sys.search(q, 1, "name"))
        ui.fr1l.set_text(sys.search(q, 1, "label"))
        ui.fr2n.set_text(sys.search(q, 2, "name"))
        ui.fr2l.set_text(sys.search(q, 2, "label"))
        ui.fr3n.set_text(sys.search(q, 3, "name"))
        ui.fr3l.set_text(sys.search(q, 3, "label"))
    } }
    // ---- destination preview ----
    // Re-read the picked result's coords EVERY tick (not the once-computed
    // top-level plat/plon): when the card opens straight on preview (intent
    // seeded the destination) the search hasn't resolved yet, so top-level
    // coords are -9999; recomputing here lets the route/ETA/map fill in the
    // moment sys.search lands.
    if scr == "preview" {
        let pplat = sys.searchnum(q, ss - 1, "lat")
        let pplon = sys.searchnum(q, ss - 1, "lon")
        ui.pvname.set_text(sys.search(q, ss - 1, "name"))
        ui.pvcat.set_text(sys.search(q, ss - 1, "cat"))
        ui.pvaddr.set_text(sys.search(q, ss - 1, "label"))
        ui.pveta.set_text(sys.navroute(olat, olon, pplat, pplon, "min") + "  ·  " + sys.navroute(olat, olon, pplat, pplon, "km"))
        ui.pmap.set_nav_polyline(sys.navroute(olat, olon, pplat, pplon, "polyline"))
        ui.pmap.set_route_markers("" + olat + "," + olon + ",0;" + pplat + "," + pplon + ",2")
    }
    // ---- plan ----
    if scr == "plan" {
        ui.oname.set_text(oname)
        ui.dname.set_text(sys.coord(dest, "name"))
        if md == "walk" { ui.eta.set_text(sys.navroute(olat, olon, dlat, dlon, "walk", vias)) }
        if md == "bike" { ui.eta.set_text(sys.navroute(olat, olon, dlat, dlon, "bike", vias)) }
        if md == "drive" { ui.eta.set_text(sys.navroute(olat, olon, dlat, dlon, "min", vias)) }
        if md == "0" { ui.eta.set_text(sys.navroute(olat, olon, dlat, dlon, "min", vias)) }
        ui.etad.set_text(sys.navroute(olat, olon, dlat, dlon, "km", vias))
        ui.themap.set_nav_polyline(sys.navroute(olat, olon, dlat, dlon, "polyline", vias))
        ui.themap.set_route_markers(mk)
        if wp1 != "0" { ui.wp1n.set_text(sys.coord(wp1, "name")) }
        if wp2 != "0" { ui.wp2n.set_text(sys.coord(wp2, "name")) }
        // per-leg time · distance
        if wp1 != "0" {
            ui.leg0.set_text(sys.navroute(olat, olon, sys.coord(wp1, "lat"), sys.coord(wp1, "lon"), "min") + "  ·  " + sys.navroute(olat, olon, sys.coord(wp1, "lat"), sys.coord(wp1, "lon"), "km"))
            if wp2 != "0" {
                ui.leg1.set_text(sys.navroute(sys.coord(wp1, "lat"), sys.coord(wp1, "lon"), sys.coord(wp2, "lat"), sys.coord(wp2, "lon"), "min") + "  ·  " + sys.navroute(sys.coord(wp1, "lat"), sys.coord(wp1, "lon"), sys.coord(wp2, "lat"), sys.coord(wp2, "lon"), "km"))
                ui.leg2.set_text(sys.navroute(sys.coord(wp2, "lat"), sys.coord(wp2, "lon"), dlat, dlon, "min") + "  ·  " + sys.navroute(sys.coord(wp2, "lat"), sys.coord(wp2, "lon"), dlat, dlon, "km"))
            }
            if wp2 == "0" {
                ui.leg1.set_text(sys.navroute(sys.coord(wp1, "lat"), sys.coord(wp1, "lon"), dlat, dlon, "min") + "  ·  " + sys.navroute(sys.coord(wp1, "lat"), sys.coord(wp1, "lon"), dlat, dlon, "km"))
            }
        }
    }
    // ---- drive (turn-by-turn) ----
    if scr == "drive" {
        let km = sys.navroutenum(olat, olon, dlat, dlon, "km", vias)
        let d = sys.navsecs(km * 1000 / 15.2) * 15.2
        ui.dvmap.set_nav_polyline(sys.navroute(olat, olon, dlat, dlon, "polyline", vias))
        ui.dvmap.set_route_markers(mk)
        ui.instr.set_text(sys.navstep(olat, olon, dlat, dlon, d, "instr", vias))
        ui.arw.set_text(sys.navstep(olat, olon, dlat, dlon, d, "arrow", vias))
        ui.ndist.set_text(sys.navstep(olat, olon, dlat, dlon, d, "dist", vias))
        let road = sys.navstep(olat, olon, dlat, dlon, d, "road", vias)
        ui.roadlbl.set_text(road)
        ui.roadpill.set_visible(road != "")
        ui.remmin.set_text(sys.navstep(olat, olon, dlat, dlon, d, "remmin", vias) + " min")
        ui.remrest.set_text(sys.navstep(olat, olon, dlat, dlon, d, "rem", vias) + "  ·  34 mph")
    }
}

SolidView{ width: Fill height: 812 flow: Overlay new_batch: true draw_bg.color: #0b1017

  // ===================== MAP BACKDROP =====================
  if scr == "drive" {
    if vw == "2d" { dvmap := MapView{ width: Fill height: 812 nav_mode: "2d" nav_period: 100 nav_route_width: 11.0 zoom: 16.0 min_zoom: 3.0 max_zoom: 19.0 use_network: true use_local_mbtiles: false } }
    if vw != "2d" { dvmap := MapView{ width: Fill height: 812 nav_mode: "3d" nav_period: 100 nav_route_width: 14.0 zoom: 15.0 min_zoom: 3.0 max_zoom: 19.0 use_network: true use_local_mbtiles: false } }
    if vw != "2d" { GradientYView{ width: Fill height: 150 draw_bg.color: #8fb2d8 draw_bg.color_2: #d8e3ed } }
  }

  // ===================== DESTINATION SEARCH =====================
  if scr == "search" {
    View{ width: Fill height: Fill flow: Down padding: Inset{left: 16 top: 50 right: 16 bottom: 10} spacing: 12
      RoundedView{ width: Fill height: 52 flow: Right align: Align{y: 0.5} draw_bg.color: #141c28 draw_bg.border_radius: 26 draw_bg.border_size: 1.0 draw_bg.border_color: #2a3644 padding: Inset{left: 19 right: 18} spacing: 12
        View{ width: 20 height: 20 flow: Overlay
          CircleView{ width: 15 height: 15 draw_bg.color: #8b99b0 }
          CircleView{ width: 10 height: 10 margin: Inset{left: 2.5 top: 2.5} draw_bg.color: #141c28 }
          RoundedView{ width: 7.5 height: 2.7 margin: Inset{left: 11 top: 13.5} draw_bg.color: #8b99b0 draw_bg.border_radius: 1.3 }
        }
        TextInput{ width: Fill height: Fill empty_text: "Search a destination" return_key_type: Send draw_bg.color: #00000000 draw_bg.color_hover: #00000000 draw_bg.color_focus: #00000000 draw_bg.color_down: #00000000 draw_bg.color_empty: #00000000 draw_bg.border_size: 0.0 draw_text.color: #eaf0f7 draw_text.text_style.font_size: 15 on_return: |text| agent.notify("set", {key: "q", value: text}) }
      }
      cnt := Label{ text: "Type a place, then search" draw_text.color: #7c8aa6 draw_text.text_style.font_size: 12 }
      if q != "0" {
      RoundedView{ width: Fill height: Fit flow: Down draw_bg.color: #141b26 draw_bg.border_radius: 16 padding: Inset{left: 6 top: 4 right: 6 bottom: 4}
        View{ width: Fill height: Fit flow: Overlay
          View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 padding: Inset{left: 10 top: 11 right: 10 bottom: 11}
            View{ width: 26 height: 26 flow: Down align: Align{x: 0.5 y: 0.5} Label{ text: "1" draw_text.color: #8fa0b8 draw_text.text_style.font_size: 13 } }
            View{ width: Fill height: Fit flow: Down spacing: 2
              sr0n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
              sr0c := Label{ width: Fill text: "" draw_text.color: #7d93b2 draw_text.text_style.font_size: 12 }
              sr0l := Label{ width: Fill text: "" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
            }
            Label{ text: "›" draw_text.color: #4a5568 draw_text.text_style.font_size: 20 }
          }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_focus: #00000000 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || agent.notify("set", {key: "sel", value: "1"}) }
        }
        SolidView{ width: Fill height: 1 draw_bg.color: #ffffff0d }
        View{ width: Fill height: Fit flow: Overlay
          View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 padding: Inset{left: 10 top: 11 right: 10 bottom: 11}
            View{ width: 26 height: 26 flow: Down align: Align{x: 0.5 y: 0.5} Label{ text: "2" draw_text.color: #8fa0b8 draw_text.text_style.font_size: 13 } }
            View{ width: Fill height: Fit flow: Down spacing: 2
              sr1n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
              sr1c := Label{ width: Fill text: "" draw_text.color: #7d93b2 draw_text.text_style.font_size: 12 }
              sr1l := Label{ width: Fill text: "" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
            }
            Label{ text: "›" draw_text.color: #4a5568 draw_text.text_style.font_size: 20 }
          }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_focus: #00000000 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || agent.notify("set", {key: "sel", value: "2"}) }
        }
        SolidView{ width: Fill height: 1 draw_bg.color: #ffffff0d }
        View{ width: Fill height: Fit flow: Overlay
          View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 padding: Inset{left: 10 top: 11 right: 10 bottom: 11}
            View{ width: 26 height: 26 flow: Down align: Align{x: 0.5 y: 0.5} Label{ text: "3" draw_text.color: #8fa0b8 draw_text.text_style.font_size: 13 } }
            View{ width: Fill height: Fit flow: Down spacing: 2
              sr2n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
              sr2c := Label{ width: Fill text: "" draw_text.color: #7d93b2 draw_text.text_style.font_size: 12 }
              sr2l := Label{ width: Fill text: "" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
            }
            Label{ text: "›" draw_text.color: #4a5568 draw_text.text_style.font_size: 20 }
          }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_focus: #00000000 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || agent.notify("set", {key: "sel", value: "3"}) }
        }
        SolidView{ width: Fill height: 1 draw_bg.color: #ffffff0d }
        View{ width: Fill height: Fit flow: Overlay
          View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 padding: Inset{left: 10 top: 11 right: 10 bottom: 11}
            View{ width: 26 height: 26 flow: Down align: Align{x: 0.5 y: 0.5} Label{ text: "4" draw_text.color: #8fa0b8 draw_text.text_style.font_size: 13 } }
            View{ width: Fill height: Fit flow: Down spacing: 2
              sr3n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
              sr3c := Label{ width: Fill text: "" draw_text.color: #7d93b2 draw_text.text_style.font_size: 12 }
              sr3l := Label{ width: Fill text: "" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
            }
            Label{ text: "›" draw_text.color: #4a5568 draw_text.text_style.font_size: 20 }
          }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_focus: #00000000 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || agent.notify("set", {key: "sel", value: "4"}) }
        }
        SolidView{ width: Fill height: 1 draw_bg.color: #ffffff0d }
        View{ width: Fill height: Fit flow: Overlay
          View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 padding: Inset{left: 10 top: 11 right: 10 bottom: 11}
            View{ width: 26 height: 26 flow: Down align: Align{x: 0.5 y: 0.5} Label{ text: "5" draw_text.color: #8fa0b8 draw_text.text_style.font_size: 13 } }
            View{ width: Fill height: Fit flow: Down spacing: 2
              sr4n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
              sr4c := Label{ width: Fill text: "" draw_text.color: #7d93b2 draw_text.text_style.font_size: 12 }
              sr4l := Label{ width: Fill text: "" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
            }
            Label{ text: "›" draw_text.color: #4a5568 draw_text.text_style.font_size: 20 }
          }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_focus: #00000000 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || agent.notify("set", {key: "sel", value: "5"}) }
        }
      }
      }
    }
  }

  // ===================== ORIGIN / STOP SEARCH OVERLAY =====================
  if scr == "find" {
    SolidView{ width: Fill height: 812 flow: Down new_batch: true draw_bg.color: #0b1017 padding: Inset{left: 16 top: 50 right: 16 bottom: 10} spacing: 12
      View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 10
        View{ width: 85 height: 34 flow: Overlay
          RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #222c3c draw_bg.border_radius: 10 padding: Inset{left: 12 top: 8 right: 12 bottom: 8} Label{ text: "‹ Back" draw_text.color: #cdd8e6 draw_text.text_style.font_size: 13 } }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "find", value: "0"}) }
        }
        View{ width: Fill height: Fit flow: Right align: Align{x: 1.0 y: 0.5} ftot := Label{ text: "" draw_text.color: #9fb0c4 draw_text.text_style.font_size: 14 } }
      }
      // FROM/TO stay visible while searching (Google-style): the field being
      // edited is the active input; the other shows its current value and is
      // tappable to switch to editing it.
      View{ width: Fill height: 46 flow: Right align: Align{y: 0.5} spacing: 12
        CircleView{ width: 11 height: 11 draw_bg.color: #22c55e }
        if find == "orig" {
          RoundedView{ width: Fill height: 44 flow: Right align: Align{y: 0.5} draw_bg.color: #172231 draw_bg.border_radius: 12 draw_bg.border_size: 1.2 draw_bg.border_color: #3f7bc4 padding: Inset{left: 16 right: 14}
            TextInput{ width: Fill height: Fill empty_text: oname return_key_type: Send draw_bg.color: #00000000 draw_bg.color_hover: #00000000 draw_bg.color_focus: #00000000 draw_bg.color_down: #00000000 draw_bg.color_empty: #00000000 draw_bg.border_size: 0.0 draw_text.color: #eaf0f7 draw_text.text_style.font_size: 15 on_return: |text| agent.notify("set", {key: "q", value: text}) }
          }
        }
        if find != "orig" {
          View{ width: Fill height: 44 flow: Overlay
            RoundedView{ width: Fill height: Fill flow: Right align: Align{y: 0.5} draw_bg.color: #141c28 draw_bg.border_radius: 12 padding: Inset{left: 16 right: 14} ffromn := Label{ width: Fill text: oname draw_text.color: #cdd8e6 draw_text.text_style.font_size: 15 } }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_down: #ffffff10 draw_bg.border_size: 0.0 draw_bg.border_radius: 12 text: "" on_click: || { agent.notify("set", {key: "find", value: "orig"}); agent.notify("set", {key: "q", value: "0"}) } }
          }
        }
      }
      View{ width: Fill height: 46 flow: Right align: Align{y: 0.5} spacing: 12
        CircleView{ width: 11 height: 11 draw_bg.color: #ff6b6b }
        if find == "dest" {
          RoundedView{ width: Fill height: 44 flow: Right align: Align{y: 0.5} draw_bg.color: #172231 draw_bg.border_radius: 12 draw_bg.border_size: 1.2 draw_bg.border_color: #3f7bc4 padding: Inset{left: 16 right: 14}
            TextInput{ width: Fill height: Fill empty_text: sys.coord(dest, "name") return_key_type: Send draw_bg.color: #00000000 draw_bg.color_hover: #00000000 draw_bg.color_focus: #00000000 draw_bg.color_down: #00000000 draw_bg.color_empty: #00000000 draw_bg.border_size: 0.0 draw_text.color: #eaf0f7 draw_text.text_style.font_size: 15 on_return: |text| agent.notify("set", {key: "q", value: text}) }
          }
        }
        if find != "dest" {
          View{ width: Fill height: 44 flow: Overlay
            RoundedView{ width: Fill height: Fill flow: Right align: Align{y: 0.5} draw_bg.color: #141c28 draw_bg.border_radius: 12 padding: Inset{left: 16 right: 14} Label{ width: Fill text: sys.coord(dest, "name") draw_text.color: #ffffff draw_text.text_style.font_size: 15 } }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_down: #ffffff10 draw_bg.border_size: 0.0 draw_bg.border_radius: 12 text: "" on_click: || { agent.notify("set", {key: "find", value: "dest"}); agent.notify("set", {key: "q", value: "0"}) } }
          }
        }
      }
      if find == "stop" {
        View{ width: Fill height: 46 flow: Right align: Align{y: 0.5} spacing: 12
          CircleView{ width: 11 height: 11 draw_bg.color: #2c7be5 }
          RoundedView{ width: Fill height: 44 flow: Right align: Align{y: 0.5} draw_bg.color: #172231 draw_bg.border_radius: 12 draw_bg.border_size: 1.2 draw_bg.border_color: #3f7bc4 padding: Inset{left: 16 right: 14}
            TextInput{ width: Fill height: Fill empty_text: "Add a stop" return_key_type: Send draw_bg.color: #00000000 draw_bg.color_hover: #00000000 draw_bg.color_focus: #00000000 draw_bg.color_down: #00000000 draw_bg.color_empty: #00000000 draw_bg.border_size: 0.0 draw_text.color: #eaf0f7 draw_text.text_style.font_size: 15 on_return: |text| agent.notify("set", {key: "q", value: text}) }
          }
        }
      }
      fcnt := Label{ text: "Your trip" draw_text.color: #7c8aa6 draw_text.text_style.font_size: 12 }
      // Default items (before a search): the current start + destination, so you
      // can pick either without typing (tap the destination while editing the
      // start to reverse the trip, etc.). Replaced by live results once you type.
      if q == "0" {
        RoundedView{ width: Fill height: Fit flow: Down draw_bg.color: #141b26 draw_bg.border_radius: 16 padding: Inset{left: 6 top: 4 right: 6 bottom: 4}
          View{ width: Fill height: Fit flow: Overlay
            View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 padding: Inset{left: 12 top: 12 right: 14 bottom: 12}
              CircleView{ width: 9 height: 9 draw_bg.color: #22c55e }
              View{ width: Fill height: Fit flow: Down spacing: 2
                ytstart := Label{ width: Fill text: oname draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
                Label{ width: Fill text: "Start point" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
              }
            }
            if find == "orig" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "orig", value: "" + olat + "," + olon + "|" + oname}); agent.notify("set", {key: "find", value: "0"}) } } }
            if find == "dest" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "dest", value: "" + olat + "," + olon + "|" + oname}); agent.notify("set", {key: "find", value: "0"}) } } }
            if find == "stop" { if wp1 == "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp1", value: "" + olat + "," + olon + "|" + oname}); agent.notify("set", {key: "find", value: "0"}) } } } }
            if find == "stop" { if wp1 != "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp2", value: "" + olat + "," + olon + "|" + oname}); agent.notify("set", {key: "find", value: "0"}) } } } }
          }
          SolidView{ width: Fill height: 1 draw_bg.color: #ffffff0d }
          View{ width: Fill height: Fit flow: Overlay
            View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 padding: Inset{left: 12 top: 12 right: 14 bottom: 12}
              CircleView{ width: 9 height: 9 draw_bg.color: #ff6b6b }
              View{ width: Fill height: Fit flow: Down spacing: 2
                Label{ width: Fill text: sys.coord(dest, "name") draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
                Label{ width: Fill text: "Destination" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
              }
            }
            if find == "orig" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "orig", value: dest}); agent.notify("set", {key: "find", value: "0"}) } } }
            if find == "dest" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "dest", value: dest}); agent.notify("set", {key: "find", value: "0"}) } } }
            if find == "stop" { if wp1 == "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp1", value: dest}); agent.notify("set", {key: "find", value: "0"}) } } } }
            if find == "stop" { if wp1 != "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp2", value: dest}); agent.notify("set", {key: "find", value: "0"}) } } } }
          }
        }
      }
      if q != "0" {
      RoundedView{ width: Fill height: Fit flow: Down draw_bg.color: #141b26 draw_bg.border_radius: 16 padding: Inset{left: 6 top: 4 right: 6 bottom: 4}
        View{ width: Fill height: Fit flow: Overlay
          View{ width: Fill height: Fit flow: Down spacing: 2 padding: Inset{left: 14 top: 12 right: 14 bottom: 12}
            fr0n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
            fr0l := Label{ width: Fill text: "" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
          }
          if find == "orig" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "orig", value: sys.search(q, 0, "lat") + "," + sys.search(q, 0, "lon") + "|" + sys.search(q, 0, "name")}); agent.notify("set", {key: "find", value: "0"}) } } }
          if find == "dest" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "dest", value: sys.search(q, 0, "lat") + "," + sys.search(q, 0, "lon") + "|" + sys.search(q, 0, "name")}); agent.notify("set", {key: "find", value: "0"}) } } }
          if find == "stop" { if wp1 == "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp1", value: sys.search(q, 0, "lat") + "," + sys.search(q, 0, "lon") + "|" + sys.search(q, 0, "name")}); agent.notify("set", {key: "find", value: "0"}) } } } }
          if find == "stop" { if wp1 != "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp2", value: sys.search(q, 0, "lat") + "," + sys.search(q, 0, "lon") + "|" + sys.search(q, 0, "name")}); agent.notify("set", {key: "find", value: "0"}) } } } }
        }
        SolidView{ width: Fill height: 1 draw_bg.color: #ffffff0d }
        View{ width: Fill height: Fit flow: Overlay
          View{ width: Fill height: Fit flow: Down spacing: 2 padding: Inset{left: 14 top: 12 right: 14 bottom: 12}
            fr1n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
            fr1l := Label{ width: Fill text: "" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
          }
          if find == "orig" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "orig", value: sys.search(q, 1, "lat") + "," + sys.search(q, 1, "lon") + "|" + sys.search(q, 1, "name")}); agent.notify("set", {key: "find", value: "0"}) } } }
          if find == "dest" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "dest", value: sys.search(q, 1, "lat") + "," + sys.search(q, 1, "lon") + "|" + sys.search(q, 1, "name")}); agent.notify("set", {key: "find", value: "0"}) } } }
          if find == "stop" { if wp1 == "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp1", value: sys.search(q, 1, "lat") + "," + sys.search(q, 1, "lon") + "|" + sys.search(q, 1, "name")}); agent.notify("set", {key: "find", value: "0"}) } } } }
          if find == "stop" { if wp1 != "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp2", value: sys.search(q, 1, "lat") + "," + sys.search(q, 1, "lon") + "|" + sys.search(q, 1, "name")}); agent.notify("set", {key: "find", value: "0"}) } } } }
        }
        SolidView{ width: Fill height: 1 draw_bg.color: #ffffff0d }
        View{ width: Fill height: Fit flow: Overlay
          View{ width: Fill height: Fit flow: Down spacing: 2 padding: Inset{left: 14 top: 12 right: 14 bottom: 12}
            fr2n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
            fr2l := Label{ width: Fill text: "" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
          }
          if find == "orig" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "orig", value: sys.search(q, 2, "lat") + "," + sys.search(q, 2, "lon") + "|" + sys.search(q, 2, "name")}); agent.notify("set", {key: "find", value: "0"}) } } }
          if find == "dest" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "dest", value: sys.search(q, 2, "lat") + "," + sys.search(q, 2, "lon") + "|" + sys.search(q, 2, "name")}); agent.notify("set", {key: "find", value: "0"}) } } }
          if find == "stop" { if wp1 == "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp1", value: sys.search(q, 2, "lat") + "," + sys.search(q, 2, "lon") + "|" + sys.search(q, 2, "name")}); agent.notify("set", {key: "find", value: "0"}) } } } }
          if find == "stop" { if wp1 != "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp2", value: sys.search(q, 2, "lat") + "," + sys.search(q, 2, "lon") + "|" + sys.search(q, 2, "name")}); agent.notify("set", {key: "find", value: "0"}) } } } }
        }
        SolidView{ width: Fill height: 1 draw_bg.color: #ffffff0d }
        View{ width: Fill height: Fit flow: Overlay
          View{ width: Fill height: Fit flow: Down spacing: 2 padding: Inset{left: 14 top: 12 right: 14 bottom: 12}
            fr3n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
            fr3l := Label{ width: Fill text: "" draw_text.color: #63708a draw_text.text_style.font_size: 11 }
          }
          if find == "orig" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "orig", value: sys.search(q, 3, "lat") + "," + sys.search(q, 3, "lon") + "|" + sys.search(q, 3, "name")}); agent.notify("set", {key: "find", value: "0"}) } } }
          if find == "dest" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "dest", value: sys.search(q, 3, "lat") + "," + sys.search(q, 3, "lon") + "|" + sys.search(q, 3, "name")}); agent.notify("set", {key: "find", value: "0"}) } } }
          if find == "stop" { if wp1 == "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp1", value: sys.search(q, 3, "lat") + "," + sys.search(q, 3, "lon") + "|" + sys.search(q, 3, "name")}); agent.notify("set", {key: "find", value: "0"}) } } } }
          if find == "stop" { if wp1 != "0" { Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_hover: #ffffff08 draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "wp2", value: sys.search(q, 3, "lat") + "," + sys.search(q, 3, "lon") + "|" + sys.search(q, 3, "name")}); agent.notify("set", {key: "find", value: "0"}) } } } }
        }
      }
      }
    }
  }

  // ===================== DESTINATION PREVIEW =====================
  if scr == "preview" {
    View{ width: Fill height: Fill flow: Overlay
      pmap := MapView{ width: Fill height: 812 nav_mode: "plan" nav_route_width: 40.0 zoom: 14.0 min_zoom: 3.0 max_zoom: 16.0 use_network: true use_local_mbtiles: false }
      View{ width: Fill height: Fill flow: Down align: Align{x: 0.5 y: 1.0}
      RoundedView{ width: Fill height: Fit flow: Overlay draw_bg.color: #0f1620 draw_bg.border_radius: 22 margin: Inset{left: 8 right: 8 bottom: 30}
        // transparent full-bleed catch so touches on the sheet's empty areas don't
        // fall through to the map behind and pan it. Real buttons on top still win.
        Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || {} }
        View{ width: Fill height: Fit flow: Down padding: Inset{left: 14 top: 13 right: 14 bottom: 13} spacing: 7
        View{ width: 110 height: 24 flow: Overlay
          RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #222c3c draw_bg.border_radius: 10 padding: Inset{left: 12 top: 8 right: 12 bottom: 8} Label{ text: "‹ Results" draw_text.color: #cdd8e6 draw_text.text_style.font_size: 9 } }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "sel", value: "0"}) }
        }
        pvname := Label{ width: Fill text: "…" draw_text.color: #ffffff draw_text.text_style.font_size: 15 }
        View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12
          pvcat := Label{ text: "" draw_text.color: #f5a623 draw_text.text_style.font_size: 9 }
          pveta := Label{ text: "" draw_text.color: #4ade80 draw_text.text_style.font_size: 9 }
        }
        pvaddr := Label{ width: Fill text: "" draw_text.color: #9fb0c4 draw_text.text_style.font_size: 9 }
        View{ width: Fill height: 42 flow: Overlay margin: Inset{top: 6}
          RoundedView{ width: Fill height: 42 flow: Right align: Align{x: 0.5 y: 0.5} draw_bg.color: #1a73e8 draw_bg.border_radius: 14 Label{ text: "Directions  ›" draw_text.color: #ffffff draw_text.text_style.font_size: 10 } }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "dest", value: sys.searchnum(q, ss - 1, "lat") + "," + sys.searchnum(q, ss - 1, "lon") + "|" + sys.search(q, ss - 1, "name") }) }
        }
        }
      }
      }
    }
  }

  // ===================== PLAN =====================
  if scr == "plan" {
    View{ width: Fill height: Fill flow: Overlay
      themap := MapView{ width: Fill height: 812 nav_mode: "plan" nav_route_width: 40.0 zoom: 15.0 min_zoom: 3.0 max_zoom: 16.0 use_network: true use_local_mbtiles: false }
      View{ width: Fill height: Fill flow: Down align: Align{x: 0.5 y: 1.0}
      RoundedView{ width: Fill height: Fit flow: Overlay draw_bg.color: #0f1620 draw_bg.border_radius: 22 margin: Inset{left: 8 right: 8 bottom: 30}
        // transparent full-bleed catch: touches on the sheet's empty (non-button)
        // areas are captured HERE instead of falling through to the map behind and
        // panning it (the "sheet not responding" feel). Real buttons sit on top and win.
        Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || {} }
        View{ width: Fill height: Fit flow: Down padding: Inset{left: 14 top: 11 right: 14 bottom: 11} spacing: 7
        View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 8
          View{ width: 84 height: 24 flow: Overlay
            RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #222c3c draw_bg.border_radius: 9 padding: Inset{left: 10 top: 6 right: 10 bottom: 6} Label{ text: "‹ Search" draw_text.color: #cdd8e6 draw_text.text_style.font_size: 8 } }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "dest", value: "0"}) }
          }
          View{ width: 70 height: 24 flow: Overlay
            if mdn == "drive" { RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #1a73e8 draw_bg.border_radius: 9 Label{ text: "Drive" draw_text.color: #ffffff draw_text.text_style.font_size: 8 } } }
            if mdn != "drive" { RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #222c3c draw_bg.border_radius: 9 Label{ text: "Drive" draw_text.color: #cdd8e6 draw_text.text_style.font_size: 8 } } }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "mode", value: "drive"}) }
          }
          View{ width: 64 height: 24 flow: Overlay
            if mdn == "walk" { RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #1a73e8 draw_bg.border_radius: 9 Label{ text: "Walk" draw_text.color: #ffffff draw_text.text_style.font_size: 8 } } }
            if mdn != "walk" { RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #222c3c draw_bg.border_radius: 9 Label{ text: "Walk" draw_text.color: #cdd8e6 draw_text.text_style.font_size: 8 } } }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "mode", value: "walk"}) }
          }
          View{ width: 64 height: 24 flow: Overlay
            if mdn == "bike" { RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #1a73e8 draw_bg.border_radius: 9 Label{ text: "Bike" draw_text.color: #ffffff draw_text.text_style.font_size: 8 } } }
            if mdn != "bike" { RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #222c3c draw_bg.border_radius: 9 Label{ text: "Bike" draw_text.color: #cdd8e6 draw_text.text_style.font_size: 8 } } }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "mode", value: "bike"}) }
          }
        }
        View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 8
          eta := Label{ text: "…" draw_text.color: #4ade80 draw_text.text_style.font_size: 14 }
          etad := Label{ text: "" draw_text.color: #9fb0c4 draw_text.text_style.font_size: 9 }
        }
        SolidView{ width: Fill height: 1 draw_bg.color: #ffffff12 }
        // origin (editable) — the WHOLE row is tappable (Fill-width button,
        // reliable hit area) so tapping the start field opens the origin search.
        View{ width: Fill height: 30 flow: Overlay
          View{ width: Fill height: Fill flow: Right align: Align{y: 0.5} spacing: 12
            CircleView{ width: 11 height: 11 draw_bg.color: #22c55e }
            oname := Label{ width: Fill text: "…" draw_text.color: #cdd8e6 draw_text.text_style.font_size: 10 }          }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_down: #ffffff10 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "find", value: "orig"}); agent.notify("set", {key: "q", value: "0"}) } }
        }
        // per-leg time · distance (shown only when the trip has stops)
        if wp1 != "0" { View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 margin: Inset{top: 1 bottom: 1} View{ width: 11 height: 15 flow: Overlay align: Align{x: 0.5 y: 0.5} SolidView{ width: 2 height: 15 draw_bg.color: #2c7be5 } } leg0 := Label{ text: "" draw_text.color: #8b99b0 draw_text.text_style.font_size: 8 } } }
        // stop 1
        if wp1 != "0" {
          View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12
            CircleView{ width: 11 height: 11 draw_bg.color: #2c7be5 }
            wp1n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
            View{ width: 30 height: 30 flow: Overlay
              RoundedView{ width: 30 height: 30 flow: Right align: Align{x: 0.5 y: 0.5} draw_bg.color: #38445a draw_bg.border_radius: 15 Label{ text: "×" draw_text.color: #c6d0de draw_text.text_style.font_size: 10 } }
              Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "wp1", value: "0"}) }
            }
          }
        }
        if wp1 != "0" { View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 margin: Inset{top: 1 bottom: 1} View{ width: 11 height: 15 flow: Overlay align: Align{x: 0.5 y: 0.5} SolidView{ width: 2 height: 15 draw_bg.color: #2c7be5 } } leg1 := Label{ text: "" draw_text.color: #8b99b0 draw_text.text_style.font_size: 8 } } }
        // stop 2
        if wp2 != "0" {
          View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12
            CircleView{ width: 11 height: 11 draw_bg.color: #2c7be5 }
            wp2n := Label{ width: Fill text: "" draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
            View{ width: 30 height: 30 flow: Overlay
              RoundedView{ width: 30 height: 30 flow: Right align: Align{x: 0.5 y: 0.5} draw_bg.color: #38445a draw_bg.border_radius: 15 Label{ text: "×" draw_text.color: #c6d0de draw_text.text_style.font_size: 10 } }
              Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "wp2", value: "0"}) }
            }
          }
        }
        if wp2 != "0" { View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12 margin: Inset{top: 1 bottom: 1} View{ width: 11 height: 15 flow: Overlay align: Align{x: 0.5 y: 0.5} SolidView{ width: 2 height: 15 draw_bg.color: #2c7be5 } } leg2 := Label{ text: "" draw_text.color: #8b99b0 draw_text.text_style.font_size: 8 } } }
        // destination (editable) — the WHOLE row is tappable, like the origin,
        // so tapping the end field opens the destination search.
        View{ width: Fill height: 30 flow: Overlay
          View{ width: Fill height: Fill flow: Right align: Align{y: 0.5} spacing: 12
            CircleView{ width: 11 height: 11 draw_bg.color: #ff6b6b }
            dname := Label{ width: Fill text: "…" draw_text.color: #ffffff draw_text.text_style.font_size: 10 }          }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_down: #ffffff10 draw_bg.border_size: 0.0 draw_bg.border_radius: 10 text: "" on_click: || { agent.notify("set", {key: "find", value: "dest"}); agent.notify("set", {key: "q", value: "0"}) } }
        }
        // add stop (searchable) — only if a slot is free
        if wp2 == "0" {
          View{ width: 140 height: 34 flow: Overlay margin: Inset{top: 2}
            RoundedView{ width: Fill height: Fill flow: Right align: Align{x: 0.5 y: 0.5} draw_bg.color: #1b2534 draw_bg.border_radius: 18 spacing: 6
              Label{ text: "+" draw_text.color: #72c0ff draw_text.text_style.font_size: 10 }
              Label{ text: "Add stop" draw_text.color: #cdd8e6 draw_text.text_style.font_size: 9 }
            }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || { agent.notify("set", {key: "find", value: "stop"}); agent.notify("set", {key: "q", value: "0"}) } }
          }
        }
        View{ width: Fill height: 42 flow: Overlay margin: Inset{top: 4}
          RoundedView{ width: Fill height: 42 flow: Right align: Align{x: 0.5 y: 0.5} draw_bg.color: #1a73e8 draw_bg.border_radius: 14 Label{ text: "▶  Start" draw_text.color: #ffffff draw_text.text_style.font_size: 10 } }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "go", value: "1"}) }
        }
        }
      }
      }
      // map controls (top-right): +/- zoom pill, then a "my location" button that
      // recenters on the trip origin. Pinch-to-zoom and one-finger drag also work.
      View{ width: Fill height: Fill flow: Down align: Align{x: 1.0 y: 0.0} padding: Inset{top: 74 right: 12} spacing: 12
        RoundedView{ width: 50 height: 104 flow: Down align: Align{x: 0.5 y: 0.5} draw_bg.color: #172232 draw_bg.border_radius: 16
          View{ width: 50 height: 51 flow: Overlay
            Label{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} text: "+" draw_text.color: #eaf0f7 draw_text.text_style.font_size: 26 }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_down: #ffffff1a draw_bg.border_size: 0.0 draw_bg.border_radius: 14 text: "" on_click: || ui.themap.nav_zoom_by("0.7") }
          }
          SolidView{ width: 34 height: 1 draw_bg.color: #ffffff22 }
          View{ width: 50 height: 51 flow: Overlay
            Label{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} text: "−" draw_text.color: #eaf0f7 draw_text.text_style.font_size: 26 }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_down: #ffffff1a draw_bg.border_size: 0.0 draw_bg.border_radius: 14 text: "" on_click: || ui.themap.nav_zoom_by("-0.7") }
          }
        }
        View{ width: 50 height: 50 flow: Overlay
          RoundedView{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #172232 draw_bg.border_radius: 25 Label{ text: "◎" draw_text.color: #72c0ff draw_text.text_style.font_size: 24 } }
          Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_down: #ffffff1a draw_bg.border_size: 0.0 draw_bg.border_radius: 25 text: "" on_click: || ui.themap.nav_center_origin("1") }
        }
      }
    }
  }

  // ===================== DRIVE (turn-by-turn) =====================
  if scr == "drive" {
    View{ width: Fill height: Fill flow: Down padding: Inset{left: 10 top: 44 right: 10 bottom: 10}
      roadpill := RoundedView{ width: Fill height: Fit flow: Right align: Align{y: 0.5} draw_bg.color: #0d3a2f draw_bg.border_radius: 13 padding: Inset{left: 9 top: 7 right: 12 bottom: 7} spacing: 9
        CircleView{ width: 30 height: 30 flow: Right align: Align{x: 0.5 y: 0.5} draw_bg.color: #1a73e8 arw := Label{ text: "↑" draw_text.color: #ffffff draw_text.text_style.font_size: 14 } }
        View{ width: Fill height: Fit flow: Down spacing: 2
          instr := Label{ width: Fill text: "Starting…" draw_text.color: #ffffff draw_text.text_style.font_size: 11 }
          ndist := Label{ width: Fill text: "" draw_text.color: #a9d6c8 draw_text.text_style.font_size: 9 }
        }
      }
      roadlbl := Label{ text: "" draw_text.color: #00000000 draw_text.text_style.font_size: 1 }
      // 2D/3D toggle floats on the LEFT side, above the bottom bar (map control,
      // not stacked under End)
      View{ width: Fill height: Fill flow: Down align: Align{x: 0.0 y: 1.0}
        View{ width: 60 height: 48 flow: Overlay margin: Inset{left: 4 bottom: 8}
          if vw == "2d" {
            RoundedView{ width: Fill height: Fill flow: Right align: Align{x: 0.5 y: 0.5} draw_bg.color: #172232 draw_bg.border_radius: 14 Label{ text: "3D" draw_text.color: #eaf0f7 draw_text.text_style.font_size: 15 } }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "view", value: "3d"}) }
          }
          if vw != "2d" {
            RoundedView{ width: Fill height: Fill flow: Right align: Align{x: 0.5 y: 0.5} draw_bg.color: #172232 draw_bg.border_radius: 14 Label{ text: "2D" draw_text.color: #eaf0f7 draw_text.text_style.font_size: 15 } }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "view", value: "2d"}) }
          }
        }
      }
      // bottom sheet. The handle + time/distance form one big swipe target: a Button
      // sits ON TOP of them (transparent) so it (a) catches the swipe to toggle and
      // (b) occludes the map so the swipe never leaks through to pan it. End is a
      // separate row BELOW the swipe target, so it stays tappable when expanded.
      // Swipe toggles endrow's visibility DIRECTLY (ui.endrow.set_visible) — no
      // agent.notify/eval_body, so the map is never torn down / re-rendered.
      View{ width: Fill height: Fit flow: Down align: Align{x: 0.5 y: 1.0} padding: Inset{left: 4 right: 82}
        RoundedView{ width: Fill height: Fit flow: Down draw_bg.color: #0c1420 draw_bg.border_radius: 16 padding: Inset{left: 18 top: 7 right: 18 bottom: 12} spacing: 4
          View{ width: Fill height: Fit flow: Overlay
            View{ width: Fill height: Fit flow: Down spacing: 4
              View{ width: Fill height: 6 flow: Right align: Align{x: 0.5 y: 0.5}
                RoundedView{ width: 44 height: 5 draw_bg.color: #3a4658 draw_bg.border_radius: 3 }
              }
              remmin := Label{ text: "" draw_text.color: #4ade80 draw_text.text_style.font_size: 22 }
              remrest := Label{ text: "" draw_text.color: #9fb0c4 draw_text.text_style.font_size: 13 }
            }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" swipe: true on_swipe_up: || ui.endrow.set_visible(true) on_swipe_down: || ui.endrow.set_visible(false) }
          }
          endrow := View{ width: Fill height: 44 flow: Overlay margin: Inset{top: 6} visible: false
            RoundedView{ width: Fill height: Fill flow: Right align: Align{x: 0.5 y: 0.5} draw_bg.color: #7a1f1f draw_bg.border_radius: 12 Label{ text: "× End" draw_text.color: #ffd9d9 draw_text.text_style.font_size: 15 } }
            Button{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.border_size: 0.0 text: "" on_click: || agent.notify("set", {key: "go", value: "0"}) }
          }
        }
      }
    }
  }

}
```
