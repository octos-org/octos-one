# The accent axis is wired to the wrong tokens (2026-08-30)

Three separate conclusions were drawn about theme axes this week. All three were
measurements of a control that is barely connected.

## The measurements

| reading | conclusion drawn at the time |
|---|---|
| `accent_hue` costs **+0.15** across 100 specimens | "more palettes are close to worthless" |
| axes vs no axes, blind paired: **52/48**, and 44/43 at p=1.000 | "the axes are inert" |
| mockup fidelity, six specimens: original 3.00, no-axes 2.67, **axes restored 2.67** | — |

The third was run because absolute quality scoring is flat in the 5-7 band and
fidelity-to-a-mockup looked like the better instrument. It is. But restoring the
axes moved fidelity by exactly nothing, which is not what "the axes carry the
palette" predicts.

## The mechanism

`_axis_accent_amber.splash` sets five tokens. Counting how often the kit reads
each:

| the axis SETS | kit reads it |
|---|---|
| `l0_accent` | **0** |
| `l0_bar` | 4 |
| `l0_active` | 2 |
| `l0_go` | 1 |
| `l0_bar_rail` | 1 |

| the axis does NOT touch | kit reads it |
|---|---|
| `l0_text` | **11** |
| `l0_fill` | **7** |
| `l0_sheet` | 6 |
| `l0_base` | 5 |
| `l0_dim` | 4 |

**Nothing reads `l0_accent` at all.** What the axis actually reaches is a
temperature bar, a selected chip and one go-button. A stock card has no
temperature bar, which is why a judge reported "amber accent absent (all white)"
on `r061-stock-swiss` while 6.4% of that render's pixels had changed elsewhere.

A mockup's palette is its GROUND and its INK. The accent axis cannot touch
either.

## What this changes

The axes are not a failed idea and the measurements were not wrong. The control
was mislabelled: it is a widget-tint knob presented as a palette knob, and every
experiment that asked "does changing the palette help?" was actually asking
"does tinting three widgets help?" — to which +0.15 and 52/48 are entirely
reasonable answers.

**The testable next step**, which has never been run: bind `accent` to
`l0_base`, `l0_fill` and `l0_text` — the tokens that actually carry a palette —
with contrast held, and re-measure fidelity against the same six mockups. If
fidelity moves, the axes were starved rather than useless. If it does not, the
idea is dead on evidence rather than on a miswiring.

## Also found

The 100-card corpus **does not parse on the shipping build**: every card's theme
line carries axes (`theme light radius: .none accent: .neutral …`) and the axes
grammar was never merged. Any future corpus run needs either the axes merged or
the theme lines stripped, and stripping changes what is being measured.
