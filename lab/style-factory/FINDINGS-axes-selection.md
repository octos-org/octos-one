# The radius "defaulting defect" did not exist (2026-08-26)

Two experiments were spent on a phantom. This is the correction, and the
methodology error that produced it.

## What I claimed

Axes-v2 showed `shape` chosen as `.square` in 76% of declarations, against an
explicit prompt warning not to default. I read that as *the generator will not
vary this axis*, hypothesised **vocabulary fluency** as the cause, and renamed
the axis to the Tailwind/Radix scale (`radius: .none/.small/.large/.full`) on
the theory that a scale the model has read a million times would unstick it.

Falsifiable prediction, stated in advance: *if fluency was the constraint,
`.none` should stop being the default.*

## What the corpus says

**Borrowed names measured 51 / 48 / 1 — no effect.** And `.none` rose to 88%.
By the stated prediction, the hypothesis is refuted.

But the aggregate was never the right statistic. Conditioned on school:

| | chose square | correct? |
|---|---|---|
| 8 sharp-corner schools (swiss, bauhaus, de_stijl, constructivist, punk_zine, art_deco, editorial, japanese_ma) | **79/80 = 99%** | yes — these schools *are* square |
| organic | 2/9 — picked `.large` 5 times | yes — varies when style calls for it |
| memphis | 11/11 | arguably wrong, arguably not |

**The 88% is corpus composition, not model behaviour.** Eight of ten schools
genuinely use sharp corners, so a correct generator *should* emit mostly
`.none`. `organic` is the existence proof that selection works: a blindly
defaulting model would have gone 9/9 there too.

There was no defaulting defect to fix. The rename measured no effect because it
solved nothing.

## The axes are working better than the aggregates suggested

Same conditional lens, all five axes — does the choice vary *by school*?

| axis | global mode | schools departing | verdict |
|---|---|---|---|
| accent | `.amber` 26% | **7/10** | design-driven |
| density | `.compact` 39% | **6/10** | design-driven |
| emphasis | `.poster` 54% | **5/10** | design-driven |
| icons | `.mono` 62% | 2/10 | partial |
| radius | `.none` 88% | 1/10 | flat |

The departures are **historically correct**: bauhaus→red, de_stijl→blue,
swiss→neutral, japanese_ma→green + airy density, editorial→airy, swiss/MA→
`.clear` emphasis. The model is applying real design-history knowledge, not
picking at random and not reflexively defaulting.

`radius` is flat most likely because the design space itself is flat there —
20th-century schools are overwhelmingly square-cornered. Low variance in the
output can mean low variance in the correct answer.

## The methodology error, which is the durable part

**An aggregate selection skew is not evidence of defaulting.** It is only
evidence once you condition on what the correct answer would have been. I
compared the distribution against *uniform* — an implicit assumption that all
four values should appear equally — when the design-correct distribution was
heavily skewed toward one value.

Cost: two full corpus runs and a shipped rename, chasing a defect that the
first conditional table would have ruled out in one query.

The same check now belongs in front of any "the model won't vary X" claim:
**tabulate against the design-correct answer, not against uniform.**

## Separately: a race I created

The cumulative axes-vs-no-axes run read `{id}-card.png` while the corpus re-run
was overwriting that same file. Early comparisons used axes-v2 renders, later
ones axes-v3, with no clean boundary. That result (55/45) is discarded, not
merely non-significant; the artifacts survived, so it re-runs against the
stable `-card-prev.png`. Parallelising two jobs that share a mutable path is
the same class of error as the confounds this corpus was built to avoid.
