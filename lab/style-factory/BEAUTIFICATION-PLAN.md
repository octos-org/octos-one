# The beautification plan

**Thesis: generated cards are not ugly because the model lacks taste — they are
ugly because the profile cannot say what the model already understands. Close
that gap in the order the measurements dictate, and every card ever generated
improves at once.**

This is the umbrella plan. Detail lives in its siblings: `FINDINGS.md` (the
evidence), `TAXONOMY-codex.md` (design theory behind the style space),
`PLAN-theme-axes.md` (the theme-axis design), `REVIEW-codex.md` (its review).

---

## 1. What we know, and how we know it

120 specimens ran the full rail — a generated mockup, an HTML twin written from
it, an L0 card written from it, both rendered on a real phone, every stage judged
against the mockup:

| | fidelity to the mockup |
|---|---|
| HTML twin, in this app's own webview | **6.67** (median 7) |
| Splash L0 card, on device | **2.76** (median 3) |

The same model wrote both from the same image, so the gap is not comprehension.
It is vocabulary, and the judge itemized it — colour/accent named in **92%** of
cards, serif/display face 62%, icon style 52%, panel chrome 34%, truncation 31%,
charts 15%. Root cause of the top line: `THEMES = [dark, light, glass, photo]`
and no colour vocabulary in the catalog at all, so **style is capped at four
looks** however good the generator gets.

Two corrections to that evidence, both found after the run and both honest
limits on it:

- **The mockups were the wrong shape.** 1024×1536 (0.667) against a 0.462 phone
  — 44% too wide. 42% of HTML judgments and 31% of card judgments mention
  emptiness, clipping or scale. Absolute means are deflated; the delta partly
  cancels; per-genre *rankings* survive because every specimen carried the same
  handicap.
- **The style space was named by vibe.** Ten genre labels, and a layout axis
  whose four values were all "hero + list" variants — so compositional variance
  across 120 specimens was effectively zero.

## 2. Why this is one loop, not two projects

The dataset is the instrument that prices the renderer backlog; the renderer work
is what the readings are for; the re-run is the measurement. Running either alone
fails: build features without the instrument and you are guessing, run the
instrument without building and you produce reports.

    calibrate the instrument → baseline → build what it prices → re-measure

The instrument is currently miscalibrated (§1), so **calibration comes first**.
It is also cheap — recipes, one constant, a judge prompt, two harness flags.

---

## Stage 1 — recalibrate, and lay the contract  *(parallel, ~1–2 days, no pixel changes)*

Two tracks, different repos, neither visual.

**Instrument.** Rewrite `recipes.py` on the layered model from
`TAXONOMY-codex.md`: choose `composition_school` + `surface_model` + optional
`digital_dialect`, apply their hard exclusions, then sample only legal
primitives (type class, scale ratio, geometry, contrast strategy, figure-ground,
ornament). Photography drops to **5%** of media — it must stop being the
generator's shortcut to richness. Add Codex's `required_capabilities` gate:
refuse to generate a mockup whose capabilities neither HTML nor the DSL can
compile. Aspect becomes **896×1920** (0.467, the phone is 0.462), with
1024×2304 for scroll-length designs and 1792×1920 for paired photo splits. Fix
the `reading` domain's empty source. Rewrite judge instructions to ignore
aspect-driven emptiness, imagery no implementation could obtain, and truncation
caused by real data. Add `--cards-only`, inject the constructors TOML into the
translation prompt, and extract mockup thumbnails/avatars as servable assets.

**Contract (renderer Phase 0).** `ThemeSpec` replacing the single theme string;
same-line axis parsing via `Token.line` (the lexer discards newlines); the axis
catalog with code/spec agreement tests; host override resolution generalizing
the existing light-over-Photo substitution; the render-cache key extended to
include the effective spec; the token-ownership table and its collision test;
and **per-mood baselines captured before anything visual ships** — today's four
device goldens are card goldens, not mood goldens.

## Stage 2 — baseline, and settle the one disagreement  *(~1 overnight + half a day)*

Run **round 2** on the corrected instrument. Its purpose is a trustworthy
baseline, not a record.

Then the **composition probe**: six hand-written cards with genuinely different
structures — asymmetric grid, void-heavy, layered, bleed — rendered and judged
against matching mockups. This settles accent-versus-composition (§6). Half a
day, and it decides what Phase 1 is.

## Stage 3 — build vocabulary in the measured order

1. **Accent axis** — L1 delta fragments that *rebind existing role tokens*, L0
   gains the token, omitted axis emits nothing.
2. **Truncation** — most bounded and half-built: `TextRow(lines:)`, emit
   `Attrs.lines`; the Label widget already does `max_lines` + ellipsis.
3. **A general icon role** — `WeatherIcon` is the only icon constructor today,
   so an icons axis over it reaches 24% of cards, not the measured 52%.
4. **Typography** — a font role on `Attrs` (none exists), faces placed in
   `aichat/widgets/resources` (what `crate_resource` actually resolves to),
   preserving the deliberate CJK/arrow fallback. `font_pair` is Codex's single
   highest-value token.
5. **Surface, then density** — blocked on semantic roles: `Panel` and `Card`
   both lower to `l0_panel`, `Grid` synthesizes its own rows, and swipe-reveal
   recognition breaks if a row becomes a card. Also needs `border`,
   `bordercolor` and `elevation` emitted at all — they exist on the node and are
   read by the evaluator but never reach the widget DSL.
6. **Charts** — a real widget project; route chart-heavy designs to the webview
   card until it exists.

If the probe favours composition, a **COMPOSITION plane** (grid spans, void
regions, layering, crop/bleed, rhythm — kept outside the semantic card
vocabulary) inserts ahead of the theme axes.

## Stage 4 — re-measure, then repeat

After each shipped feature, re-run **cards only** against the same mockups: no
image generation, no HTML, directly comparable numbers.

---

## 3. The work, by layer

| layer | shipped this week | still to build |
|---|---|---|
| **L0** (what a card may say) | `TextEyebrow` | theme axes + parser + `ThemeSpec`; `TextRow(lines:)`; a general icon role; possibly a composition plane |
| **L1** (how roles are painted) | `weight_hero`, `hero_factor`, `l0_bar`/`l0_bar_rail`, `icon_mono`, `panel_inset`, `l0_scrim_top`, dark `l0_fill`, quieter hairline, `weight_semi`, `l0_eyebrow` | one delta fragment per axis value; `font_pair`/`leading`/`tracking`; real stroke + shadow params; alpha, blur, masks; ornament plane |
| **plumbing** | TempBar `flat_ink`/`rail_ink`, WeatherIcon `mono_ink`, `bg2`→`color_2`, Roboto-Thin at weight ≤250, hug-context text sizing | emit border/elevation; read `Attrs.lines`; font-family transport |
| **host** | — | override resolution; ThemeSpec in the render-cache key; kit assembly cached by spec |

## 4. Standing gates

- **Genericity query** before any token is built: how many cards, across how many
  domains, consume that role? `TextEyebrow` 86% — vocabulary. `l0_bar` 6%,
  weather-only — plumbing. Build the generalization instead.
- **Regression replay** over the existing corpora: did this move cards that never
  asked for it?
- **Eyeball review of three renders** per phase. The judge has ±1 variance and
  known biases; it ranks, it does not certify.

## 5. Scope

This is the **Makepad path**. Splash-OH depends on `splash-core` and
Splash-Android's shipping path states "No DSL is involved" — neither consumes
this kit today, so axes widen an existing divergence rather than breaking
parity. Track it deliberately; do not claim three-backend support.

## 6. The open question

My ordering is measurement-driven: accent, named in 92% of cards. Codex's is
theory-driven: *"the highest-value change is not another theme token, it is a
COMPOSITION layer"*, because schools are distinguished by structure before
palette. Our own corpus supports the theory uncomfortably well — every round-1
layout was a hero-plus-list variant.

They are not the same layer. Accent is a **theme** fix: cheap, safe, and it
improves every existing card retroactively. Composition is **card vocabulary**:
expensive, structural, and it does nothing for cards already generated. The
probe in Stage 2 decides which leads; it does not decide whether the other
happens.

## 7. Risks

1. **Silent axis collision** — two deltas writing one token, resolved by order.
   Mitigated by the ownership table and its test.
2. **Stale renders** — a theme change outside the cache key. Fixed in Phase 0.
3. **Profile creep** — `accent: .amber` is a mood word; a hex is not. The closed
   token set is the enforcement.
4. **Judge-driven design** — optimising against one judge. Hence the eyeball gate
   and the reminder that "named in 92%" is complaint frequency, not expected lift.
5. **Taste is not free** — unbounded palettes produce more variance, not more
   beauty. The taste in this pipeline comes from the mockups; improving the
   briefs stays the highest-leverage lever on quality.
