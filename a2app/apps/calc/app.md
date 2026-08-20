# Calc app — CALCULATOR (assemble from widgets; no exemplar)

A dark, iOS-style **calculator card**: big right-aligned display over a 5-row
key grid. Immediate-execution (classic phone calculator), entirely
Splash-local. Use it for "calculator", "计算器", or any bare arithmetic ask.

Build from `widgets/design-system.md`, `widgets/containers.md` and
`widgets/interaction.md` (§ Splash-local state, § style templates). NO
`agent.notify`, NO `{{state.*}}`, NO network sys calls. The ONLY sys helper
used is `sys.fmtnum` for the display (the script engine cannot round). Keep
the block under 10,000 bytes.

## State + fns (full-script body)

`// name: calc-app` first; the FIRST executable line is the state object:

```
let calc = { acc: 0 entry: 0 op: 0 frac: 0 fresh: 1 }
```

`op` is a NUMBER: 0 none · 1 add · 2 subtract · 3 multiply · 4 divide.
Exactly these fns IN THIS ORDER (a fn may only call a fn defined ABOVE it —
forward references fail SILENTLY; expression closures call ONE fn each):

```
fn show(x) { ui.display.set_text(sys.fmtnum(x, 8)) }
fn digit(d) {
    if calc.fresh == 1 { calc.entry = 0 calc.frac = 0 calc.fresh = 0 }
    if calc.frac > 0 { calc.entry = calc.entry + d * calc.frac calc.frac = calc.frac / 10 }
    else { calc.entry = calc.entry * 10 + d }
    show(calc.entry)
}
fn dot() { if calc.fresh == 1 { calc.entry = 0 calc.fresh = 0 } if calc.frac == 0 { calc.frac = 0.1 } }
fn apply() {
    if calc.op == 0 { calc.acc = calc.entry }
    if calc.op == 1 { calc.acc = calc.acc + calc.entry }
    if calc.op == 2 { calc.acc = calc.acc - calc.entry }
    if calc.op == 3 { calc.acc = calc.acc * calc.entry }
    if calc.op == 4 { if calc.entry != 0 { calc.acc = calc.acc / calc.entry } else { calc.acc = 0 ui.display.set_text("Error") } }
}
fn setop(o) { apply() calc.op = o calc.entry = calc.acc calc.fresh = 1 show(calc.acc) }
fn equals() { apply() calc.op = 0 calc.entry = calc.acc calc.fresh = 1 show(calc.acc) }
fn clearall() { calc.acc = 0 calc.entry = 0 calc.op = 0 calc.frac = 0 calc.fresh = 1 show(0) }
fn negate() { calc.entry = 0 - calc.entry show(calc.entry) }
fn pct() { calc.entry = calc.entry / 100 show(calc.entry) }
```

## Layout

Root `SolidView{ width: Fill height: 780 flow: Down draw_bg.color: #0d0d10
padding: Inset{left: 16 top: 48 right: 16 bottom: 20} spacing: 12
new_batch: true }`.

1. **Display** — `View{ width: Fill height: 120 align: Align{x: 1.0 y: 1.0} }`
   holding `display := Label{ text: "0" }` (font_size 56, white).
2. **Key grid** — FIVE `View{ width: Fill height: 78 flow: Right
   spacing: 12 }` rows of `Button`s from THREE style templates you define
   (§ style templates): `KeyNum` (`#333338` bg, white 24pt, `width: 78
   height: 78`, radius 39), `KeyOp` (`#ff9f0a` bg, white 26pt) and `KeyFn`
   (`#a5a5ad` bg, black 20pt). Rows exactly:
   - `AC` `±` `%` `÷`  → `clearall()` `negate()` `pct()` `setop(4)`
   - `7` `8` `9` `×`   → `digit(7)` … `setop(3)`
   - `4` `5` `6` `−`   → `digit(4)` … `setop(2)`
   - `1` `2` `3` `+`   → `digit(1)` … `setop(1)`
   - `0` (double-wide, `width: 168`) `.` `=` → `digit(0)` `dot()` `equals()`

## Failure conditions

Missing `// name: calc-app`; no `let calc =` opening line; missing any of the
eleven fns; ops as strings instead of numbers; a display write that bypasses
`show()`/`sys.fmtnum`; block-form closures `||{`; any `agent.notify`,
`{{state.*}}` or network sys call; fewer than 19 keys; block over
10,000 bytes.
