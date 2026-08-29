# style-factory — the beauty measurement stack

An instrument for answering "did this change make cards look better?" with a
number instead of an opinion. Built 2026-08-24 → 2026-08-27.

**Read `FINDINGS-axes-selection.md` before proposing more theme/palette work.**
The short version is at the bottom of this file: more styles measured as *not*
helping, and the reason is a capability gap, not a vocabulary gap.

## Instruments

| file | what it does |
|---|---|
| `batch_styles.py` | the harness: mockup → HTML twin → Splash card → device render → judge. `--cards-only` skips image generation; `--retranslate` regenerates cards against a new vocabulary and judges them blind against the previous render. |
| `recipes.py` | the specimen sampler — 10 design schools with hard exclusions, 5 surface models, 4 dialects, media at 5% photography, `dsl_gap` capability labels, domains weather/news/stock/quake. |
| `cumulative_judge.py` | blind paired judging of two render generations over the same 100 specimens, order swapped per specimen. |
| `noise_floor.py` | re-judges the same pairs with the order **flipped** to measure how often the judge agrees with itself. This calibrates every paired number the stack produces. |

Running the full pipeline needs an image-generation API key and a connected
phone. `--cards-only` needs only the phone. `cumulative_judge.py` and
`noise_floor.py` need neither — they read screenshots already on disk.

## The judge is trustworthy; check it stays that way

Measured 2026-08-27 over 100 pairs: **87% self-agreement**, implied per-call
reliability ~93%, noise sd **~2.5 wins on N=100**. Flipping presentation order
moved the tally by 3 wins out of 100.

That is the number that makes everything else interpretable. A 61–39 sits ~4.4
sd out and is real; a 52–48 is nothing. Re-run `noise_floor.py` if the judge
model changes.

## Ledgers

`ledgers/` holds the raw result of every run that backs a claim in the FINDINGS
documents. `ledger.jsonl` is the most recent corpus run (borrowed-vocabulary /
`radius`). `ledger-round2-legacy.jsonl` is the older round-2 ledger that used to
sit at the top level of this directory.

`cumulative-CONTAMINATED.jsonl` is kept deliberately: it was produced by running
the judge concurrently with a corpus regeneration that was overwriting the very
files being judged. It is a worked example of a self-inflicted confound, not a
result.

## Corpus

`corpus/` — 967 generated `.card` files across activity, chart, city-picks,
convert, dashboard, nav, news, quake, stock, travel, weather, weather-activity,
weather-theme, youtube.

Two uses beyond archaeology:

1. **n-gram speculation corpus.** The serving corpus on the H100 is 5 cards,
   which is why NGRAM acceptance collapsed to a median of 2.25 accepted tokens
   of 32 drafted on novel content. Feeding this corpus is a free speed win.
2. **Training data** for draft-model work.

## What is NOT in git

`~/home/gh200-backup/beauty-dataset/styles/out/` — 253 MB of PNGs: 100 mockups
(188 MB), 100 card renders, 100 HTML renders, plus the preserved control
generations (`-card-noaxes`, `-card-axesv1`, `-card-prev`).

The mockups cost image-generation API calls to recreate, so they are **not**
cheaply regenerable. They live only in that backup directory. Anyone depending
on this stack should copy them somewhere durable.

## What was measured, so it is not re-measured

- **HTML twin 7.25 vs Splash card 2.53** over 100 specimens — a 4.72 gap.
- **The gap is capability, not palette.** `accent_hue` — the purest "more
  colours" capability, required by 80 of 100 specimens — measures **+0.15
  points**. Per-card theme axes measured **52/48** against no axes at all, and
  **44/43 (p = 1.000)** restricted to pairs where two independent judgments
  agreed.
- **School spread says the same thing.** Schools L0 can already serve score
  3.50–3.60 (swiss, japanese_ma: flat fills, hairlines, a weight ramp). Schools
  needing what it lacks score 1.88–2.15 (art_deco, bauhaus, de_stijl, memphis:
  stroke, texture, hard shadow, serif display). A 1.6-point spread no number of
  palettes closes.
- **Aggregate skew is not evidence of defaulting.** `radius: .none` at 88% read
  as a stuck axis until conditioned on school: the 8 sharp-cornered schools
  chose it 79/80, which is *correct*, and organic chose `.large` 5 of 9. Two
  experiments were spent on a defect that did not exist. Condition on the
  design-correct answer before calling a distribution a defect.

The queue that follows from this is in `PLAN-vocabulary-loop.md`: stroke/border
first (required by 53 of 100 specimens; it currently emits but paints nothing —
a real widget-layer bug), then texture (+0.56) and hard shadow (+0.64).
