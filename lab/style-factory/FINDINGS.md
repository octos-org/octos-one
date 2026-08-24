# The style factory — 120 specimens, mockup → HTML → Splash (2026-08-23)

4 domains (weather, news, stock, reading) × 10 genres × font/layout/accent/density
mutations. Every specimen: a gpt-image-2 mockup, an Opus-written HTML twin
rendered in octos-one's own webview, an Opus-written L0 card gated by
`l0validate` and rendered on the OnePlus 6T — each stage judged by Opus against
the mockup. Unattended, resume-safe, zero human steps after launch.

## Headline

| stage | fidelity to the mockup |
|---|---|
| HTML twin in the app's webview | **6.67** mean (median 7, n=115) |
| Splash L0 card on device | **2.76** mean (median 3, n=91) |
| paired delta | **3.82** |

93 of 120 cards validated; 91 rendered and judged; 5 harness errors.
The HTML mean says the *device* can show these designs. The card mean is the
renderer's vocabulary bill, and this run prices every line of it.

## The dominant finding: an L0 card cannot name a colour, and there are only four moods

The mockups asked for seven accents (amber 19, indigo 18, forest green 18,
vermilion 18, google-blue 17, magenta 14, electric cyan 12). The catalog offers
`THEMES = [dark, light, glass, photo]` and **no colour vocabulary at all**
(`color` does not appear in `ui-l0-constructors.toml`). The 120 generated cards
could only ever declare `light` (77), `dark` (24) or `photo` (15).

So **92% of judged cards were marked down on colour/accent** — structurally, not
stylistically. Style diversity is currently capped at four looks no matter what a
card, a generator or a fine-tuned model wants.

The fix is NOT to let cards write hex — that would collapse §1.1's "a card names
roles, the theme owns presentation". It is an **accent axis on the mood**:
`theme light accent: .amber`, a closed token set the palette maps to
`l0_accent`/`l0_accent_ink`, exactly like `theme <mood>` today. Four moods × N
accents unlocks the space without a card ever naming a colour.

## The priced bill (judge's own words, 91 cards)

| missing vocabulary | named in | share | mean score when named |
|---|---|---|---|
| colour / accent | 84 | 92% | 2.8 |
| serif / display face | 56 | 62% | 2.8 |
| icon style (line-art, avatars) | 47 | 52% | 3.0 |
| missing text / labels | 32 | 35% | 2.8 |
| card & panel chrome (border, elevation, texture, rules) | 31 | 34% | 2.8 |
| truncation / ellipsis | 28 | 31% | 3.0 |
| letterspacing / caps | 21 | 23% | 3.0 |
| hero scale / weight | 15 | 16% | 3.0 |
| chart / sparkline | 14 | 15% | 2.6 |
| gradient fill | 12 | 13% | 2.3 |
| row density / spacing | 7 | 8% | 3.1 |

Ranked by frequency × cost, the roadmap is: **accent axis → a serif/display font
axis → an icon style axis (mono/line-art, already half-built) → panel chrome
(border, elevation, texture) → a one-line/ellipsis token → chart primitives.**
Gradient fill is rarer but the most expensive when asked for (2.3).

## Genre and domain

Cards, excluding `reading` (see caveat):

    cinematic-photo 3.62 · editorial-serif 3.40 · dark-terminal 3.00 ·
    neon-night 3.00 · newspaper 2.90 · material-light 2.88 · dense-feed 2.88 ·
    pastel-soft 2.86 · brutalist 2.80 · glass-vibrant 2.33

`cinematic-photo` leads — the one lane this week's vocabulary (hairline hero,
gradient scrim, mono icons, TextEyebrow, dark inset pane) was built for, and the
only genre producing an exemplar (`s014-weather-cinematic-photo`, 6/7). That is
the pipeline's own thesis confirmed on 120 samples: **ship vocabulary, the
scores move.** `glass-vibrant` is last because it needs the two features L0 most
lacks — saturated gradients and tinted translucency.

By domain: weather 3.11, news 3.00, stock 2.90, reading 1.95.

## Caveats, stated plainly

- **`reading` is not a fair measurement.** `sys.reading` answers the device's own
  saved-article list, which is empty on this handset, so those 22 cards rendered
  with no rows. A recipe-design error, not a renderer gap. Re-run with a seeded
  reading list before quoting that 1.95.
- **Single-judge, ±1 variance**, with the known length/content biases. The
  per-genre gaps here (3.6 vs 2.3) are larger than that noise; individual
  specimen scores are not.
- **Data-path history.** The first ~65 card renders were scored on loading
  skeletons: the phone's hotel WiFi died, then adb reverse sockets rotted, then —
  the real one — `makepad.OCTOS_PROXY` was never set on the seeded intent, so
  card fetches (Java-side `HttpURLConnection`) had no route while the WebView
  stage happened to work. All three were fixed and every affected specimen was
  re-queued; the numbers above are from renders with live data on screen.
- **5 harness errors** (regex/parse edge cases in the last reading specimens),
  not model or renderer failures.

## What this run produced

- `ledger.jsonl` — 120 records: recipe, per-stage scores, judge notes.
- `out/` — 120 mockups, 115 HTML twins, 93 valid L0 cards, ~200 device renders:
  the first style-diverse (mockup, HTML, card) corpus.
- This file: the priced renderer roadmap.
