# Convert app — CURRENCY / UNIT CONVERTER (assemble from widgets; no exemplar)

A dark **converter card**: an amount keypad, a live converted result, a swap
button — and, for currency, TAPPABLE currency selectors (`USD ▾`) that open
an inline picker of common currencies. Two flavors — pick ONE from the
request:

- **CURRENCY** (the default): "convert 100 usd to eur", "usd to rmb",
  "currency converter", "汇率" → the LIVE rate via `sys.fx`/`sys.fxnum`.
  Bare "converter" → USD → EUR. A named amount seeds the keypad.
- **UNITS**: "km to miles", "kg to lbs", "多少英里" → the SAME card with a
  fixed factor from the SANCTIONED table below — nothing else, and NO picker.

Build from `widgets/design-system.md`, `widgets/containers.md`,
`widgets/interaction.md` (§ fn tick(), § Splash-local state, § style
templates) and `widgets/sys-helpers.md` (`sys.fx` / `sys.fxnum` /
`sys.fmtnum`). Keep the block under 11,000 bytes.

## Sanctioned unit factors — the ONLY ones you may write

`mi = km × 0.621371` · `ft = m × 3.28084` · `in = cm × 0.393701` ·
`lb = kg × 2.20462` · `oz = g × 0.035274` · `gal = L × 0.264172` ·
`mph = km/h × 0.621371` · `°F = °C × 1.8 + 32` · `°C = (°F − 32) / 1.8`.
(Reversed direction = divide, except °C/°F which use both formulas.) ANY
other unit pair, or ANY literal currency rate, is a FAILED generation.

## CURRENCY — state + fns (full-script body)

`// name: convert-app` first; FIRST executable line:

```
let conv = { amt: 1 frac: 0 fresh: 1 from: "USD" to: "EUR" picking: 0 }
```

The pair lives in STATE STRINGS (`conv.from`/`conv.to`) — every
`sys.fx`/`sys.fxnum` call takes them as arguments, never literals (the ONLY
literal pair is the request's starting pair in the seed + the two prefetch
bindings below). Seed `amt` from the request when it names one — "convert
100 usd" → `amt: 100`. **NEVER seed `amt: 0`**: the card must open showing a
real conversion. `fresh: 1` makes the first digit tap replace the seed.

Exactly these fns IN THIS ORDER (a fn may only call a fn defined ABOVE it —
forward references fail SILENTLY; `fn tick()` calls no fns, so it goes
FIRST, immediately after the state line — in a card with this many fns the
engine fails to find a late-defined tick):

```
fn tick() {
    let r = sys.fxnum(conv.from, conv.to)
    if r >= 0 {
        ui.result.set_text(sys.fmtnum(conv.amt * r, 2) + " " + conv.to)
        ui.rateline.set_text("1 " + conv.from + " = " + sys.fmtnum(r, 4) + " " + conv.to)
    }
}
fn update_header() {
    ui.from_btn.set_text(conv.from)
    ui.to_btn.set_text(conv.to)
}
fn recalc() {
    let r = sys.fxnum(conv.from, conv.to)
    ui.amount.set_text(sys.fmtnum(conv.amt, 6) + " " + conv.from)
    if r >= 0 {
        ui.result.set_text(sys.fmtnum(conv.amt * r, 2) + " " + conv.to)
        ui.rateline.set_text("1 " + conv.from + " = " + sys.fmtnum(r, 4) + " " + conv.to)
    } else { ui.result.set_text("…") ui.rateline.set_text("…") }
}
fn digit(d) { /* fresh/frac handling exactly as in the calc pattern */ recalc() }
fn dot() { /* calc pattern */ }
fn clearall() { conv.amt = 0 conv.frac = 0 conv.fresh = 1 recalc() }
fn pick_from() { conv.picking = 1 ui.scrim.set_visible(true) ui.pickpanel.set_visible(true) }
fn pick_to() { conv.picking = 2 ui.scrim.set_visible(true) ui.pickpanel.set_visible(true) }
fn close_picker() { conv.picking = 0 ui.scrim.set_visible(false) ui.pickpanel.set_visible(false) }
fn choose(c) {
    if conv.picking == 1 { conv.from = c }
    if conv.picking == 2 { conv.to = c }
    conv.picking = 0
    ui.scrim.set_visible(false)
    ui.pickpanel.set_visible(false)
    update_header()
    recalc()
}
fn swap() { let t = conv.from conv.from = conv.to conv.to = t update_header() recalc() }
```

⚠️ In BOTH `recalc()` and `tick()` the rate is read ONCE into a local
(`let r = sys.fxnum(…)`) and every use — the `>= 0` gate, the math, the
rateline — goes through `r`. NEVER put a `sys.*` call directly inside an
`if` condition in `tick()` (a native call in a tick-path condition kills the
whole tick silently; as a set_text ARGUMENT it is fine — the clock card).

- `fn tick()` is SELF-CONTAINED — it must NOT call `recalc()` or any of your
  fns (interaction.md § fn tick()), and it must never be the FIRST caller of
  a new pair's fetch. That is safe here: every pair change goes through
  `choose()`/`swap()`, whose `recalc()` (click context — allowed) issues the
  new pair's fetch; tick then re-reads the cache and fills the result when
  the rate lands (~1 s).
- **The STARTING pair's fetches are issued by BODY bindings**: the
  `rateline` label's INITIAL text binds the live forward rate
  (`text: "1 USD = " + sys.fx("USD", "EUR") + " EUR"`), and a zero-height
  prefetch label right after it binds the reverse
  (`Label{ width: 0 height: 0 text: sys.fx("EUR", "USD") }`) — starting pair
  substituted in both.
- Rates come from `sys.fxnum`/`sys.fx` ONLY — never a literal number.

## CURRENCY — layout: an OVERLAY root with a floating pop-up picker

The picker is a POP-UP, not an inline row: it FLOATS OVER the card (never
pushing content down), opens from a selector tap, and closes on selection or
an outside tap. This is the library's overlay idiom (the weather card's
Overlay root; nav's `visible: false` + `set_visible`).

Root `SolidView{ width: Fill height: 900 flow: Overlay
draw_bg.color: #0d0d10 new_batch: true }` with EXACTLY THREE children, in
this order (later Overlay children draw ON TOP):

**(1) THE MAIN COLUMN** — `View{ width: Fill height: Fill flow: Down
padding: Inset{left: 16 top: 48 right: 16 bottom: 20} spacing: 12 }`
(the padding lives HERE, not on the root, so the scrim can cover the whole
card). Accent `#30d158` on the result and eyebrow. Top to bottom:

1. **Masthead** — eyebrow `CONVERTER` (11, `#30d158`), then the SELECTOR ROW
   `View{ width: Fill height: 52 flow: Right spacing: 10
   align: Align{y: 0.5} }`:
   - `from_btn := Button{ text: "USD" }` (`#ffffff14` bg, white 20,
     radius 12, `width: Fit height: 44`, padding l/r 18,
     `on_click: || pick_from()`)
   - `Label{ text: "→" }` (20, `#ffffff80`)
   - `to_btn := Button{ text: "EUR" }` (same style,
     `on_click: || pick_to()`).
   Selector text is the BARE code — no `▾`/`▼` glyph (they render as tofu
   on the default font chain). Under the row a hint
   `Label{ text: "TAP A CURRENCY TO CHANGE IT" }` (9, `#ffffff55`).
2. **AMOUNT** — `RoundedView` (`#ffffff0d`, radius 16, Fit + padding):
   `amount := Label{ text: "1 USD" }` (30, white; initial text MUST match
   the seed amount + starting FROM code).
3. **RESULT** — `RoundedView` (`#30d15814`, radius 16, Fit + padding):
   `result := Label{ text: "…" }` (font_size 44, `#30d158`), under it the
   live-rate `rateline :=` binding and the zero-height reverse prefetch from
   the fns section above.
4. **SWAP** — `Button{ text: "⇄  SWAP" }` (`#ffffff14` bg, white 15,
   `width: Fill height: 52`, radius 14, `on_click: || swap()`).
5. **Keypad** — FOUR `View{ width: Fill height: 72 flow: Right spacing: 12 }`
   rows from a `Key` style template (`#333338` bg, white 22, `width: Fill
   height: 72`, radius 14): `7 8 9` / `4 5 6` / `1 2 3` / `AC 0 .` — digits
   `on_click: || digit(7)` …, `clearall()`, `dot()`.

**(2) THE SCRIM** — `scrim := Button{ width: Fill height: 900 text: ""
visible: false draw_bg.color: #00000099 draw_bg.color_hover: #00000099
draw_bg.color_focus: #00000099 draw_bg.color_down: #00000099
draw_bg.border_size: 0.0 grab_key_focus: false
on_click: || close_picker() }` — the dimming layer; a tap ANYWHERE outside
the pop-up dismisses it. EXPLICIT height 900 (`height: Fill` inside an
Overlay is the classic trap — always explicit).

**(3) THE POP-UP PANEL** — `pickpanel := RoundedView{ width: Fill
height: Fit visible: false flow: Down spacing: 8
margin: Inset{left: 24 top: 150 right: 24}
padding: Inset{left: 12 top: 12 right: 12 bottom: 12}
draw_bg.color: #1f1f26 draw_bg.border_radius: 16.0 }` — floats just below
the selector row, over the amount/result. Inside: a tiny caption
`Label{ text: "SELECT CURRENCY" }` (9, `#ffffff55`) and TWO chip rows
(`View{ width: Fill height: 44 flow: Right spacing: 8 }`), EIGHT currencies
total: `USD EUR CNY JPY` / `GBP KRW INR CAD`. **Each chip is ONE plain
Button, fully written out** —
`Button{ width: Fill height: 44 text: "USD" draw_bg.color: #333338
draw_bg.border_radius: 10.0 draw_text.color: #ffffff
draw_text.text_style.font_size: 14 on_click: || choose("USD") }`
(expression form, one per chip; NO style template for the chips). Do NOT
build a chip as a View + Label + transparent overlay Button — a Button
nested inside a style-template instantiation does not receive taps.

## UNITS flavor

No picker, no selector buttons — a static title (`Kilometers → Miles`, 26
white) replaces the selector row, and a `dir` flag replaces the pair state:
`let conv = { amt: 1 frac: 0 fresh: 1 dir: 0 }`. `fn recalc()` and
`fn tick()` are TWO mirror branches on `conv.dir` using the sanctioned
factor — `conv.amt * 0.621371` one way, `conv.amt / 0.621371` the other
(°C/°F use the two formulas; no `>= 0` gate — factors are never loading).
`fn swap()` flips `dir` and updates a title label. Same amount/result/swap/
keypad layout, same fn-ordering and self-contained-tick rules — but a plain
`flow: Down` root (no Overlay, no scrim/pickpanel children).

## Failure conditions

Missing `// name: convert-app`; no `let conv =` opening line; an `amt: 0`
seed; a literal currency rate anywhere; currency flavor with literal pair
strings in `recalc()`/`tick()` fx calls (state args only), or missing
`fn choose(`/`pick_from`/`pick_to`/`close_picker` or the hidden `scrim :=`/
`pickpanel :=` Overlay children, or a picker that pushes the column down
instead of floating over it; a unit
factor not in the sanctioned table; missing `fn tick()` or (currency)
`sys.fxnum`; converted values not formatted by `sys.fmtnum`; block-form
closures `||{`; any `agent.notify` or `{{state.*}}`; block over
11,000 bytes.
