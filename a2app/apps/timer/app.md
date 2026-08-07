# Timer app — TIMER / STOPWATCH (assemble from widgets; no exemplar)

A dark, iOS-Timer-style **countdown timer** OR **stopwatch** card — pick ONE
mode from the request and generate only it:

- "timer", "countdown", "5 minute timer", "计时器", "倒计时" → **TIMER**
  (count DOWN from a seed; a named duration seeds it — "5 minute timer" →
  300 — else default 300). The TIMER also has a TAP-TO-SET pop-up: tapping
  the big time opens a floating duration grid.
- "stopwatch", "秒表" → **STOPWATCH** (count UP from 0; no pop-up, no
  presets, plain `flow: Down` root).

Build from `widgets/design-system.md`, `widgets/containers.md` and
`widgets/interaction.md` (§ fn tick(), § Splash-local state). Everything is
Splash-local — NO `agent.notify`, NO `{{state.*}}`, NO sys fetches. Format
every time with `sys.fmtdur` (the script engine has no floor/`%`). Keep the
block under 10,000 bytes.

## State + fns (full-script body)

`// name: timer-app` first; the FIRST executable line after it is the state
object (no comment before it):

- TIMER: `let tmr = { left: 300 total: 300 running: 0 }` (seed per request)
- STOPWATCH: `let tmr = { elapsed: 0 running: 0 }`

Flags are NUMBERS (0/1) — unary `!` does not exist; flip with if/else
(interaction.md § fn tick()). Expression closures call exactly ONE fn.

TIMER fns — exactly these seven, in THIS order. `fn tick()` is
SELF-CONTAINED (calling one of your other fns from inside tick() fails
SILENTLY) and is defined FIRST, right after the state line; every other fn
also calls no fns (everything inline):

```
fn tick() {
    if tmr.running == 1 {
        if tmr.left > 0 { tmr.left = tmr.left - 1 ui.big.set_text(sys.fmtdur(tmr.left)) }
        if tmr.left == 0 { tmr.running = 0 ui.go.set_text("START") ui.status.set_text("DONE ✓") }
    }
}
fn open_picker() { ui.scrim.set_visible(true) ui.setpanel.set_visible(true) }
fn close_picker() { ui.scrim.set_visible(false) ui.setpanel.set_visible(false) }
fn setdur(n) {
    tmr.running = 0
    tmr.total = n
    tmr.left = n
    ui.title.set_text(sys.fmtdur(n) + " Timer")
    ui.big.set_text(sys.fmtdur(n))
    ui.total.set_text("of " + sys.fmtdur(n))
    ui.go.set_text("START")
    ui.status.set_text("")
    ui.scrim.set_visible(false)
    ui.setpanel.set_visible(false)
}
fn toggle() {
    if tmr.running == 1 { tmr.running = 0 ui.go.set_text("START") }
    else { if tmr.left > 0 { tmr.running = 1 ui.go.set_text("PAUSE") ui.status.set_text("") } }
}
fn reset() { tmr.running = 0 tmr.left = tmr.total ui.big.set_text(sys.fmtdur(tmr.left)) ui.go.set_text("START") ui.status.set_text("") }
fn add(n) { tmr.left = tmr.left + n tmr.total = tmr.total + n ui.title.set_text(sys.fmtdur(tmr.total) + " Timer") ui.big.set_text(sys.fmtdur(tmr.left)) ui.total.set_text("of " + sys.fmtdur(tmr.total)) }
```

STOPWATCH: only `tick`/`toggle`/`reset`, counting UP (`tick` inlines
`tmr.elapsed = tmr.elapsed + 1` when running), `fn reset()` back to `00:00`,
no `add`, no picker fns, no `status`/DONE state.

## TIMER layout: an OVERLAY root with the tap-to-set pop-up

Root `SolidView{ width: Fill height: 640 flow: Overlay
draw_bg.color: #0d0d10 new_batch: true }` with EXACTLY THREE children in
this order (later Overlay children draw ON TOP):

**(1) THE MAIN COLUMN** — `View{ width: Fill height: Fill flow: Down
padding: Inset{left: 16 top: 56 right: 16 bottom: 24} spacing: 16 }`
(padding HERE, not on the root, so the scrim covers the whole card).
Accent `#ff9f0a` on the eyebrow, GO button and status. Top to bottom:

1. **Masthead** — eyebrow `TIMER` (11, `#ff9f0a`), then the LIVE title
   `title := Label{ text: "5 Minutes" }` (30, white; initial text = the
   request's seed, e.g. `5 Minutes`). The title is a NAMED widget because
   `setdur`/`add` rewrite it (`10:00 Timer`) — a stale title that still says
   the seeded duration after the user changes it is a bug.
2. **BIG TIME (tappable)** — `RoundedView{ width: Fill height: 210
   flow: Overlay new_batch: true draw_bg.color: #ffffff0d
   draw_bg.border_radius: 20.0 }` holding
   - a centered `View{ width: Fill height: 210 flow: Down spacing: 8
     align: Align{x: 0.5 y: 0.5} }` with `big := Label{ text: sys.fmtdur(300) }`
     (font_size 72, white; initial text = the seed),
     `total := Label{ text: "of " + sys.fmtdur(300) }` (13, `#ffffff99`),
     `status := Label{ text: "" }` (15, `#ff9f0a`), and a hint
     `Label{ text: "TAP TIME TO SET" }` (9, `#ffffff55`);
   - then the tap catcher: `Button{ width: Fill height: 210 text: ""
     draw_bg.color: #00000000 draw_bg.color_hover: #00000000
     draw_bg.color_focus: #00000000 draw_bg.color_down: #00000000
     draw_bg.border_size: 0.0 grab_key_focus: false
     on_click: || open_picker() }` — a plain Button DIRECTLY in this
     fixed-height Overlay (NEVER inside a style template — templated Buttons
     don't receive taps; explicit height 210, never `Fill`).
3. **CONTROLS** — `View{ flow: Right spacing: 12 height: 64 }`:
   `go := Button{ text: "START" }` (accent-tinted `#ff9f0a22` bg, orange
   text, radius 16, `width: Fill height: 64`, `on_click: || toggle()`) and
   `Button{ text: "RESET" }` (`#ffffff14` bg, white text,
   `on_click: || reset()`).
4. **ADJUST CHIPS** — `View{ flow: Right spacing: 10 height: 44 }` of three
   `Button`s `+1 MIN` / `+5 MIN` / `+10 MIN` (`#ffffff0d` bg, 13pt,
   `on_click: || add(60)` / `add(300)` / `add(600)`).

**(2) THE SCRIM** — `scrim := Button{ width: Fill height: 640 text: ""
visible: false draw_bg.color: #00000099 draw_bg.color_hover: #00000099
draw_bg.color_focus: #00000099 draw_bg.color_down: #00000099
draw_bg.border_size: 0.0 grab_key_focus: false
on_click: || close_picker() }` — dims the card; a tap outside the pop-up
dismisses it. EXPLICIT height 640, never `Fill`.

**(3) THE POP-UP PANEL** — `setpanel := RoundedView{ width: Fill
height: Fit visible: false flow: Down spacing: 8
margin: Inset{left: 24 top: 120 right: 24}
padding: Inset{left: 12 top: 12 right: 12 bottom: 12}
draw_bg.color: #1f1f26 draw_bg.border_radius: 16.0 }` — floats over the big
time. Inside: a caption `Label{ text: "SET DURATION (MIN)" }` (9,
`#ffffff55`) and THREE chip rows (`View{ width: Fill height: 44 flow: Right
spacing: 8 }`), TWELVE durations: `0:30 1 2 3` / `5 10 15 20` /
`25 30 45 60`. Each chip is ONE plain fully-written Button —
`Button{ width: Fill height: 44 text: "10" draw_bg.color: #333338
draw_bg.border_radius: 10.0 draw_text.color: #ffffff
draw_text.text_style.font_size: 14 on_click: || setdur(600) }` — with the
matching seconds: 30, 60, 120, 180 / 300, 600, 900, 1200 / 1500, 1800,
2700, 3600. NO style template for the chips.

## STOPWATCH layout

Plain `flow: Down` root (no Overlay, no scrim/setpanel): masthead, big-time
card (NOT tappable, no hint), START/PAUSE + RESET. Nothing else.

## Failure conditions

Missing `// name: timer-app`; no `let tmr =` opening state line; no
`fn tick(`; tick() (or any fn) calling another of your fns; TIMER missing
the tap catcher, `scrim :=`/`setpanel :=` Overlay children or `fn setdur(`;
a pop-up that pushes the column down instead of floating; a title not named `title :=` or not rewritten by `setdur`/`add`; any time string
not built by `sys.fmtdur`; boolean flags flipped with `!`; block-form
closures `||{`; any `agent.notify`, `{{state.*}}` or network sys call; both
modes generated at once; block over 10,000 bytes.
