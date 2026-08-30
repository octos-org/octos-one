# E0/E1: mockups have a gradient; cheap metrics don't find it (2026-08-29)

The mockup-as-ground-truth plan rests on an assumption nobody had tested: that
mockups differ in quality in a way that can be read off and reused. Mockup image
quality had scored a uniform 8–9 in every earlier measurement, and a score that
never varies carries no information. So before extracting anything, two
questions.

## E0 — is there a gradient? **Yes, and a clean one.**

8 distinct mockups, every pair judged blind by Opus in **both orders**: 56
comparisons.

| | result | |
|---|---|---|
| self-agreement under order swap | **24/28 (86%)** | 50% is a coin; 87% was measured on rendered cards, so this matches the judge's known ceiling |
| triads consistent | **36/36 (100%)** | no cycles at all — a perfect total order |

| wins | mockup |
|---|---|
| 7/7 | `ref_mockup` — Florence, terracotta |
| 5/7 | `r012-weather-art_deco` |
| 3/7 | `r004-weather-punk_zine`, `r010-weather-punk_zine`, `r021-weather-swiss` |
| 2/7 | `r061-stock-swiss` |
| 1/7 | `r034-news-japanese_ma` |
| 0/7 | `r096-quake-memphis` |

The differences are real, shared, and globally ordered. **The premise holds.**

## What the gradient actually is

One axis, and the judge names it in almost every verdict: *restrained*,
*generous negative space*, *disciplined grid*, *confident hierarchy*, *decisive
scale jump*. Not "beauty" in the round — a single dimension.

That is good news for extraction. One dominant axis is far easier to lift than
multidimensional taste.

## E1 — can that axis be computed? **Not by these metrics.**

Four measures from the Ngo / Aalto-Interface-Metrics family, Spearman against
the judge's ranking. With n=8, |rho| > 0.71 is p < 0.05.

| metric | rho | |
|---|---|---|
| negative space | −0.27 | |
| distinct hues | −0.05 | |
| scale spread | +0.20 | |
| edge-density clutter | −0.24 | |

**Nothing predicts.** The best is a third of what significance requires.

### Why — and this is the useful part

The axis is **not monotonic** in any area-based measure. The *lowest-clutter*
mockup in the set (`japanese_ma`, 0.067 edge density against `ref_mockup`'s
0.056) lost all seven of its comparisons. The judge's reasons say exactly why:

> "B's void reads as blankness" · "empty, not a pause" · "A's empty half orphans
> a stray element" · "void is unmotivated"

Clutter loses. Emptiness loses just as hard. What wins is a **decisive
hierarchy** — a dominant focal element with everything else clearly subordinate.
The distinction the judge is making is *negative space* versus *void*, and no
proportion-of-pixels metric can tell those apart: both are large areas of
uniform colour.

One measurement defect worth recording separately: `negative_space` ranked
`ref_mockup` **lowest** in the set, when it visibly has the most. Its background
is a textured terracotta wall, so quantisation scatters the ground across many
bins. The metric is defeated by texture, not by the design.

## Standing

This is the third time metrics ported from the aesthetics literature have failed
to transfer here. It is worth being precise about what was and was not shown:

- Not shown: that the axis is incomputable. **n=8 cannot support metric-fishing**
  — with four metrics on eight points, something would eventually correlate by
  chance, and stopping now is the honest call rather than iterating until one
  does.
- Shown: these four, as written, do not predict, and the reason is structural
  rather than a tuning problem.

## What this does and does not say about extraction

E1 asked *"can a metric predict which mockup is better?"* — it did **not** ask
*"can a palette or a type ratio be lifted from a mockup and rebuilt with?"*
Those are different questions and only the first was answered.

Lifting ingredients is still open, and it is a much easier problem: a palette is
extracted, not judged. But it stays blocked on the accent axis, which currently
writes to `l0_accent` (**0 readers**), a bar, a highlight and a button — and
never touches `l0_base`, `l0_fill`, `l0_sheet` or `l0_text`. Nowhere for an
extracted palette to land.

## Next, in order

1. **Rewire accent** to the four surface/ink cans, holding luminance. Nine files,
   literal values. Until this lands, nothing downstream can be measured.
2. **Grow the mockup pool** past 8. Every conclusion here is bounded by n=8, and
   that bound is doing most of the work in the null result.
3. **Then test extraction fidelity directly** — lift a palette, rebuild, and put
   the rebuild against the original in the same paired judge. That measures
   whether extraction preserves anything, which is the actual question.

Do not build a metric for "decisive hierarchy" yet. At n=8 it would be fitted,
not found.

---

# The judge: glm-5.3-flash vs Opus

Opus at 56 comparisons per experiment does not scale — judging a 50-mockup pool
is 2,450 comparisons. So the same eight mockups, the **identical prompt**, and
the identical 56 comparisons were run through Z.ai's `glm-5.3-flash` on the
coding endpoint.

It reads UI images accurately — asked what colour the Florence mockup's ground
is and what its largest number is, it answered "orange" and "30 (Wednesday's
high)", both correct.

| | Opus | glm-5.3-flash |
|---|---|---|
| self-agreement under order swap | 24/28 (86%) | **22/28 (79%)** |
| transitive triads | 36/36 | **28/28 (100%)** |
| wall clock, 56 comparisons | minutes | **45 s** |
| tokens | — | 133 k prompt, 1.2 k completion |

So it is genuinely reading the images, and its ordering has no cycles at all.
But it is **not a drop-in**:

| | |
|---|---|
| rank correlation with Opus | **rho = +0.65** — below the 0.71 that n=8 needs |
| same winner on jointly-decided pairs | 16/18 (89%) |

### The disagreement is systematic, not noise

Both disagreements involve `r034-news-japanese_ma`, the sparse one, and
`r021-weather-swiss` moved from 3/7 under Opus to 6/7 under GLM. Opus reads that
emptiness as unresolved — *"void reads as blankness"*, *"empty, not a pause"* —
where GLM rewards it as *"elegant whitespace"*.

**GLM likes minimalism more than Opus does.** That is a bias, not a wobble.

## Cascade: cheap first, escalate the flips

A pair where GLM contradicts itself under order swap is a pair GLM is unsure
about. Route only those to Opus. Simulated over both complete datasets, so this
costs nothing to check:

| | |
|---|---|
| decided by GLM alone | **22/28 (79%)** |
| escalated to Opus | 6/28 |
| agreement with pure Opus | **22/24 (92%)** |
| rank correlation | **rho = +0.85** — above significance |

Four fifths of the work at flash cost, and the ranking holds.

**What the cascade does not fix:** GLM was *self-consistent* about preferring the
sparse mockup, so that bias never escalates. Cascading removes noise, not bias.
For a final ranking, or any question where minimalism is the axis under test,
Opus still has to see the pair.

## Practical protocol

```
glm-5.3-flash on every pair, both orders
  agrees with itself   -> take it                (79% of pairs)
  contradicts itself   -> escalate to Opus       (21%)
```

This is what unblocks growing the mockup pool past 8 — the bound that is doing
most of the work in the null result above.

Key is read from `ZAI_KEY` or `ZAI_KEY_FILE`, never checked in. Endpoint is
`api.z.ai/api/coding/paas/v4` — the plain `paas/v4` path returns "insufficient
balance" on this account. `glm-5.3` (non-flash) is text-only and rejects image
content; `glm-4.6v` also works.

---

# E1/E2/E3 — extraction, three ways

Three independent extractors over the same eight mockups: pure CV, Opus, and
glm-5.3-flash. The schema is deliberately RELATIVE — absolute pixel sizes are
meaningless from an image with no dpi, and font names are ~80% top-5 at best and
irrelevant when you ship one font.

## E1 — palette

Colours compared as CIE deltaE76. Under 10 is the same colour family.

| field | cv vs opus | cv vs glm | **opus vs glm** |
|---|---|---|---|
| ground | 5.1 (6/8 within 10) | 6.6 (5/8) | **2.8 (8/8)** |
| ink | 43.3 (4/8) | 54.2 (4/8) | **3.9 (6/8)** |
| accent | 74.7 (3/8) | 72.7 (1/8) | **6.3 (5/8)** |

**The two models agree almost exactly; the CV baseline is only usable for the
ground.** That inverts the assumption this experiment started from. Pixels do not
have opinions about their own colour — but "which of these colours is the *ink*"
is a semantic question, not a pixel one, and k-means cannot answer it. My "most
distant luminance" and "most saturated" heuristics are simply wrong.

Where CV disagrees on ground it is also arguably wrong: on the punk-zine collage
it returned the yellow-green overlay, where both models returned the paper white
underneath. The models are reading the *page*; CV is counting pixels.

## E2 — type

Hero-to-body ratio. No reference exists, so the three only bound each other.

| mockup | cv | opus | glm |
|---|---|---|---|
| punk_zine r004 | **1.0** | 8.5 | 8.0 |
| punk_zine r010 | **1.0** | 12.0 | 9.0 |
| art_deco | 13.25 | 9.0 | 7.0 |
| swiss | 8.92 | 8.5 | 5.0 |
| japanese_ma | 2.83 | 2.6 | 3.0 |
| ref_mockup | 6.57 | 12.0 | 8.0 |

CV collapses to 1.0 on both punk-zine mockups — its band detector needs a flat
ground and those are full-bleed textures. The same texture failure that broke
`negative_space` above. The models correlate directionally, with **GLM
systematically lower than Opus**.

## E3 — composition

**Margin fraction is the one field all three agree on**, and closely:
ref 0.075 / 0.085 / 0.08 · swiss 0.072 / 0.075 / 0.07 · japanese_ma 0.062 /
0.065 / 0.06. Within about a percentage point across three independent methods.

`hero_align` agrees 6–7 of 8 on every pair.

`bands` does not agree at all — CV says 13–18 where the models say 2–5. CV is
counting text lines; the models are counting semantic groups. The field is
underspecified rather than mismeasured.

### E1/E2/E3 summary

| what | verdict |
|---|---|
| margin fraction | **extractable** — three methods within ~0.01 |
| ground colour | **extractable** — models within deltaE 2.8 |
| ink, accent | **models only** — CV cannot, it is a semantic question |
| hero alignment | **extractable** — 7/8 |
| hero-to-body ratio | directional; GLM biased low; CV fails on texture |
| band count | **not extractable as specified** |

---

# E4 — the round trip. **Extraction loses.**

Six cards, each built three ways — shipped palette, Opus's extraction, GLM's
extraction — and judged in both orders by both judges. Layout, content and type
are identical across the three arms, so a win is attributable to colour alone.

| judge | pairs decided | extracted wins | shipped wins |
|---|---|---|---|
| glm-5.3-flash | 9/12 | 5 | 4 |
| **Opus** | 7/9 | **1** | **6** |

GLM calls it a coin flip. Opus rejects extraction decisively — and its reasons
are specific, repeated, and correct:

> "B's white icons vanish on cream" · "A's dark-on-dark text is illegible" ·
> "Panel lifts off background; B's card is flat near-black, indistinguishable"

## Why it loses — three named defects

**1. A palette is not (ground, ink, accent).** The first transfer set only
`l0_base` and `l0_text` and produced an unreadable screen. The shipped dark
palette expresses its secondary inks and every overlay as WHITE at low alpha —
`l0_soft` 230, `l0_dim` 153, `l0_fill` 18, `l0_hairline` 26 — all of which assume
a dark ground. Rebuilding each value as a *position on the ground-to-ink axis*
fixed legibility for body text and is what the shipped transfer now does.

**2. Icons are outside the palette.** They stay white-derived whatever the
override says, so a light extraction leaves them invisible. Opus names this in
four separate verdicts. This is a real renderer gap, not an extraction gap.

**3. Panel separation is a ratio, not a colour.** The shipped dark palette gets
its card-off-page separation from an *alpha overlay*, which self-scales to any
ground. An absolute extracted colour does not, so on a near-black ground the
panel disappears.

## The gates caught most of it, before any judge

Running the contrast gate over the same eighteen renders:

| | |
|---|---|
| shipped palette | **6/6 clean** (5 PASS, 1 UNKNOWN) |
| extracted palettes | **8/12 FAIL** |

Zero false alarms on the shipped arm, and it flags two thirds of the extraction
damage — including a 1.3:1 and a 1.8:1 that no judge should ever have been asked
about. This is the lexicographic rule working exactly as intended: reject on the
invariant, spend the judge only on what survives.

Where the gate and Opus disagree they are answering different questions. The
gate measures text against its ground; Opus *also* penalises panel-vs-page
separation, which **no gate currently checks**. A first attempt at that metric
reported 1.00:1 on every render including known-good ones — it is broken, and is
recorded here as unbuilt rather than as a number.

## What E4 actually establishes

- Palette transfer as implemented is a **net negative** — it loses 1–6 to the
  shipped palette under the judge that reads legibility properly.
- The failure is **not in the extraction**. Opus and GLM agree on the ground to
  within deltaE 2.8. The failure is in the *transfer*: a palette is a system of
  relationships with a minimum contrast and a panel separation that have to be
  **enforced**, not merely carried across.
- The cheap judge **cannot be trusted on legibility**. GLM scored 5–4 on renders
  where Opus found white icons on cream and dark-on-dark headlines. That is a
  sharper limit than E0's minimalism bias, and it means the cascade must never
  be the only thing standing between a broken palette and a ship.

## Next

1. **Constrain the transfer, then re-run E4.** Clamp ink so text meets 4.5:1
   against the extracted ground before rendering, and express panel lift as an
   alpha overlay rather than an absolute colour. Both are small changes and both
   target a named defect.
2. **Build the panel-separation gate** properly. The defect is real and Opus
   found it three times; the metric is not written.
3. **Bring icons into the palette.** Until then no light extraction can win.

---

# E4 v2 — the three fixes, measured. **No improvement.**

Same six cards, same mockups, same judges, same prompt. Only the transfer
changed: ink clamped to 4.5:1 against the extracted ground, panel lift expressed
as a ratio (target 1.12:1, the value both shipped palettes happen to land on),
and `icon_mono` pinned so icons take the extracted ink.

| judge | v1 extracted / shipped | v2 extracted / shipped |
|---|---|---|
| glm-5.3-flash | 5 / 4 | 3 / 3 |
| **Opus** | **1 / 6** | **1 / 6** |

## Two of the three fixes did land

Opus says so directly, in the verdicts where extraction won or came close:

> "Flatter, deeper panel lifts card and holds secondary text contrast better" ·
> "Warm ivory headline reads chosen, not default white; legibility unchanged"

So the panel-lift ratio works, and an extracted ground genuinely reads as
deliberate rather than default. The transfer is better than it was. It still
loses, because two defects that have nothing to do with the palette dominate the
verdicts.

## Why it still loses

**1. The "icons" are emoji.** Four separate verdicts say *"white icons vanish on
cream"*, and a crop confirms it. The source is:

```
TextRow(text: "🍽️")
```

An emoji in a text node. Colour-font glyphs carry their own colour and ignore
`draw_text.color`, so **no palette can tint them**. `icon_mono = 1` was pinned
and reached nothing, because these were never icon widgets.

This is an authoring finding, not a palette finding: **a card that uses emoji as
icons is not theme-portable.** It renders correctly under exactly the polarity
its author had on screen and is invisible under the other one. Worth a lint at
the L0 level, not a fix in the transfer.

**2. Photo-backed cards have a different ground.** Three more verdicts —
*"dark text on photo loses contrast"*, *"off-card text nearly vanishes into the
photo"*. On a `Photo` root the text does not sit on `l0_base` at all; it sits on
a photograph, and legibility comes from `l0_scrim` / `l0_scrim_top`, which the
override never sets. Clamping ink against the extracted ground is the right
calculation against the wrong surface.

**3. The accent clamp can flatten a design.** One verdict: *"gold monochrome
flattens emphasis"*. Pushing an accent to 3:1 against the ground moved it onto
the ground-to-ink axis, which is exactly where an accent should not be.

## What the clamp measurement itself showed

Worth recording because it corrects the v1 diagnosis. Across all sixteen
extractions the ground/ink pairs mostly cleared AA already — 10:1 to 15:1 — and
only three needed clamping at all (3.6→4.6, 3.8→4.6, 4.0→4.7).

So v1's dark-on-dark text did **not** come from a bad extracted pair. It came
from the DERIVED values — `l0_dim` and `l0_soft` sitting too near the ground —
and from photo grounds. The clamp fixed a problem that was mostly not there.

One real bug the preview caught before it shipped: the first clamp chose its
direction by a luminance threshold, so against a terracotta ground it pushed
toward white and topped out at 4.2:1 when black would have reached 6.2:1.
Choosing the destination by which one actually reaches further fixes it.

## Standing

Palette transfer is now better built and still a net negative. The remaining gap
is **not** in the palette:

- emoji-as-icons cannot be tinted by anything — needs an L0 lint
- photo grounds need the scrim in the transfer, not the base
- accents must stay off the ground-to-ink axis

Until the first two are addressed there is no point re-running E4 a third time;
the verdicts would name the same two things again.

---

# Blocker check — and the E4 sample was rigged

Two blockers came out of the v2 verdicts. Measuring how widespread each is turned
up something worse than either of them.

## How much of the corpus a palette can reach

| card shape | count | share | why the palette misses it |
|---|---|---|---|
| photo root | 395 | **41%** | text sits on a photograph; legibility comes from the scrim, not `l0_base` |
| emoji as icon | 54 | 6% | colour-font glyphs carry their own colour and ignore `draw_text.color` |
| map surface | 30 | 3% | the card sits on a live map, same problem as a photo |
| **transferable** | **488** | **50%** | |

Half the corpus. Not a rounding error either way.

## The sampling error

The six cards E4 ran on: **four photo-backed, two emoji.** Zero of six clean,
against a corpus rate of 50%.

`make_samples.py` takes one card per model family in alphabetical order, and the
first six families happen to be dashboards and travel cards, which skew heavily
photographic. So E4's 1–6 verdict was never measuring the transfer. It was
measuring what happens when you repaint a surface the paint cannot reach.

This is the same failure as the byte-identical weather corpus earlier in this
document: a sample drawn by convenience rather than by design, producing a
confident number about the wrong thing.

## E4 v3 — the same experiment on cards the palette can reach

| judge | v1 (blocked) | v2 (blocked, fixed transfer) | **v3 (transferable)** |
|---|---|---|---|
| Opus | 1 ext / 6 ship | 1 ext / 6 ship | **5 ext / 5 ship** |
| glm-5.3-flash | 5 ext / 4 ship | 3 ext / 3 ship | **0 ext / 6 ship** |

Note v3 is a different card population, not a further transfer change — the
v1→v2 pair is the transfer ablation, v2→v3 is the blocker effect. The blocker
effect is much the larger of the two.

**Under Opus, extraction goes from decisively losing to a dead tie.** Its reasons
are consistent about why the extracted arm wins when it wins:

> "warm off-white feels chosen" · "bone card reads chosen against green" ·
> "warm paper palette reads chosen; darker meta text stays legible"

## And the two judges invert

This is the sharpest judge finding yet. On the same six cards they go opposite
ways — Opus 5–5, GLM 0–6 — and each is internally consistent about its reason:

| judge | rewards | typical verdict |
|---|---|---|
| Opus | **deliberateness** | "warm bone palette reads chosen" |
| glm-5.3-flash | **raw contrast** | "pure white magnitudes punch harder; B's grey mutes emphasis" |

Neither is wrong. A card in white-on-near-black does have more contrast; a card
in warm bone does read as more deliberate. But it means the cheap judge is not
merely a noisier Opus — **on palette questions it optimises for a different
thing**, and the cascade cannot fix that, because GLM is perfectly
self-consistent about it.

The two cards both judges reject agree with this reading: `026-quake` took a grey
source palette and got a duller card ("grey mutes every emphasis"), and
`019-nav` is a map card that slipped through the filter before maps were counted.

## Standing

- The E4 verdict in the section above is **withdrawn**. It was measured on a
  sample where every card hit a blocker.
- On cards a palette can reach, transfer is **level with the shipped palette
  under Opus** — not yet a win, no longer a loss.
- The corpus splits 50/50 on whether a palette can reach it at all. Photo roots
  at 41% are not an edge case to fix later; they are half the remaining problem
  and they need the scrim in the transfer.
- Emoji-as-icon is 6% and wants an L0 lint, not a transfer fix.
