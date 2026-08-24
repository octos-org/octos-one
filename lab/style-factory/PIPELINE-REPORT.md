# pipeline-v2 — first specimen through the mockup → HTML → Splash rail (2026-08-22)

## The design (user's spec)
One gpt-image-2 generation produces BOTH the clean background photo and the
mockup UI over that same photo (split panel), so the background can be cropped
out with zero divergence and reused as the literal background of every later
stage. Typography locked to Roboto in the mockup prompt (Thin hero), which is
Android's system font — so webview and Splash render the same family.

## Artifacts
- `paired.png` — the one-shot generation; `bg.png` + `mockup.png` — its halves
- `weather.html` — stage 1: live open-meteo data, bg.png as background, Roboto,
  hairline hero; rendered IN octos-one's own webview via the new `seed.html`
  file trigger (SEED_HTML, main.rs — an intent-extra bridge only forwards
  known names, so the trigger is a pushed file, consumed by rename)
- `html_render3.png` — stage 1 on-device render (2 CSS iterations)
- `../translated/weather-tokyo.card` — stage 2: the L0 card (photo mood)
- `splash_render.png` — stage 2 on-device render

## Scores (Opus, reference-based vs mockup.png)
| stage | fidelity | overall |
|---|---|---|
| HTML in webview | **8** | **8** |
| Splash L0 | **6** | **6** |

**Measured vocabulary loss: −2 points**, itemized by the judge and mapping 1:1
onto the translation gap list: rainbow TempBar with no rail (gap 7), emoji-style
weather icons vs line art (gap 8), hero ramp ceiling (gap 2), no letterspaced
uppercase (gap 4), heavy rows/dividers (gaps 10-11). That is the renderer
roadmap, now with a price tag per feature.

## Caveats
- Background pinning in the Splash stage failed: the app's photo cache answered
  before the seeded `scene` URL, so the backdrop differs from the target (judge
  was told to ignore photo choice). Fix for next run: clear the app's photo
  cache, or point `state.mood`'s query at the tunnel-served crop.
- The condition word was absent in the Splash render (proxy was off for the
  pinning attempt; `sys.weatherword` had no route) — cost fidelity unfairly.
- webview does not render `backdrop-filter` (frosted = flat there too), so the
  "no blur primitive" gap (5) is shared by both stages on this device.
- Single-judge variance is about ±1; the 8-vs-6 gap is larger than the noise.

## Parity round (2026-08-22): Splash 4 → 6 vs the HTML twin's 8

Shipped (aichat 70171595, splash-makepad 0ee6366, octos-one 548a96f):
mono silhouette WeatherIcons (premultiplied ink), flat indigo TempBar on a
faint rail, hero top-step scale (hairline 69pt), dark inset floating pane,
quieter hairlines/weights — all mood-owned, all default-off, so every
photo-mood card in the corpus inherits them and no dark-mood card changes.

Found on the way: a photo-mood icon constructor evaluated to nil (every
icon silently absent — the drop to 4/10), and this VM cannot mutate a fresh
dict's w/h fields; compose around constructors like l0_tinted instead.

Remaining, priced by the stabilized judge list: letterspaced caps (no
tracking primitive), the 'Rain' condition word (live-data quirk, not
vocabulary), true blur, a per-slot weight axis, and the card's own tall
mid-void. Renders: splash_render{2..6}.png, splash_final.png.

## Gap-closure round 2 (2026-08-22): Splash reaches 7/7 vs the HTML twin's 8/8

Arc: 4 -> 5 -> 6 -> 7. Closed this round (splash l0-visualisations,
splash-makepad dd7c19d, octos-one ce672b2):
- Fill-inside-Fit, THIRD sighting: a fit row's text children defaulted to
  Fill = zero width — the condition word and both feels captions were
  invisible on every card. Emitter now tracks hug context (RAII guard);
  fixed for every container at once.
- TextEyebrow: a real L0 role (catalog+TOML+kit+emitter). The card writes
  TextEyebrow(text: city); the theme uppercases and thin-space tracks the
  literal — T O K Y O. Live values degrade gracefully.
- Photo-mood value weight 400.

Remaining, judged: taste nits inside single-judge variance (bar length and
rounding, icon stroke weight, row spacing), the judge's objection to card
content the mockup lacks (the feels row — arguably the card is right), and
ONE unimplemented primitive: backdrop blur, which needs render-to-texture
in the makepad pipeline. Render: splash_v8.png.

## Specimen 2 — news (2026-08-22): the pipeline generalizes

Same rail, second domain, ~40 minutes end to end, no new machinery:
paired mockup (newsroom bg + UI, Roboto locked) -> HTML twin in the app
webview (live Algolia front page, direct — no tunnel needed) -> L0 card
(news-photo.card, validator-clean on the 2nd try, rendered on the 1st).

Scores vs the news mockup: **HTML 8/8, Splash 6/7** — mirroring the
weather specimen's 8/8 vs 7/7. The stage delta is stable across domains.

What last round's vocabulary bought for free: TextEyebrow reused
unchanged (TOP STORIES), the hug fix meant every meta row was visible on
the first render, pane/weights inherited. Background pinning SOLVED:
fresh mood phrase + proxy off + seeded scene URL through adb-reverse —
the Splash render shows the exact mockup crop. Both stages happened to
fetch the same live lead story, making them directly comparable.

New vocabulary priced by this specimen's judge list: a styled rank
numeral (large/accent — ranks are role-fixed TextRow today), a one-line/
ellipsis text token (titles wrap where the mockup truncates), panel
border+elevation, dot separators (card copy nit), and the hero weight on
long live titles reading bold not light (diagnose next round).
