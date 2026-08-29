# Composition probe — answered early, and not as designed (2026-08-24)

**Question:** should Phase 1 be the accent axis (my measurement-driven ordering)
or a COMPOSITION plane (Codex's theory-driven ordering)?

**Method planned:** hand-write cards with genuinely different compositions,
render, judge, compare deltas.

**What actually happened:** the probe collapsed at step one. Inspecting
mockup-versus-card pairs for three representative specimens
(`s001-weather-editorial-serif`, `s009-weather-dark-terminal`,
`s010-weather-pastel-soft`) shows that **the mockups and the cards already share
their composition.** Every one is city label → large temperature → condition →
forecast list. There was nothing compositional to close.

That is a finding about the *instrument*, and it matches what the recipe audit
already showed: round 1's layout axis had four values, all "hero + list"
variants, so the image model produced one composition regardless. **Round 1
cannot answer the composition question** — the variance was never there.

## What the same comparisons DO show, consistently across all three

The gaps are entirely theme-token gaps:

| gap | mockup | card |
|---|---|---|
| hero scale | fills the top third | small; `hero_factor` is 1 for light/dark, only `photo` got 1.6 |
| accent | vivid blue / red / amber on highs and icons | monochrome black or white |
| display face | condensed or light display type | Roboto regular/bold |
| icon style | line-art, tinted with the accent | filled emoji-style multicolour |
| row density | tight rows on hairlines | tall padded rows |
| truncation | one line per row | condition text wraps mid-word — "Thund erstorm", "Partly Cloud y" |

## Verdict

**For the compositions our generator actually produces, theme tokens are the
binding constraint.** Phase 1 stays as planned: accent, and with it the hero
scale and density knobs that are already parameterized for `photo` and simply
never extended to the other three moods.

This does **not** refute Codex's argument — schools may well be distinguished by
structure before palette. It says the claim is untested, because no mockup in
the corpus varied composition. The proper test needs taxonomy-based recipes that
actually request asymmetric grids, void-heavy fields and layered compositions,
which is Stage 1 work regardless.

**Re-run this probe after the recipe rewrite**, using mockups whose composition
genuinely differs, and compare a hand-matched card against the generated one.

## Bonus findings, both cheap

- **Truncation is uglier than the score suggests.** Mid-word wrapping is visible
  in two of three cards. `TextRow(lines:)` was already Phase 2; this is an
  argument for keeping it early.
- **The generator under-uses what exists.** `s001`'s card contains no
  `WeatherIcon` at all, and its hero renders "28" with no degree — vocabulary
  present in the profile but unused. Some of the gap is prompt quality, not
  renderer capability, and prompt fixes are free.
