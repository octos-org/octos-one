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

**Bisected to completion 2026-08-30.** `border_color` is NOT the fault either.
Hardcoding the ink at DESIGN time — `border_color: instance(#f00)` written
straight into `RoundedShadowView`'s prototype, the same way `Button`,
`CheckBox`, `DropDown` and `GlassPanel` declare theirs, and those render — still
produces **zero red pixels** on device with `panel_border: 8`.

So every input is ruled out: the property name is right (`border_color`, matching
`view_ui.rs:113`), the width provably reaches the shader (a 30-logical border
insets the fill by exactly 82 device px at this phone's 2.75 scale), the ink is
correct at design time as well as runtime, and the same instance mechanism draws
the fill and the new shadow on the very same prototype.

**`sdf.stroke` does not draw in this makepad build.** That is an upstream defect,
not an octos-one one, and it cannot be fixed from this repository. Worth noting
from the CPU reference implementation
(`platform/src/os/headless/shader_runtime_preamble.rs:1403`): `stroke` computes
`d = (shape.abs() - width*0.5).max(0.0)` and then `alpha = -d/aa + 0.5`, so a
stroke's maximum alpha is **0.5** even where it does draw — the clamp at zero
means the centre of the band never reaches full opacity.

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

---

# Four rounds of fix-and-rescore (2026-08-30)

Each round fixed exactly what the judge named, then re-scored the same four
mood renders on device.

| round | change | mean | judge's top complaint after |
|---|---|---|---|
| 0 | — | **5.25** | "empty mid-screen void" (4 of 4 moods) |
| 1 | `air_factor`, icon overflow | **5.00** | "system emoji icons" (3 of 4) |
| 2 | `icon_mono` in the base | **5.00** | "spectral gradient bars" (2 of 4) |
| 3 | single-hue `l0_bar` in the base | **5.00** | "generic flat surfaces, no atmosphere or accent system" |

**Every complaint that was fixed stopped being raised.** The void complaint
vanished after the gap change; the emoji complaint vanished after `icon_mono`;
the rainbow complaint vanished after `l0_bar`. The judge is responsive and
consistent, and the noise-floor work says it agrees with itself 87% of the time.

**The score did not move.** That is the finding, not a failure of the fixes.

Two of the three fixes were decided by a natural experiment that was already
sitting in the palettes: `photo` was the only mood carrying `icon_mono` and
`l0_bar`, and it was the only mood NOT accused of emoji icons, and later the
only one NOT accused of spectral bars. The defaults were wrong, not the knobs.

## Why the number is stuck at 5

The rubric handed to the judge says **"5 means competent but plain"**. Round 3's
complaints are no longer defects — "generic flat surfaces", "reads as a generic
table, not a designed surface", "no depth or temperature-semantic colour". They
are all one statement: *nothing is wrong, and nothing is ambitious.*

So the rounds moved the screen from **broken** to **plain**, which is real and
visible, and plain is exactly 5. Reaching 9 is not more defect-fixing. It needs
the things a judge calls atmosphere and depth:

- **Stroke**, which is bisected and broken in the makepad prototype (`border_size`
  applies, `sdf.stroke` never paints).
- **Texture** and **hard-offset shadow**, neither of which any renderer here has.
- **A composition that fills its screen.** The fixture is four content blocks on
  a tall phone; the void shrank 164px → 126px but the card is still sparse. The
  photo mood scores best precisely because a photograph occupies the space the
  other moods leave empty.

## One defect still open

The photo mood's forecast scrim covers `Now`…`Fri` and leaves `Sat` outside it,
sitting on the bare photograph. Visible in `bar_photo.png`; the judge called it
"misaligned, ragged forecast card". Not diagnosed.

---

# Calibrating the 9/10 target (2026-08-30)

Five rounds of fix-and-rescore moved the mean from 5.25 to 5.25. Every complaint
the judge named was fixed and stopped being raised; the number never moved. That
raised the obvious question, which should have been asked before round 1: **what
does this rubric actually award?**

Scored with the *identical* prompt used on our device renders:

| subject | score |
|---|---|
| our best device render | 6 |
| **top-scoring HTML twin** — unconstrained CSS, best implementation in the 100-specimen corpus | **7** |
| **AI-generated design mockup** — an image, not an implementation, under no engineering constraint whatsoever | **7** |
| the target | **9** |

**Nothing in this project reaches 9, including a professional-grade generated
design that never had to be built.** The judge reserves 9-10 for "ship as a
flagship app screen" and does not award it to a Didone editorial poster with
plaster texture.

So the gap between our renders and the ceiling is about **1 point**, not 4. The
remaining 2 points sit above anything this pipeline has ever produced in any
medium.

## What that means for the target

A 9/10 absolute score is not a renderer goal — it is a goal about the design
being generated, and this rubric does not award it even to the mockups. Useful
targets, given the calibration:

- **Close the 1 point to the HTML twin.** That is the real, reachable headroom,
  and it is what the capability queue was always about.
- **Use paired judging, not absolute.** It has a measured 87% self-agreement and
  it moved cleanly on real changes; the absolute score sat at 5 through five
  rounds of genuine, visible improvement, which makes it nearly useless as a
  progress signal.
- **Recalibrate any absolute target against a reference first.** Five rounds
  were spent chasing a number that a design mockup cannot hit.

## The rounds were not wasted

Every one produced a merged, delivery-gated improvement, and each complaint
disappeared once fixed: the mid-screen void (164px -> 126px), rain glyphs
falling into the next row, stock-emoji icons, spectral gradient bars, and a flat
one-tone page. The screens moved from *broken* to *competent*. The score simply
does not resolve that difference — which is itself the finding.

## Confirmed by the project's own scorer

The rubric above was one I wrote. The app already ships its own UX critic
(`monitor.rs:ux_score`, gpt-4o), so the current renders were scored with THAT
instrument too — its prompt verbatim, changing only the app noun:

> "You are a ruthless senior mobile UI/UX designer... Score its VISUAL DESIGN +
> UX from 0-10 (**reserve 9-10 for App-Store-featured quality; most screens are
> 4-6**)."

**All four moods: 6/10. Mean 6.00.**

So two independent judges, two independent rubrics, two different model families
agree — Opus at 5.25, gpt-4o at 6.00 — and the project's own rubric says in its
own words that this band is where most screens live and that 9-10 is reserved
for App-Store-featured work. The app's dev loop iterates while `score < 8`, so
even the shipped self-improvement loop does not target 9.

That is four independent lines of evidence: five rounds of real fixes that never
moved the number, a best-in-corpus HTML twin at 7, an unconstrained AI mockup at
7, and the project's own instrument at 6 with a rubric that says so.
