# Clock app — WORLD CLOCK (assemble from widgets; no exemplar)

A dark, iOS-Clock-style **world clock card**: one hero city with a live
seconds display, plus four more cities in rows. Use it for any time-of-day
request ("what time is it in tokyo", "world clock", "time in london and NY",
"现在几点").

Build from `widgets/design-system.md`, `widgets/containers.md`,
`widgets/interaction.md` (§ fn tick(), § Splash-local state) and
`widgets/sys-helpers.md` (`sys.citytime` / `sys.citytimenum` /
`sys.geocodenum`). Keep the block under 9,000 bytes.

## Cities

- The HERO is the city the request names (first one, if several). The ROWS are
  the other named cities, padded from this default list (skip any already
  shown, keep 4 rows): New York, London, Tokyo, Shanghai.
- A bare "world clock" / "几点" with no city: hero = San Francisco, rows = the
  four defaults.

## State + tick (full-script body)

- `// name: clock-app` is the first line; then `fn tick()` and the root view.
  No `let` state object is needed — the clock has no user state.
- `fn tick()` re-sets EVERY time label once per second
  (interaction.md § fn tick()):

```
fn tick() {
    ui.hero_time.set_text(sys.citytime(sys.geocodenum("Tokyo", "lat"), sys.geocodenum("Tokyo", "lon"), "hms"))
    ui.hero_sub.set_text(sys.citytime(...,"day") + " · " + sys.citytime(...,"date") + " · " + sys.citytime(...,"offset"))
    ui.row1_time.set_text(sys.citytime(sys.geocodenum("London", "lat"), sys.geocodenum("London", "lon"), "hm"))
    /* … row2..row4 time + every rowN_sub the same way … */
}
```

- EVERY coordinate is `sys.geocodenum("<city>", "lat"/"lon")` — never typed
  digits. Every time string is `sys.citytime(...)` — NEVER computed by you
  (you do not know DST anywhere). Labels show `—` for the first second.
- Also bind the SAME expressions as the labels' initial `text:` so the card
  isn't empty before the first tick.

## Layout, top to bottom

Dark monitor theme: root `SolidView{ width: Fill height: 900 flow: Down
draw_bg.color: #0d0d10 padding: Inset{left: 16 top: 56 right: 16 bottom: 24}
spacing: 14 new_batch: true }`. White primary text, secondary `#ffffff99`,
hairlines `SolidView{ height: 1 draw_bg.color: #ffffff14 }`. ONE accent for
all eyebrows and offsets: `#64d2ff`.

1. **Masthead** — eyebrow `Label{ text: "WORLD CLOCK" }` (11, `#64d2ff`),
   then `Label{ text: "<Hero City>" }` (30, white).
2. **HERO** — `RoundedView{ width: Fill height: Fit draw_bg.color: #ffffff0d
   draw_bg.border_radius: 20 padding: Inset{left: 16 top: 18 right: 16
   bottom: 18} flow: Down spacing: 6 }`:
   - `hero_time := Label{ text: sys.citytime(..., "hms") }` (font_size 56, white)
   - `hero_sub := Label` (13, `#ffffff99`) — `day · date · offset`.
3. **Section label** — `Label{ text: "AROUND THE WORLD" }` (11, `#ffffff66`).
4. **FOUR city rows** — each `View{ width: Fill height: 64 flow: Right
   spacing: 12 align: Align{y: 0.5} }`:
   - a `flow: Down` column (`width: Fill`): city name (16, white), then
     `rowN_sub := Label` (12, `#ffffff80`) — `day · offset`
   - `rowN_time := Label` (26, white, right side).
   Hairline separators between rows (not after the last).

## Failure conditions

Missing `// name: clock-app`; no `fn tick(`; any typed lat/lon digits or any
time/day/offset string not from `sys.citytime`; fewer than 4 city rows; a
`{{state.*}}` slot or `agent.notify` anywhere (this card has no remote
state); more than ONE ```runsplash block; block over 9,000 bytes.
