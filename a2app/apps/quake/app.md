# quake — the LATEST-EARTHQUAKES app (assemble from widgets; no exemplar)

A dark, seismic-monitor style **latest-earthquakes card**: the USGS live feed
(all M2.5+ events, last 24 h), newest first. Use it for any earthquake /
seismic request ("earthquakes", "recent quakes", "any earthquakes today?",
"地震").

YOU generate this card by ASSEMBLING the widget patterns — there is no
exemplar to copy. Build from:
- `widgets/design-system.md` — the look (type scale, spacing)
- `widgets/containers.md` — the containers
- `widgets/sys-helpers.md` — `sys.quakes` / `sys.quakesnum`
- `widgets/map-pane.md` — the zoomable map pane (mosaic + pin + zoom chips)

## Name line + state model (mandatory)

The FIRST line inside the fence is exactly:

```
// name: quake-app
```

The map zoom lives in the `{{state.zoom|8}}` slot (default level 8) per
`widgets/map-pane.md` — no `let` state and no per-zoom branches are needed;
begin directly with the root `SolidView{ ... }`.

## Visual language

Dark monitor theme: near-black root `SolidView{ draw_bg.color: #0d0d10 }`,
white primary text, secondary `#ffffff99`, tertiary `#ffffff66`, hairline
separators `SolidView{ height: 1 draw_bg.color: #ffffff14 }`. ONE accent for
all magnitude numbers: `#ff9f0a`. Root: `width: Fill height: 1450 flow: Down
padding: Inset{left: 16 top: 56 right: 16 bottom: 24} spacing: 14`. Body text
uses the default font; no custom fonts.

## Layout, top to bottom

1. **Masthead** — eyebrow `Label{ text: "USGS · LAST 24 H · M2.5+" }` (11px,
   `#ff9f0a`), title `Label{ text: "Earthquakes" }` (font_size 30, white),
   then a count line: `sys.quakes(0, "count") + " events worldwide"` (13px,
   `#ffffff99`).

2. **BLOCK: LATEST** — the most recent event (index 0) as a hero card:
   `RoundedView{ width: Fill height: Fit draw_bg.color: #ffffff0d
   draw_bg.border_radius: 20 padding: Inset{left: 16 top: 14 right: 16
   bottom: 14} flow: Right spacing: 14 align: Align{y: 0.5} new_batch: true }`
   containing:
   - the magnitude alone, huge: `Label{ text: sys.quakes(0, "mag") }`
     (font_size 54, `#ff9f0a`, width 120)
   - a `flow: Down` column (`width: Fill`): the place (16px, white,
     `width: Fill`), then the age + depth line:
     `sys.quakes(0, "time") + " · depth " + sys.quakes(0, "depth")`
     (13px, `#ffffff99`).

3. **BLOCK: EPICENTER-MAP** — the ZOOMABLE map of the latest epicenter,
   built EXACTLY per `widgets/map-pane.md` with the epicenter as the anchor
   (`LATN` = `sys.quakesnum(0, "lat")`, `LONN` = `sys.quakesnum(0, "lon")`,
   zoom slot `{{state.zoom|8}}` in ALL SIX sys calls: the four `sys.maptile`
   quadrants plus the two `sys.mappin(..., 744)` margin offsets).
   - Section label `Label{ text: "EPICENTER REGION" }` (11px, `#ffffff66`)
     OUTSIDE the pane.
   - Then the pane copied from `widgets/map-pane.md` verbatim with the quake
     anchors substituted: the 744 true-centered mosaic, the FIXED dead-center
     📍 pin, and the bottom overlay control bar — the live
     `"z" + "{{state.zoom|8}}"` indicator plus the THREE stepper buttons
     `−` / `8` / `+` (dec / set-default / inc on `key: "zoom"`, with
     `step: "1"`, `min: "5"`, `max: "14"`, `default: "8"`).
   - Immediately below the pane, the REQUIRED tile attribution:
     `Label{ text: "© OpenStreetMap contributors © CARTO" }` (11px,
     `#ffffff66`). The keyless CARTO tiles mandate this credit.

4. **Section label** — `Label{ text: "EARLIER TODAY" }` (11px, `#ffffff66`).

5. **BLOCK: FEED** — SIX rows, indexes 1..6, each
   `View{ width: Fill height: 64 flow: Right spacing: 12
   align: Align{y: 0.5} }`:
   - `Label{ width: 56 text: sys.quakes(i, "mag") }` (font_size 20, `#ff9f0a`)
   - a `flow: Down` column (`width: Fill`): the place (14px, white,
     `width: Fill`), then `sys.quakes(i, "time") + " · " +
     sys.quakes(i, "depth")` (12px, `#ffffff80`)
   with a hairline separator `SolidView{ height: 1 draw_bg.color: #ffffff14 }`
   between rows (not after the last).

## LIVE DATA — MANDATORY

- EVERY displayed value is a `sys.quakes(INDEX, "field")` call — magnitude,
  place, time, depth, count. NEVER invent or hardcode a quake; a made-up
  magnitude destroys trust in the whole card.
- The map pane's coordinates come from `sys.quakesnum(0, "lat")` /
  `sys.quakesnum(0, "lon")` chained into every `sys.maptile` quadrant and both
  `sys.mappin(..., 744)` margin offsets — never literal numbers.
- Expect ≥ 20 `sys.quakes(` calls across the masthead count, the LATEST block
  and the six feed rows.
- Values render as `—` while the feed loads; the card re-evaluates when data
  lands. Do not add loading spinners.

## Failure conditions

Any of these is a FAILED generation:
- missing the `// name: quake-app` first line;
- any hardcoded magnitude, place name, depth, age, or epicenter coordinate;
- fewer than 6 feed rows, or feed rows not bound per-index (1..6);
- no EPICENTER-MAP block, or its 4 `sys.maptile` quadrants / 2
  `sys.mappin(..., 744)` margin offsets not fed by `sys.quakesnum` and the
  `{{state.zoom|8}}` slot;
- missing any of the THREE stepper buttons (`−`/`8`/`+` firing
  dec/set/inc on `key: "zoom"`), or a row of preset-level chips instead,
  or any sys call in the pane missing the `{{state.zoom|8}}` slot;
- the 📍 pin positioned with computed offsets instead of the fixed
  dead-center overlay (align 0.5/0.5, bottom-margin 22);
- any overlay in the map pane other than the mosaic, the pin View and the
  bottom control bar;
- missing the `© OpenStreetMap contributors © CARTO` attribution line below
  the map pane;
- more than ONE ```runsplash block, or prose outside the fence;
- block over 12,000 bytes.
