# Weather-Activity app — COMPOSED requirements spec (assemble from widgets; no exemplar)

This app COMPOSES two parents at the BLOCK level: `apps/weather/app.md`'s
`BLOCK: CURRENT` on top, then `apps/activity/app.md`'s place-row pattern for
the suggestions. Reuse the parents' established blocks EXACTLY as their specs
define them — never redesign them. **One** dark, design-system card answering
"what should I DO given this weather?" — activities / plans / "should I go
out" / "这个天气适合做什么" — where weather or air quality decides the answer.
This file is also the CANONICAL EXAMPLE of a composed spec: when the router
composes a NEW app (see framework.md, `## Composing a NEW app (AMA composer)`),
it imitates this file's shape.

**YOU generate this card by ASSEMBLING the parents' blocks + the fine-grained
widget patterns** — there is no exemplar. Build it from
`widgets/design-system.md` (screen frame, tokens, list rows),
`widgets/weather-icon.md`, and `widgets/sys-helpers.md` (`sys.weather` /
`sys.airquality` display strings, `sys.weathernum` / `sys.aqinum` /
`sys.placesnum` numbers, `sys.places` venues). Requirements below are
MANDATORY.

The VERY FIRST line of the block is `// name: weather-activity` — the name
line is a hard rule. Root/screen: weather's `BLOCK: PHOTO-BACKDROP` — the full-screen city photo matching current conditions + dark scrim (the weather app's visual identity; NOT the plain gradient). The verdict line and each activity-row group sit on translucent panels (`RoundedView` `#00000055`, border_radius 20, padded 14-16) so text stays legible over the photo. Card height 1100+ (a scrolling page like the parent). Resolve LAT, LON to the
request's place (a bare "should I go out" = the user's current city from the
conversation context) — REAL decimal coordinates, the SAME LAT, LON in EVERY
`sys.*` call. No state keys or tap overlays are required.

## Structure, top to bottom

**(1) CURRENT — weather's `BLOCK: CURRENT`**, reproduced per
`apps/weather/app.md` (same content, same live bindings, over the PHOTO-BACKDROP set above — NOT a gradient). Its mandatory bindings, briefly:
- City (font 30).
- Hero temp ALONE on its line (font 60, `margin: Inset{top: 6 bottom: 0}`):
  `sys.weather(LAT, LON, "current.temperature_2m") + "°"`.
- The `flow: Right` row: animated `WeatherIcon{ draw_bg.cond: <N> width: 60
  height: 60 }` + condition Label (font 20), cond per `widgets/weather-icon.md`.
- The H/L/Feels line (font 15, #ffffffcc): `"H:" + sys.weather(LAT, LON,
  "daily.temperature_2m_max.0") + "°  L:" + sys.weather(LAT, LON,
  "daily.temperature_2m_min.0") + "°  Feels " + sys.weather(LAT, LON,
  "current.apparent_temperature") + "°"`.

**(2) THE BRANCH — the composition rule (MANDATORY).** Everything below the
current block is chosen AT RENDER TIME by branching on live NUMBERS
(`sys.weathernum` / `sys.aqinum`, see `widgets/sys-helpers.md`), wrapped in
the loading guard (values are `-9999` until the fetch lands). Skeleton:

```
if sys.weathernum(LAT, LON, "current.temperature_2m") >= -9998 {
    if sys.weathernum(LAT, LON, "current.temperature_2m") >= 18 && sys.aqinum(LAT, LON, "current.us_aqi") < 100 && sys.weathernum(LAT, LON, "daily.precipitation_probability_max.0") < 40 {
        /* OUTDOOR: verdict + place rows from park / garden / trail / viewpoint */
    } else {
        /* INDOOR: verdict + place rows from museum / cafe / cinema / library */
    }
} else {
    Label{ text: "Loading conditions…" }
}
```

The OUTDOOR branch draws its categories from `park`, `garden`, `trail`,
`viewpoint`; the INDOOR branch from `museum`, `cafe`, `cinema`, `library`.

**(3) VERDICT** — each branch STARTS with one plain verdict `Label` directly
in the column (body size, `text/secondary`) that states the call and CITES
live values, e.g. `"Clear and " + sys.weather(LAT, LON,
"current.temperature_2m") + "° — a great day to be outside"` or `"AQI " +
sys.airquality(LAT, LON, "current.us_aqi") + " — better indoors today"`.

**(4) PLACE ROWS** — then 4–6 rows per branch in `apps/activity/app.md`'s row
pattern (plain hairline-separated `flow: Right` rows, NO filled containers):
- its category emoji `Label` (width ~36);
- the venue name = `sys.places(LAT, LON, "<cat>", i, "name")` (heading,
  `text/primary`);
- under it a reason line (caption, `text/dim`) pairing the LIVE distance with
  a LIVE weather/air value, e.g. `sys.places(LAT, LON, "park", 0, "distance")
  + " away · AQI " + sys.airquality(LAT, LON, "current.us_aqi") + " — fine
  for a run"`.

Draw from 2+ of the branch's categories (indexes 0, 1, 2… per category; one
fetch serves each category). Wrap each branch's rows in that branch's places
guard: `if sys.placesnum(LAT, LON, "<lead-cat>") >= -9998 { …rows… } else {
Label{ text: "Finding places nearby…" } }`.

## LIVE DATA — MANDATORY (never invent a value)

- Weather/air DISPLAY values: `sys.weather` / `sys.airquality`; every `if`
  condition: `sys.weathernum` / `sys.aqinum` / `sys.placesnum`. No literal
  weather or air numbers, ever.
- Venue names and distances: `sys.places` ONLY — you have NO idea what is near
  the user; a venue name you typed yourself (even a famous one) is a FAILURE.
- You choose ONLY: the emoji, the condition word + WeatherIcon cond, the
  captions/reason phrasing (nothing venue-specific), and which of the branch's
  categories to draw from. The thresholds (18°, AQI 100, rain 40%) are as
  specced.

## Category budget — MANDATORY

Each branch binds AT MOST TWO distinct `sys.places` categories (outdoor: pick
2 of park/garden/trail/viewpoint; indoor: pick 2 of museum/cafe/cinema/library)
— the data source allows 2 concurrent requests per device; more rate-limits the
device and the rows freeze on "—". Take 2-3 rows per category by index. Only
the ACTIVE branch evaluates, so the two branches never fetch concurrently.

## Failure conditions

A missing `// name: weather-activity` first line, a photo background (this
card is the gradient screen), a missing or partially-bound BLOCK: CURRENT
(city, hero temp, WeatherIcon row, H/L/Feels — all four), a missing `-9999`
loading guard, unconditional suggestions (no `sys.weathernum` / `sys.aqinum`
branch), an outdoor branch not gated on AQI AND rain, fewer than 4 place rows
in either branch, any venue name or distance not bound via `sys.places`, a
reason line citing no live value, any hardcoded weather/air number, or
filled/tinted row containers = FAILURE.
