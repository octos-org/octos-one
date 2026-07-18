# Weather app

The DEFAULT for weather is an IMMERSIVE FULL-SCREEN iOS WEATHER CARD: a REAL photo
of the city fills the whole screen; the CURRENT conditions sit at the top, a
translucent 7-DAY FORECAST panel sits directly below them, then TWO FULL-WIDTH MAP
PANES stacked vertically — first a LIVE 卫星云图 (real satellite cloud imagery),
then a LIVE 空气质量图 (air-quality map) — each on its own row so the maps read
large, then a frosted 6-TILE DETAIL GRID (air quality, UV, sunrise, sunset,
humidity, wind) — like a refined iOS Weather app.

**YOU generate this card by ASSEMBLING the widget patterns** — there is no
exemplar to copy. Build it from THIS spec + `widgets/weather-icon.md`,
`widgets/containers.md`, `widgets/sys-helpers.md` (the image + data helpers),
and `widgets/interaction.md`. Reproduce EXACTLY this structure: a full-screen
Overlay (BLOCK: PHOTO-BACKDROP below — photo, dark scrim), then a Down column
= BLOCK: CURRENT, the 7-day forecast, the two map panes, then
BLOCK: DETAIL-TILES. `// name: weather-app` is the first line.

Composed "what should I DO in this weather" intents are NOT this card — they
route to `apps/weather-activity/app.md`, a composed app that reuses this
spec's `BLOCK: CURRENT`.

## STYLE CHOICES — pick the skin, keep the data

The weather card has **selectable visual styles** (same live `sys.weather` data,
different skin). Choose ONE per request; the data bindings are identical across
all of them. Selection order:

1. **Explicit style keyword in the request** (any language) wins:
   - `dark` / 深色 → **dark** — dark `#0f0f0f` cards, thin Roboto temps, multi-city
     list, rounded day chips. Reproduce `exemplars/style-dark.splash`.
   - `minimal` / `light` / `clean` / 简约 / 浅色 → **light** — light `#f2f2f7` bg,
     white cards, hairline dividers. Reproduce `exemplars/style-light.splash`.
   - `glass` / `vibrant` / `gradient` / 毛玻璃 → **glass** — blue→indigo gradient
     sky, frosted translucent cards, a feels/humidity/wind/UV stat grid + 7-day
     strip. Reproduce `exemplars/style-glass.splash`.
   - `photo` / `immersive` / 大图 → **immersive** — the full-screen photo card.
     Reproduce `exemplars/style-immersive.splash` (a lean photo hero + 7-day
     panel) OR the full `exemplars/weather-canonical.splash` (adds the satellite /
     air-quality map panes + detail grid) when the request wants the rich version.
2. **Default** (no style keyword): **immersive** for a single named city; **dark**
   for a bare "weather" with no city (its multi-city list reads better).

Whatever the style, **adapt the city name + real lat/lon** to the request and keep
EVERY temperature a `sys.weather(...)` call (the style files are hardcoded demos —
Shanghai/Tokyo/SF — you MUST swap in the requested city and its lat/lon). All
styles load the bundled Roboto weights via
`crate_resource("makepad_widgets:resources/Roboto-*.ttf")` and show whole-degree
temps (the helper rounds temperature paths automatically). Full catalog +
previews: `docs/weather-styles/README.md`.

## LIVE DATA — MANDATORY (never hardcode weather numbers)

Every weather/air number in this card MUST come from a live data helper — you do
NOT know the real weather, so you must NEVER type an invented number. Use
`sys.weather(LAT, LON, "path")` and `sys.airquality(LAT, LON, "path")` (see
`widgets/sys-helpers.md`) as the `text` of each value `Label`, concatenating the
unit string, e.g. `text: sys.weather(LAT, LON, "current.temperature_2m") + "°"`.
Pass the city's REAL decimal lat/lon (the SAME LAT, LON used by the map helpers).
A value shows "—" for a moment while it loads, then the card auto-refreshes with
the real reading. The ONLY things you choose yourself are labels, the photo query,
the `WeatherIcon`/emoji condition, and the color categories.

## BLOCK: PHOTO-BACKDROP (the weather app's visual identity — reusable)

The immersive frame every weather-family card sits in: a full-screen Overlay
whose FIRST child is a REAL city photo matching the current conditions
(`Image{ src: http_resource(sys.photo("<city> <scene/weather>")) fit:
ImageFit.CropToFill width: Fill height: Fill }`), a dark scrim
(`SolidView{ width: Fill height: Fill draw_bg.color: #00000066 }`) over it for
legibility, then the inner `flow: Down` content column. Composed apps that
reuse BLOCK: CURRENT MUST reproduce THIS backdrop too — the plain gradient
screen is NOT the weather look. Content sections over the photo sit on
translucent panels (`RoundedView` `#00000055`, border_radius 20) like the
forecast panel below.

## Background-Image rules

- The background Image MUST use `fit: ImageFit.CropToFill` (fills the whole box,
  cropping overflow — a true edge-to-edge photo). NEVER use Smallest/Biggest/
  Vertical/Horizontal on it: those size the photo to its own aspect and leave bare
  letterbox bands.
- The ROOT Overlay container and the Image MUST have NO `padding` and NO `margin` —
  an Overlay child's Fill height = parent height MINUS parent padding MINUS its own
  margin, so ANY inset there SHRINKS the photo and exposes bare background. Put ALL
  insets (the top status-bar clearance, side and bottom padding) ONLY on the inner
  `flow: Down` column, exactly as specified here. The inner column MUST use
  `padding: Inset{left: 22 top: 54 right: 22 bottom: 8}` — the `top: 54` clears the
  phone's status bar so the CITY NAME sits comfortably below it (NEVER use a small
  top like 6 — the city name ends up jammed under the status bar / clock).
- Photo: `sys.photo("<city> <scene/weather>")` matching the actual conditions.

## Structure, top to bottom

The two `### BLOCK:` headings below are NAMED REUSABLE BLOCKS: other app specs
(composed apps like `apps/weather-activity`) may reference these blocks by name
and must reproduce them per THIS spec — same content, same live bindings.

### BLOCK: CURRENT

(1) The current-conditions block, at the top:
- City (font 30).
- The hero temperature ALONE on its line (font 60, `margin: Inset{top: 6 bottom: 0}`
  so its tall glyphs are not clipped) — its text is LIVE:
  `text: sys.weather(LAT, LON, "current.temperature_2m") + "°"`.
- A `flow: Right` row (height 60, align y 0.5, spacing 10) holding an ANIMATED
  `WeatherIcon{ draw_bg.cond: <N> width: 60 height: 60 }` followed by the condition
  `Label` (font 20). `WeatherIcon` is a live shader-animated weather glyph (rays
  rotate, rain/snow falls, wind/fog drifts, lightning flashes); pick `draw_bg.cond`
  by CURRENT condition: 0 clear/sunny, 1 partly cloudy, 2 cloudy/overcast, 3
  rain/drizzle, 4 thunderstorm, 5 snow, 6 wind, 7 fog/haze/mist. (See
  `widgets/weather-icon.md`.)
- Then an `H:__°  L:__°  Feels __°` line (font 15, #ffffffcc), every number LIVE:
  `"H:" + sys.weather(LAT, LON, "daily.temperature_2m_max.0") + "°  L:" +
  sys.weather(LAT, LON, "daily.temperature_2m_min.0") + "°  Feels " +
  sys.weather(LAT, LON, "current.apparent_temperature") + "°"`.

**(2) 7-DAY FORECAST** — directly under the current block (this comes BEFORE the
detail grid). A translucent RoundedView (draw_bg.color #00000055, border_radius 20)
with ONE SolidView row per day, EACH ROW a FIXED `height: 40` (roomy iOS-style rows;
the fixed height still clips color-emoji line-box inflation so rows stay uniform):
day name width 92 (font 14), a weather EMOJI width 34 (☀️ sunny, ⛅ partly, ☁️
cloudy, 🌧️ rain, ⛈️ storm, ❄️ snow), a Filler, then lo° dim (#ffffff88) and hi°
white width 48, all font 14. Give SEVEN rows: Today, then the next six days by name.
The lo°/hi° of row N are LIVE: `sys.weather(LAT, LON, "daily.temperature_2m_min.N")`
and `sys.weather(LAT, LON, "daily.temperature_2m_max.N")` for N = 0 (Today) … 6.
(The day NAMES and EMOJI you choose; the two temps must be sys.weather calls.)

**(3) TWO FULL-WIDTH MAP PANES** — stacked vertically (NOT side by side — each pane
is its own row so the maps read large), each a `width: Fill` RoundedView
(draw_bg.color #000000aa, border_radius 16, flow: Down):
- The FIRST pane is the 卫星云图 — REAL satellite cloud imagery:
  `Image{ src: http_resource(sys.satellite(LAT, LON)) fit: ImageFit.CropToFill width: Fill height: 190 }`
  (sys.satellite(LAT, LON) takes the city's real lat/lon, SAME as the air map below)
  + a `卫星云图` caption (font 11, #ffffffcc).
- The SECOND pane is the LIVE 空气质量图 air-quality map — a `height: 190 flow:
  Overlay` View stacking
  `Image{ src: http_resource(sys.basemap(LAT, LON)) fit: ImageFit.CropToFill width: Fill height: 190 }`
  UNDER
  `Image{ src: http_resource(sys.airmap(LAT, LON)) fit: ImageFit.CropToFill width: Fill height: 190 }`
  (fixed height, NOT Fill — Fill inside an Overlay wrongly resolves to the whole
  card) — pass the CITY's real decimal LAT, LON (e.g. Tokyo 35.68, 139.65; both maps
  take the SAME lat/lon) — + a `空气质量图` caption (font 11, #ffffffcc). (See
  `widgets/sys-helpers.md`.)

### BLOCK: DETAIL-TILES

(4) The detail grid — below the map panes. A `flow: Down` View of THREE `flow: Right`
rows, each holding TWO equal frosted tiles (`width: Fill`). Every tile is a
RoundedView (draw_bg.color #ffffff1f, border_radius 18) stacking an UPPERCASE
caption (font 11, #ffffff99), a big value (font 20), and a sub-line (font 12,
#ffffffcc). The SIX tiles in order:
Every value here is LIVE (sys.airquality / sys.weather); only captions, sub-lines
and the color category are yours:
- AIR QUALITY — value = `sys.airquality(LAT, LON, "current.us_aqi")` (the AQI
  NUMBER); set its `draw_text.color` by category — Good #32d74b, Moderate #ffd60a,
  Unhealthy #ff9f0a, Very Unhealthy #ff453a — and put the category word in the sub.
- UV INDEX — `sys.weather(LAT, LON, "daily.uv_index_max.0")`; sub Low/Moderate/
  High/Very High.
- SUNRISE — `sys.weather(LAT, LON, "daily.sunrise.0")` (already "HH:MM"); sub `🌅 Dawn`.
- SUNSET — `sys.weather(LAT, LON, "daily.sunset.0")` (already "HH:MM"); sub `🌇 Dusk`.
- HUMIDITY — `sys.weather(LAT, LON, "current.relative_humidity_2m") + "%"`; sub free.
- WIND — `sys.weather(LAT, LON, "current.wind_speed_10m") + " km/h"`; sub free.

The WHOLE inner column is a TALL, VERTICALLY-SCROLLING page (~1500dp) — it does NOT
need to fit one screen; the user DRAGS to scroll down and reveal the forecast, the
maps row and the detail grid, so use comfortable, breathable spacing rather than
cramming everything in.

## Data shape it needs

- city
- temp (hero)
- H / L / feels
- 7 × (day name, weather emoji, lo°, hi°)
- aqi + category
- uv (0–11)
- sunrise (clock time)
- sunset (clock time)
- humidity (percent) + dew point
- wind (e.g. `8 mph`) + compass direction
- lat / lon (real decimal; both maps take the SAME lat/lon)

---

Widgets used: WeatherIcon, sys.photo, sys.satellite, sys.basemap, sys.airmap,
GradientYView, RoundedView, SolidView, Filler, Image, Label — see `widgets/`.
