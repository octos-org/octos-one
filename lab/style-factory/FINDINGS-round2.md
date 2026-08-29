# Round 2 — 100 specimens on a calibrated instrument (2026-08-25)

Design schools instead of vibe labels, phone-matched aspect (896×1920), a judge
told to ignore what no implementation could fix, `quake` replacing the empty
`reading` source, and the constructor contract handed to the translator.

## Operationally, the first clean run

**100 specimens, 100 valid cards, 100 judged, 0 errors.** Round 1 lost 22% of
cards to invented arguments; injecting `ui-l0-constructors.toml` into the
translation prompt eliminated that failure mode entirely.

| | HTML twin | Splash card | gap |
|---|---|---|---|
| round 1 (vibe genres, 0.667 aspect, biased judge) | 6.58 | 2.76 | 3.82 (sd 1.79) |
| **round 2 (schools, phone aspect, fixed judge)** | **7.25** | **2.53** | **4.72** (sd 1.29) |

The gap widened because both ends moved for good reasons: HTML rose once the
handicaps were removed, and the cards face a genuinely harder style space —
round 1 was mostly photo-backdrop and material-light, the two things L0 already
does best. The tighter spread says this is a systematic capability deficit, not
a few bad specimens.

## The headline: my Phase 1 was wrong, and the measurement says so

Round 1 ranked work by how often the judge *named* a gap — colour led at 92%, so
accent became Phase 1. Round 2 can measure something better: the **cost** of a
missing capability, by comparing specimens that require it against those that do
not.

**`accent_hue`: +0.15 points.** Required by 80 of 100 specimens, and cards that
need it score barely below cards that don't. The capability I planned to build
first is, on its own, close to worthless.

That is exactly the caution written into `BEAUTIFICATION-PLAN.md` — *"92% is
complaint frequency, not expected lift"* — now confirmed against my own ordering.

## A methodological correction that matters more than any single number

The first cut of this analysis reported `stroke` at +1.09, the largest cost on
the board. **That number is not trustworthy.** Capabilities are declared per
*school*, so most of them never vary within a school — `stroke`, `tracking`,
`serif_display`, `rotation`, `pattern`, `geometric_shape`, `mask`, `font_pair`
and `palette_key` are perfectly confounded with the school that requires them.
Their apparent costs are school effects wearing a capability's name.

Only capabilities that vary independently (via sampled harmony, dialect or
media) support an estimate:

| capability | cost | n requiring |
|---|---|---|
| hard_shadow | +0.64 | 17 |
| texture | +0.56 | 16 |
| glow | +0.24 | 18 |
| **accent_hue** | **+0.15** | 80 |
| bevel | +0.14 | 5 |
| gradient | 0.00 | 19 |
| soft_shadow | −0.48 | 2 |
| pixel_font | −0.82 | 12 |

## What the schools say instead

| school | card | what its style needs |
|---|---|---|
| japanese_ma | **3.60** | flat fill, hairlines — L0 has these |
| swiss | **3.50** | flat fill, hairlines, weight ramp — L0 has these |
| editorial | 2.75 | serif display, font pair |
| organic | 2.67 | masks, gradient |
| bauhaus | 2.15 | stroke, geometric shape |
| punk_zine / de_stijl / memphis / constructivist | 2.00 | stroke, texture, rotation, pattern |
| art_deco | 1.88 | serif display, tracking, stroke |

A **1.6-point spread** between schools L0 can already serve and schools needing
vocabulary it lacks. That is the honest size of the missing-vocabulary prize —
but it does not say which piece is worth what.

## Composition, first real reading

Round 1 could not answer this at all (every layout was hero-plus-list). Now:

    void_field 3.31 · asymmetric 2.57 · modular_grid 2.41 ·
    magazine 2.33 · layered 2.08 · hero_stack 1.50 (n=2)

L0 handles **emptiness** well — a void field is mostly spacing, which it has —
and handles **layering** worst, having no overlap or z-order vocabulary. Also
confounded with school (MA→void, constructivist→layered), so treat as a hint.

## Domains, and the `quake` swap

weather 2.44 · news 2.68 · stock 2.44 · quake 2.56 — even, with no dead domain.
Round 1's `reading` scored 1.95 purely because its source was an empty device
list. The USGS feed fixed it.

Exemplars (cards ≥5): `r021-weather-swiss` 6, `r032-news-japanese_ma` 5,
`r034-news-japanese_ma` 5. All from the two schools L0 can already serve.
36 of 100 HTML twins reached ≥8.

## What to do next: ablate, don't correlate

Recipe labels cannot separate a capability from the school that demands it, and
no amount of statistics on this corpus will fix that. The corpus was built so we
would not need to: **build one capability, re-run `--cards-only` over the same
100 specimens, and measure the lift.** That is causal rather than correlational,
and it costs one overnight with no image generation.

**Phase 1 should be `stroke`, not accent**, on three grounds:

1. It is the most-required missing capability (53 of 100) and appears in every
   school scoring ≤2.15.
2. It is the cheapest thing on the board to implement. `border`, `bordercolor`
   and `elevation` already exist on the shared node and are already read by the
   evaluator — the Makepad emitter simply never writes them
   (`l0_widgets.rs:888` emits background, gradient stop and radius only). This
   is emitter work, not a new contract.
3. Its measured cost is unknown *because* it is confounded, and the ablation is
   precisely how that gets resolved.

Accent stays on the roadmap — a theme needs a hue eventually — but at +0.15 it
has lost its claim to going first.
