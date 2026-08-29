# Scanning the node contract against what the renderer draws (2026-08-30)

Every field an L0 card can carry, checked against what the Makepad renderer
actually consumes, then the gaps fixed and the result measured on device.

## The scan

`Attrs` declares **88 fields**. Nineteen are read by the evaluator and written
by *nothing* — a card can set them and they vanish silently:

    icon_name  elevation  enabled  items  selected  hint  badge  supporting
    helper  indeterminate  group  step  value2  accent  markcolor  total
    tilt  rotation  illum

**Only eight are reachable from L0 at all** (present in the constructor catalog
or the kit): `elevation`, `selected`, `badge`, `helper`, `step`, `total`,
`tilt`, `illum`. The rest are Material form-control roles that L0 cards never
use, so dropping them costs nothing.

Two guesses worth recording as wrong, because both looked like beauty
capabilities and neither is:

- **`rotation` is map camera bearing**, not element rotation. It cannot rotate a
  card element for a Memphis or constructivist layout.
- **`accent` / `markcolor` are Material control inks** — a checkbox's tick, a
  switch's thumb.

So of the whole dropped set, `elevation` was the only one with visual value.

## Fixed: elevation

`Card` and `Chip` now lower to `RoundedShadowView` rather than `RoundedView`.
That prototype is a strict superset: identical fill, gradient, radius and border
uniforms, plus `shadow_color` / `shadow_radius` / `shadow_offset`. A card that
sets no elevation writes an explicitly transparent shadow, so nothing moves for
existing cards and all 148 tests including the four device goldens still pass.

**Delivery-gated on device:** the light baseline darkens the page by **44
luminance levels** just below the card edge (199 against a 243 background).

## Fixed: a scope error on every render

`l0_stroke` read `l0_hairline` 27 lines before that name was declared. `let`
evaluates at its own line, so the name was unresolved, and an unresolved name
coerces to 0 — a fully transparent stroke ink. The device log went from one
scope error per render to none.

## Not fixed, but bisected: the border stroke

The earlier note at the emission site guessed the emission was at fault. It is
not.

**`border_size` reaches the shader.** Rendering the light baseline at
`panel_border: 30` insets the white fill by 82 device px — exactly 30 logical px
at this phone's 2.75 scale — and the card's own rows visibly hang outside the
shrunken fill. So the uniform applies and `sdf.box` insets by it.

**The stroke never paints.** That 82px band renders as plain page background at
every ink tried: opaque red, and a low-alpha red chosen specifically to rule out
`u32 > i32::MAX` (`argb(255,255,0,0)` is 4294901760). `hex()` emits `#RRGGBBAA`
correctly, and the *same* instance mechanism works for `draw_bg.color` and for
the new `draw_bg.shadow_color`.

So the failure is isolated to `sdf.stroke` / `border_color` in the makepad
prototype — widget work, below anything octos-one controls.

## The UX measurement, which did not go where expected

Target was 9/10. Opus scoring renders on device:

| | score |
|---|---|
| baseline fixtures (light/dark/glass/photo) | 5, 5, 5, 6 — **mean 5.25** |
| live-generated cards | **4** |

Live generation scores *worse* than the fixtures, so this is not a
fixture-mismatch artifact.

**What the judge actually blames, in its own words, is not missing marks:**

- *"Empty mid-screen void"* — named for all four moods. Measured: **540px of
  dead vertical space, 23.1% of the screen**, the largest single band 164px
  sitting between the hero and the forecast card.
- *"icon glyphs overlap adjacent rows"* — the weather glyph's rain drops sit at
  `h * 0.68` inside a 96px box, which leaves them nearer the *next* row's cloud
  than their own. The shader does not overflow its box; the proportion is wrong.
- *"clipped, unresolved satellite card"*
- *"washed-out photo kills contrast"*

One structural cause is visible in the fixture: `Col(gap: 36)` carries a comment
saying it is *"the mockup's air — the half-screen of photograph"*. In the photo
mood that gap frames an image and scores 6; in the three flat moods the same
card leaves a literal void and scores 5. The composition does not adapt to the
mood, and the emitter deliberately cannot fix that — it *"translates spacing; it
does not choose it"* (`l0_widgets.rs:15`). Spacing is the kit's to own, and a
card's literal `gap:` bypasses the kit.

## What this changes about priorities

The queue in `PLAN-vocabulary-loop.md` put stroke first because 53 of 100
specimens need it. That remains true for *fidelity to a mockup*. But for
**absolute UX score**, the measured blockers are composition and rendering
defects, not missing marks — and no amount of stroke, texture or shadow work
addresses a screen that is 23% empty with glyphs reading against the wrong row.

Both of the remaining capability items — border stroke, and icon proportion —
now sit in the same place: the makepad widget layer, not octos-one.
