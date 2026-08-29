# Cheap aesthetic gates: what transfers, and what does not (2026-08-30)

There is real literature on computationally checkable interface aesthetics —
Ngo et al.'s fourteen measures (balance, equilibrium, symmetry, density,
simplicity, …), later simplified to seven, and Aalto Interface Metrics, which
computes seventeen from a screenshot or URL. Four of those map directly onto
defects found by hand this session, so they looked like free wins.

**Three of the four do not work as naively implemented.** Measured against two
known-good renders and one known-bad:

| gate | broken quake card | quake exemplar (good) | shipped weather (good) | verdict |
|---|---|---|---|---|
| contrast | 18.3:1 pass | 20.4:1 pass | **1.0:1 fail** | wrong region |
| alignment | 5 edges | **10 edges** | 3 edges | inverted |
| dead space | 24% | 15% | 11% | no separation |
| palette | 1 hue | 4 hues | 2 hues | no separation |

## Why each failed

**Contrast samples the wrong thing.** By hand it caught three real bugs — a card
that had merged into its page (separation 0), a bar at 1.76:1 on navy, a hero at
2.59:1 over a photograph — but each time I chose the two regions myself: this
bar against that panel. Automatically it picks the most common ink and the most
common background across the whole screen, which on `ship_light` lands in the
app's dark green chrome and reports 1.0:1 against itself. **Contrast is a valid
gate that requires element segmentation first**, which is precisely why AIM
segments the page before computing anything. Skipping that step does not
approximate it.

**Alignment is inverted.** The premise — fewer distinct left edges is more
disciplined — is wrong for card layouts. The good exemplar has TEN because it is
a legitimate multi-column list (magnitude, place, time); the broken card has
five. A metric that scores the good render worse than the bad one is not
mis-tuned, it is measuring the wrong property.

**Density and palette do not separate.** Both pass everything at any threshold
that does not also fail the good renders. With three samples that is unsurprising
and not yet evidence either way.

## What does work

`layout_lint.py` — a text block taller than it is wide has been squeezed
narrower than its content. One rule, no thresholds to tune, and it separates
cleanly: one failure on the broken card, clean on six known-good renders.

## The pattern, again

Every one of these three failed for the same reason the session's other dead
ends did: **the metric was ported from a context where its assumptions held into
one where they do not**, and I did not check the assumptions before trusting the
number. Ngo and AIM operate on segmented interfaces. Mine operate on raw
screenshots including a phone bezel and app chrome. The literature is sound; the
port was not.

Not shipped. `layout_lint.py` stays; the other three are recorded here so the
next attempt starts from segmentation rather than from pixels.
