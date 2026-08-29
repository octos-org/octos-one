# Plan: an ablation loop that grows the L1 alphabet, without touching syntax

Supersedes the ordering in `PLAN-theme-axes.md`. Evidence: `FINDINGS-round2.md`.

## What round 2 changed about the diagnosis

100 specimens, 100 valid, 0 errors. HTML 7.25, cards 2.53, gap **4.72**.

The gap is **not theme variety, it is theme vocabulary**. `accent_hue` — the
purest "more palettes" capability — measures **+0.15 points**, required by 80 of
100 specimens and barely separating them. A thousand palettes would still be
unable to draw a border, cast a shadow, lay a texture or set a serif face.

The school table is the same statement: japanese_ma **3.60** and swiss **3.50**,
whose entire systems are flat fills, hairlines and a weight ramp — an alphabet
L1 already has — against art_deco **1.88** and the stroke-hungry schools at
**2.00**. A 1.6-point spread that no palette count closes.

Two further chunks, honestly separated:
- **composition** (`layered` 2.08 vs `void_field` 3.31) — card structure, not a
  theme concern, and untouched by anything here.
- **generator fluency** — one card emitted no `WeatherIcon` and a degree-less
  hero, using nothing L0 lacks. Free to fix with better exemplars.

## The fence: zero syntax changes

The loop may edit **palettes, the kit, and emitter attribute-writing**. It may
not touch the parser, the catalog, `Attrs`' shape as a contract, or any
card-facing grammar. Consequences, which are the point:

- **Every card that exists stays valid and byte-identical in source.** The
  966-card corpus and the 100 round-2 specimens improve without regeneration.
- No parser risk, no catalog drift, no spec/code disagreement — the entire class
  of blockers the earlier review found simply does not arise.
- A theme-side change is **retroactive**; a syntax change only helps cards
  written after it. The fence is where the leverage is, not a safety tax.

## The queue, cheapest-first

1. **Stroke / border.** `border`, `bordercolor`, `elevation` already exist on the
   shared node and are already read by the evaluator; the Makepad emitter writes
   only background, gradient stop and radius (`l0_widgets.rs:888`). The kit can
   set them today. Required by 53 of 100 specimens and present in every school
   scoring <= 2.15.
2. **Extend the photo-only knobs to dark/light/glass.** `hero_factor`,
   `weight_hero`, `icon_mono`, `panel_inset`, `l0_bar` exist and are wired into
   exactly one mood. Pure palette editing — no Rust, no syntax.
3. **Truncation, theme-side.** A `row_lines`-style knob rather than
   `TextRow(lines:)`. Whether a title truncates is presentation, so the theme
   owning it is more correct under profile §1.1, and it needs no catalog change.
   `Attrs.lines` exists and is never read; the Label widget already supports
   `max_lines` plus ellipsis overflow.
4. **Panel chrome** — elevation and shadow, same mechanism as stroke.
5. **Typography.** Needs a font ROLE carried on the node (an internal struct
   change, not card grammar) and faces placed in `aichat/widgets/resources`,
   which is what `crate_resource` resolves to. Must preserve the deliberate
   CJK/arrow fallback behaviour.

Stop when the next capability's measured lift falls below threshold.

## Gates, all of which already exist

- **Ablation.** The score must rise on specimens whose `dsl_gap` names that
  capability, measured by `--cards-only` over the same 100 specimens — no image
  generation, no HTML. This is the causal test that replaces the confounded
  correlations of round 2.
- **Regression.** The four mood baselines captured 2026-08-24, plus the 966-card
  beauty corpus. Any change that moves cards which never asked for it is
  reverted.
- **Genericity.** How many cards, across how many domains, consume the role
  before it is built. `TextEyebrow` reached 86%; `l0_bar` reached 6% and was
  weather-only.

## Escalation — never done by the loop

The `theme <mood> accent: .x` axis and its `ThemeSpec`/parser work; any new
constructor; a composition plane; bundling font binaries. These are one
deliberate, reviewed change later — and by our own measurement they are the
less valuable half.

## HTML's role

Not an intermediate and never was: the card stage reads the mockup image, not
the HTML. HTML is the **control arm** that says what the same device achieves
with an unconstrained renderer, which is what makes a low card score
interpretable. Pay for it once per corpus; skip it every iteration. It keeps a
separate life as a shipping target for designs whose required capabilities the
DSL genuinely lacks — `dsl_gap` is already the routing key.

## Open questions for review

1. Is the zero-syntax fence actually achievable for every queued item, verified
   against the code rather than asserted?
2. Does stroke work as claimed end to end — kit sets it, evaluator reads it,
   emitter writes it — or is something missing between?
3. Is theme-side truncation really possible, or does `Attrs.lines` need a
   card-facing argument to ever be set?
4. What confounds survive the ablation gate?
5. What breaks if an agent edits palettes, kit and emitter autonomously
   overnight?

---

# Review outcome (2026-08-25)

`REVIEW-codex-loop.md`. Verdict: **not safe to run as described.** Accepted,
with one finding refuted on device.

## Refuted: BLOCKER 5, the generic-background defect

The review argued that containers mapped to plain `View` emit `draw_bg`
properties without `show_bg`, which defaults false (`view.rs:79`), so page
fills and the photo scrim never paint. The citation is accurate and the
conclusion is wrong: assigning `draw_bg.*` through the script apply path
enables the background. Measured on device — the light mood's page reads
**#f2f2f7 both with and without an explicit `show_bg`**, so the emitted fix was
a no-op and was reverted, with the behaviour recorded at the emission site.

Worth stating how nearly this went the other way: an interior-pixel probe
settles it in seconds, and a probe at x=14 lands in the card's dark edge margin
and reads black. I briefly reported "no mood's page colour has ever rendered"
off that bad sample. **Sample the interior.**

## Accepted, and blocking

- **`--cards-only` does not skip HTML.** It reuses the cached document but still
  judges it and may trigger a revision. Real bug; must be fixed before any
  ablation, or every iteration pays for the control arm it was meant to drop.
- **"Stroke" is a bundle, not a knob.** Bauhaus outlines, De Stijl thick rules,
  Deco double frames and neubrutalist hard offsets cannot be one mood-owned
  border, and `Attrs.elevation` is documented Material soft-shadow — it cannot
  express hard-offset or inner relief. The queued item was underspecified.
- **The ablation's confounds are real**: live data changes between A/B renders
  (headlines, prices, row counts — no judge instruction undoes a layout change),
  an unseeded judge on a moving model alias, the `<=3` retry keeping the maximum
  and biasing upward, `WeatherIcon` animating at ~60fps during pixel settling,
  and deleting prior screenshots so no blind paired re-judge is possible.
- **Baselines live outside the repo.** An unversioned baseline is a weak one.

## Accepted, and useful

- **Typography needs no new field**: `Attrs.variant` is already documented as
  the type role on a text node and the kit already uses it for `eyebrow`. New
  faces still need binaries, which stay outside the fence.
- **Theme-side truncation is confirmed feasible** — `lines` is read into `Attrs`
  (`l0_eval.rs:180`) with no card-facing argument, and `Label` exposes
  `max_lines` plus ellipsis. Ellipsis needs a bounded width, so fit/hug cases
  need tests.

## Corrected order

1. Freeze the experiment: pin the judge model, seed **all** sources rather than
   city and photo alone, park animation, retain prior screenshots for blind
   paired judging.
2. Fix `--cards-only` to genuinely skip the HTML stage.
3. Commit the mood baselines into the repo.
4. Pilot the two items the review endorses as defensible zero-syntax work:
   theme-side truncation, and a narrowly scoped `Card`/`Chip` border.
5. Only then revisit broad stroke, panel chrome and typography, each specified
   per surface model rather than as one capability word.
