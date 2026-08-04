# Weather app

The DEFAULT for weather is an IMMERSIVE FULL-SCREEN iOS WEATHER CARD: a REAL photo
of the city fills the whole screen; the CURRENT conditions sit at the top, a
translucent 7-DAY FORECAST panel sits directly below them, then TWO FULL-WIDTH MAP
PANES stacked vertically — first a LIVE satellite cloud-imagery pane (卫星云图),
then a LIVE air-quality contour map (空气质量图) — each on its own row so the maps
read large, then a SUN & MOON panel (the sun's arc from sunrise to sunset, and the
current moon phase), then a frosted 4-TILE DETAIL GRID (air quality, UV, humidity,
wind) — like a refined iOS Weather app. All USER-FACING text is in ONE language,
chosen by the request — see the LANGUAGE rule below.

**YOU generate this card by ASSEMBLING the widget patterns** — there is no
exemplar to copy. Build it from THIS spec + `widgets/weather-icon.md`,
`widgets/containers.md`, `widgets/sys-helpers.md` (the image + data helpers),
and `widgets/interaction.md`. Reproduce EXACTLY this structure: a full-screen
Overlay (BLOCK: PHOTO-BACKDROP below — photo, dark scrim), then a Down column
= BLOCK: CURRENT, the 7-day forecast, the two map panes, the SUN & MOON panel,
then BLOCK: DETAIL-TILES. `// name: weather-app` is the first line.

**Emit EXACTLY ONE top-level element** — the root Overlay, and nothing beside it.
Sibling top-level elements are laid out SIDE BY SIDE, so an extra
`SolidView{ width: Fill height: N draw_bg.color: #0a0e14 }` added "for the
background" does not sit behind the card; it takes half the screen's width and
squeezes the whole card into the other half. The dark base colour belongs on the
root Overlay ITSELF, and the photo already covers it.

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

Whatever the style, **adapt the city name to the request and resolve its
coordinates with `sys.geocodenum` — never typed digits** (see LIVE DATA below), and keep
EVERY temperature a `sys.weather(...)` call (the style files are hardcoded demos —
Shanghai/Tokyo/SF — you MUST swap in the requested city and its lat/lon). All
styles load the bundled Roboto weights via
`crate_resource("makepad_widgets:resources/Roboto-*.ttf")` and show whole-degree
temps (`sys.weather` rounds temperature paths automatically — do NOT round in the
card). Full catalog + previews: `docs/weather-styles/README.md`.

## LANGUAGE — pick ONE for the whole card, NEVER mix

Every word on the card must be in the SAME language, and that language is the one
the REQUEST was written in:

- an English request ("weather in shanghai") → the card is **entirely English**
- a Chinese request ("上海天气", "上海的天气怎么样") → the card is **entirely Chinese**

A card reading `Shanghai / Cloudy / AIR QUALITY` next to `卫星云图 / 日出 / 100% 照亮`
looks unfinished. That includes the CITY NAME (Shanghai vs 上海), the condition
word, the day names, every tile caption and every sub-line — not just the headings.

| element | English | 中文 |
|---|---|---|
| city | Shanghai | 上海 |
| condition | Partly Cloudy / Cloudy / Rain | 局部多云 / 多云 / 雨 |
| day names | Today, Wed, Thu … | 今天、周三、周四… |
| satellite pane caption | Satellite | 卫星云图 |
| air-quality pane caption | Air Quality | 空气质量图 |
| sun panel caption | Sunrise / Sunset | 日出 / 日落 |
| moon phase name | `sys.moonphase("name")` | `sys.moonphase("name_zh")` |
| moon illumination | `… + "% illuminated"` | `… + "% 照亮"` |
| detail tile captions | AIR QUALITY, UV INDEX, HUMIDITY, WIND | 空气质量、紫外线、湿度、风速 |
| AQI categories | Good / Moderate / Unhealthy | 优 / 良 / 不健康 |
| UV categories | Low / Moderate / High / Very High | 低 / 中等 / 高 / 很高 |

`sys.moonphase("name")` returns English; use `"name_zh"` for the 八相 names
(新月 / 蛾眉月 / 上弦月 / 盈凸月 / 满月 / 亏凸月 / 下弦月 / 残月) on a Chinese card.
Numbers, `°`, `%`, `km/h` and the ↑ ↓ ≈ glyphs are language-neutral and stay as-is.

## FONT WEIGHTS — how a weather card gets its 秀气

Weight, not size, is what makes this card look refined. `draw_text.text_style.font_size: N`
on its own leaves the DEFAULT weight and the result reads heavy and generic. Set
a full `TextStyle` with an explicit `font_family` on every line that matters:

```
draw_text.text_style: TextStyle{
    font_family: FontFamily{
        latin   := FontMember{ res: crate_resource("makepad_widgets:resources/Roboto-Thin.ttf") asc: 0.0 desc: 0.0 }
        chinese := FontMember{ res: crate_resource("makepad_widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0 }
    }
    font_size: 76
}
```

Bundled weights: `Roboto-Thin` (hero temperature ONLY), `Roboto-Light` (city,
condition, stat lines, forecast rows), `Roboto-Regular`, `Roboto-Medium`,
`Roboto-Bold`. Bigger + thinner beats smaller + heavier every time.

**ALWAYS include the `chinese` member.** Writing an explicit `font_family`
REPLACES the whole default chain, which had Chinese and emoji members in it.
Roboto contains NO CJK glyphs, so a Chinese card whose hero declares only a
`latin` member renders 上海 and 多云 as empty TOFU BOXES (▯▯) — the card looks
broken at its largest text. `LXGWWenKaiRegular.ttf` covers CJK at every weight
above; there is no Thin/Light CJK face, and it is not missed at these sizes.

Carry the member even on an English card: it costs nothing when unused, and it is
the difference between a city name rendering and not.

**Do NOT set `font_family` on a label carrying colour emoji** (the ☀️ ⛅ 🌧️ in
forecast rows). Overriding the family drops the `NotoColorEmoji` member the same
way, and the emoji turns into tofu. Leave those labels on the default chain.

**Glyph fallback.** `FontFamily` is an ORDERED chain: any glyph missing from one
member is looked up in the next. Roboto has NO arrows (`↑` `↓`) — a line using
them MUST add a NotoSans member or they render as tofu boxes:

```
font_family: FontFamily{
    latin   := FontMember{ res: crate_resource("makepad_widgets:resources/Roboto-Light.ttf") asc: 0.0 desc: 0.0 }
    sym     := FontMember{ res: crate_resource("makepad_widgets:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0 }
    chinese := FontMember{ res: crate_resource("makepad_widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0 }
}
```

The member NAMES are arbitrary labels; only the order matters. `°` and `≈` ARE in
Roboto and need no fallback. Colour emoji (☀️ ⛅ 🌧️) resolve through
`NotoColorEmoji` but inflate the line box, which is why forecast rows pin a fixed
`height`.

## LIVE DATA — MANDATORY (never hardcode weather numbers)

Every weather/air number in this card MUST come from a live data helper — you do
NOT know the real weather, so you must NEVER type an invented number. Use
`sys.weather(LAT, LON, "path")` and `sys.airquality(LAT, LON, "path")` (see
`widgets/sys-helpers.md`) as the `text` of each value `Label`, concatenating the
unit string, e.g. `text: sys.weather(LAT, LON, "current.temperature_2m") + "°"`.
A value shows "—" for a moment while it loads, then the card auto-refreshes with
the real reading. The ONLY things you choose yourself are labels, the photo query,
the `WeatherIcon`/emoji condition, and the color categories.

### NEVER type a coordinate — look it up

**LAT and LON above are NOT numbers you write.** A coordinate you recall is an
invented number exactly like a temperature you recall, and it is wrong in the same
way: plausible for a famous city, fabricated for anywhere else, and silently
pointing at the wrong place when a name is ambiguous. Everywhere this spec says
`LAT, LON`, emit:

```
sys.geocodenum("<place>", "lat"),  sys.geocodenum("<place>", "lon")
```

so a call reads:

```
sys.weather(sys.geocodenum("Shanghai", "lat"), sys.geocodenum("Shanghai", "lon"),
            "current.temperature_2m")
```

Use the SAME place string in every call on the card — every one shares a single
cached lookup, so the cost is one request no matter how many times it appears.
`sys.geocodenum` also anchors `sys.satellite`, `sys.basemap`, `sys.aqigrid`,
`sys.daylight`, `sys.weekmin` and `sys.weekmax`. Use `sys.geocode("<place>","name")`
when you want the resolved place NAME as display text, and `"country"`, `"admin1"`,
`"timezone"` or `"population"` for the other facts.

**Do NOT hoist the lookup into a top-level `let`.** A card's top-level `let`
bindings are evaluated ONCE at build time, before any fetch has resolved, so
`let LAT = sys.geocodenum(…)` freezes at the `-9999` loading sentinel and never
updates. Call it inline at each use site instead; that is what makes it re-resolve
on the redraw when the lookup lands.

YOU still decide WHICH place the request means — resolving "nvidia" to Santa Clara,
or 上海 to Shanghai, is world knowledge and it is your job. What you must not do is
turn that name into digits.

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

(1) The current-conditions block, at the top. The whole block is CENTRED: wrap
its lines in a `View{ width: Fill height: Fit flow: Down align: Align{x: 0.5} }`
and give the condition row `align: Align{x: 0.5 y: 0.5}` (a left-aligned hero
reads as a draft). Use `Align{x: …}` — there is no `alignx` property.
The hero must read DELICATE (秀气), not heavy. Weight carries that, not size: set
an explicit `font_family` on every line of this block — a bare
`draw_text.text_style.font_size` leaves the default weight and the block renders
chunky. Use `Roboto-Thin.ttf` for the big temperature and `Roboto-Light.ttf` for
the text around it (both bundled; see the FONT WEIGHTS section below).

- City — Roboto-Light, font 26, `#ffffffe6`.
- The hero temperature ALONE on its line — **Roboto-Thin**, font 76,
  `margin: Inset{top: 2 bottom: 0}` so its tall glyphs are not clipped. Thin is
  what makes a 76pt number elegant instead of shouty; at the default weight this
  line alone ruins the block. Its text is LIVE:
  `text: sys.weather(LAT, LON, "current.temperature_2m") + "°"`.
- A `flow: Right` row (height 52, align x 0.5 y 0.5, spacing 8) holding an ANIMATED
  `WeatherIcon{ draw_bg.cond: <N> width: 46 height: 46 }` followed by the condition
  `Label` (Roboto-Light, font 17, `#ffffffe6`). `WeatherIcon` is a live
  shader-animated weather glyph (rays rotate, rain/snow falls, wind/fog drifts,
  lightning flashes); pick `draw_bg.cond` by CURRENT condition: 0 clear/sunny,
  1 partly cloudy, 2 cloudy/overcast, 3 rain/drizzle, 4 thunderstorm, 5 snow,
  6 wind, 7 fog/haze/mist. (See `widgets/weather-icon.md`.)
- Then the high/low/feels line — **ICON GLYPHS, never the words "H:", "L:" or
  "Feels"** (spelled-out labels read as a data table, not a weather app). Use
  `↑` high, `↓` low, `≈` feels-like, THREE spaces between groups. Roboto-Light,
  font 14, `#ffffffb3`, every number LIVE:
  `"↑" + sys.weather(LAT, LON, "daily.temperature_2m_max.0") + "°   ↓" +
  sys.weather(LAT, LON, "daily.temperature_2m_min.0") + "°   ≈" +
  sys.weather(LAT, LON, "current.apparent_temperature") + "°"`.

  `↑` and `↓` are NOT in Roboto — this line MUST declare a NotoSans fallback
  member or they render as tofu boxes. See FONT WEIGHTS below for the exact form.

**(2) 7-DAY FORECAST** — directly under the current block (this comes BEFORE the
detail grid). A translucent RoundedView (draw_bg.color #00000055, border_radius 20)
with ONE SolidView row per day, EACH ROW a FIXED `height: 40` (roomy iOS-style rows;
the fixed height still clips color-emoji line-box inflation so rows stay uniform):
day name width 92 (font 14), a weather EMOJI width 34 (☀️ sunny, ⛅ partly, ☁️
cloudy, 🌧️ rain, ⛈️ storm, ❄️ snow), a Filler, then lo° dim (#ffffff88) and hi°
white width 48, all font 14. Give SEVEN rows: Today, then the next six days by name.
The lo°/hi° of row N are LIVE: `sys.weather(LAT, LON, "daily.temperature_2m_min.N")`
and `sys.weather(LAT, LON, "daily.temperature_2m_max.N")` for N = 0 (Today) … 6.
**The day NAME is a HELPER CALL, never a literal string.** Row N's label is
`sys.dayname(LAT, LON, N, "en")` — or `"zh"` per the LANGUAGE rule — which yields
`Today`, `Thu`, `Fri`, … (`今天`, `周四`, …). You do NOT reliably know today's date,
and a wrong weekday is invisible: it looks exactly like a right one. Typing them
out produced `Today, Wed, Thu, …` on a Wednesday — today repeated as tomorrow, so
every row after the first was mislabelled. Cards are also SAVED and re-served, so a
literal is stale the next morning even when it was right when generated.
`sys.dayname` reads the date from the same cached forecast as the temperatures, so
the labels belong to the FORECAST'S place, not to wherever the phone is.

(The EMOJI you choose; the day name and both temps must be helper calls.)

BETWEEN the lo° and hi° labels put a `TempBar` — the spectrum range bar. It fills
the gap so the cool end sits against the low reading and the warm end against the
high one. Drop the `Filler` when you use it; the bar takes the slack. Pass RAW
degrees — the widget normalises against `wmin`/`wmax` itself:

```
Label{ width: 46 align: Align{x: 1.0} padding: Inset{right: 5}
       text: sys.weather(LAT,LON,"daily.temperature_2m_min.N") + "°"
       draw_text.color: #ffffff88 draw_text.text_style.font_size: 14 }
TempBar{ width: Fill height: 8 margin: Inset{left: 10 right: 10}
         draw_bg.tlo:  sys.weathernum(LAT, LON, "daily.temperature_2m_min.N")
         draw_bg.thi:  sys.weathernum(LAT, LON, "daily.temperature_2m_max.N")
         draw_bg.wmin: sys.weekmin(LAT, LON)
         draw_bg.wmax: sys.weekmax(LAT, LON) }
Label{ width: 46 align: Align{x: 0.0} padding: Inset{left: 5}
       text: sys.weather(LAT,LON,"daily.temperature_2m_max.N") + "°"
       draw_text.color: #ffffff draw_text.text_style.font_size: 14 }
```

Every part of those two labels matters for CENTRING the bar, and none of it is
optional:

- **`align`** — each label is a FIXED 46-wide box holding a ~30-wide number, so a
  left-aligned lo label strands ~16px of empty box between its digits and the bar
  while the hi label strands none. The bar is then geometrically centred in its own
  slot but visibly off to the RIGHT of the gap between the two numbers.
  `Align{x: 1.0}` pushes the lo number right against the bar, `Align{x: 0.0}`
  keeps the hi number left against it. (`Align{x: …}`, never `alignx` — see
  BLOCK: CURRENT.)
- **`padding`** — a right-aligned label sets its text flush to the box edge and the
  `°` then overhangs the clip and is CUT OFF, rendering as `29ᶜ`. Widening the box
  does NOT help: right-alignment moves the text with the edge. 5px of padding on
  the inner side of each label pulls the digits back inside the clip AND makes the
  two gaps equal.

The bar itself is a hairline whatever `height` you give it — the widget caps the
drawn track at 5px and centres it in the box — so `height` only sets the row's
rhythm and cannot make the bar thick.

Use `sys.weathernum` (NOT `sys.weather`) for tlo/thi — the string form does not
coerce and the bar collapses.

**NEVER type wmin/wmax as literal numbers.** They are the week's actual lowest low
and highest high, and you CANNOT know them — every temperature on this card is a
live fetch you do not see. Guessing produces something like `wmin: 10 wmax: 35`
for a 27–39° week, which clamps every high to the red end and crushes the whole
week into the top of the scale. `sys.weekmin` / `sys.weekmax` read the real range
off the same cached forecast, at no extra request.

**(3) TWO FULL-WIDTH MAP PANES** — stacked vertically (NOT side by side — each pane
is its own row so the maps read large), each a `width: Fill` RoundedView
(draw_bg.color #000000aa, border_radius 16, flow: Down):
- The FIRST pane is the 卫星云图 — REAL satellite cloud imagery:
  `Image{ src: http_resource(sys.satellite(LAT, LON)) fit: ImageFit.CropToFill width: Fill height: 190 }`
  (sys.satellite(LAT, LON) takes the city's real lat/lon, SAME as the air map below)
  + a caption (font 11, #ffffffcc) — `Satellite` or `卫星云图` per the LANGUAGE rule.
- ZOOM (optional third arg on all three map helpers, default 8): if the request
  asks to zoom the maps ("zoom in", "close-up", "放大" → 10; "wide"/"zoom out"
  → 6), pass it to `sys.satellite(LAT, LON, Z)` AND to BOTH air-map layers
  `sys.basemap(LAT, LON, Z)` / `sys.airmap(LAT, LON, Z)` — the two air-map
  layers must share the SAME Z or the overlay misaligns. Otherwise omit it.
- The SECOND pane is the LIVE 空气质量图 air-quality map — a `height: 190 flow:
  Overlay` View stacking
  `Image{ src: http_resource(sys.basemap(LAT, LON)) fit: ImageFit.CropToFill width: Fill height: 190 }`
  UNDER an **`AqiContour`** (fixed height, NOT Fill — Fill inside an Overlay wrongly
  resolves to the whole card) + a caption (font 11, #ffffffcc) — `Air Quality` or
  `空气质量图` per the LANGUAGE rule.

  `AqiContour` draws US-AQI as a FILLED CONTOUR FIELD in the EPA category colours
  with isolines at each band boundary — an air-quality tile only marks discrete
  monitoring stations, and over most cities it is very nearly empty. The contour
  is translucent, so the basemap beneath still gives geographic context.

  It takes a 4×4 grid of readings as the sixteen uniforms `a0`..`a15`, ROW-MAJOR
  with the NORTH row first. Emit ALL SIXTEEN — every call shares one cached fetch,
  so the cost is a single request. `span` is the width in degrees (use `1.6` for a
  city); pass the SAME LAT, LON as the maps:

```
AqiContour{ width: Fill height: 190
    draw_bg.a0:  sys.aqigrid(LAT, LON, 1.6, 0)   draw_bg.a1:  sys.aqigrid(LAT, LON, 1.6, 1)
    draw_bg.a2:  sys.aqigrid(LAT, LON, 1.6, 2)   draw_bg.a3:  sys.aqigrid(LAT, LON, 1.6, 3)
    draw_bg.a4:  sys.aqigrid(LAT, LON, 1.6, 4)   draw_bg.a5:  sys.aqigrid(LAT, LON, 1.6, 5)
    draw_bg.a6:  sys.aqigrid(LAT, LON, 1.6, 6)   draw_bg.a7:  sys.aqigrid(LAT, LON, 1.6, 7)
    draw_bg.a8:  sys.aqigrid(LAT, LON, 1.6, 8)   draw_bg.a9:  sys.aqigrid(LAT, LON, 1.6, 9)
    draw_bg.a10: sys.aqigrid(LAT, LON, 1.6, 10)  draw_bg.a11: sys.aqigrid(LAT, LON, 1.6, 11)
    draw_bg.a12: sys.aqigrid(LAT, LON, 1.6, 12)  draw_bg.a13: sys.aqigrid(LAT, LON, 1.6, 13)
    draw_bg.a14: sys.aqigrid(LAT, LON, 1.6, 14)  draw_bg.a15: sys.aqigrid(LAT, LON, 1.6, 15)
}
```

  (See `widgets/sys-helpers.md`.)

**(3b) 日出日落 + 月相 — the SUN & MOON panel.** Directly under the map panes, a
`width: Fill height: Fit` RoundedView (draw_bg.color #00000055, border_radius 20,
padding 16, flow: Down, spacing 10) holding TWO parts:

- **The sun's path** — a `SunArc{ width: Fill height: 96 draw_bg.progress:
  sys.daylight(LAT, LON) }`, with a `flow: Right` row beneath it carrying the two
  times at the ends: sunrise `Label` (`sys.weather(LAT, LON, "daily.sunrise.0")`,
  already "HH:MM") then a `Filler` then sunset (`daily.sunset.0`), both
  Roboto-Light font 13 `#ffffffb3`, and a caption (font 11, `#ffffff99`) —
  `Sunrise / Sunset` or `日出 / 日落` per the LANGUAGE rule.
  `SunArc` draws a hairline arc from sunrise to sunset with the sun
  riding it at the CURRENT time; `sys.daylight` returns 0 at sunrise and 1 at
  sunset, and the widget dims the sun outside that range for night.
  This REPLACES the old SUNRISE and SUNSET number tiles — two blunt clock
  readings say far less than one curve showing where in the day you are.

- **The moon phase (月相)** — a `flow: Right` row (align y 0.5, spacing 14) holding
  a `MoonPhase{ width: 72 height: 72 draw_bg.phase: sys.moonnum("phase") }` beside
  a `flow: Down` column with the phase NAME (Roboto-Light font 16, `#ffffffe6`)
  over an illumination line (font 12, `#ffffff99`). BOTH follow the LANGUAGE rule:
  `sys.moonphase("name") … + "% illuminated"` on an English card,
  `sys.moonphase("name_zh") … + "% 照亮"` on a Chinese one.
  `MoonPhase` renders the real lit fraction with a correct elliptical terminator.
  Use `sys.moonnum("phase")` for the UNIFORM — `draw_bg.phase` needs a number, and
  the string form of `sys.moonphase` silently reads as 0 (a permanent new moon).
  It is computed from the clock, so it never shows a "—" placeholder.

### BLOCK: DETAIL-TILES

(4) The detail grid — below the SUN & MOON panel. A `flow: Down` View of TWO
`flow: Right` rows, each holding TWO equal frosted tiles (`width: Fill`). Every
tile is a RoundedView (draw_bg.color #ffffff1f, border_radius 18) stacking an
UPPERCASE caption (font 11, #ffffff99), a big value (Roboto-Light font 20), and a
sub-line (font 12, #ffffffcc). The FOUR tiles in order:
Every value here is LIVE (sys.airquality / sys.weather); only captions, sub-lines
and the color category are yours:
- AIR QUALITY — value = `sys.airquality(LAT, LON, "current.us_aqi")` (the AQI
  NUMBER); set its `draw_text.color` by category — Good #32d74b, Moderate #ffd60a,
  Unhealthy #ff9f0a, Very Unhealthy #ff453a — and put the category word in the sub.
- UV INDEX — `sys.weather(LAT, LON, "daily.uv_index_max.0")`; sub Low/Moderate/
  High/Very High.
- HUMIDITY — `sys.weather(LAT, LON, "current.relative_humidity_2m") + "%"`; sub free.
- WIND — `sys.weather(LAT, LON, "current.wind_speed_10m") + " km/h"`; sub free.

SUNRISE and SUNSET are NOT tiles here — they belong to the SUN & MOON panel above.

Keep every sub-line to TWO OR THREE WORDS ("Moderate", "Light breeze", "Feels
humid"). A tile is half the screen wide minus padding; a sentence like "Dew point
comfortable" does not fit and is clipped mid-word, which looks like a bug.

## CARD HEIGHT — get this wrong and everything below the fold is LOST

The card is a TALL page (~1500dp) that the user drags through; it does NOT fit one
screen. The card list itself does the scrolling, so the card must simply BE tall.
Use EXACTLY this height scheme:

| element | height |
|---|---|
| root Overlay | `Fit` |
| backdrop `Image` | `2000` — a FIXED number; THIS is what makes the card tall |
| dark scrim over the photo | `Fill` |
| inner `flow: Down` column | `Fit` |

The photo must be TALLER than the content, not merely tall. A `Fit` Overlay takes
the height of its tallest child, so if the column outgrows the photo the surplus
has no photo behind it and ends in a hard edge of bare `#0a0e14` partway down the
card. `2000` clears the full content with room to spare; any leftover photo at the
bottom simply reads as more sky.

Two failures to avoid, both of which silently DISCARD the bottom half of the card:

- **A fixed height on the ROOT** (`height: 858`, one screen) clips everything past
  it. Nothing scrolls, because there is nothing to scroll — the content is simply
  truncated, and the maps, SUN & MOON panel and detail tiles never appear.
- **`height: Fill` on the inner column** resolves to the root's height rather than
  to the content's, so the column cannot grow to hold what you put in it. It must
  be `Fit`.

Do NOT wrap the card in a `ScrollYView` — it is not needed and a fixed-height one
reintroduces the clipping. Give the page comfortable, breathable spacing rather
than cramming sections in to save vertical room; there is no room to save.

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
