# sys.* helpers

There are TWO kinds of `sys.*` helper:

1. **Image helpers** (`sys.photo`, `sys.satellite`, `sys.basemap`, `sys.airmap`)
   return a URL — always use as `http_resource(sys.xxx(...))` inside an `Image`.
2. **Data helpers** (`sys.weather`, `sys.airquality`, `sys.stock`, `sys.news`,
   `sys.movers`, `sys.places`) return a LIVE VALUE STRING — use directly as
   `Label` text. Their numeric twins (`sys.weathernum`, `sys.aqinum`,
   `sys.placesnum`) return NUMBERS for `if` conditions. See the DATA section
   at the bottom.

## sys.photo("<city> <scene/weather>")

Returns an AI photo URL. Pass a short description combining the city and the
scene/weather so the photo matches the actual conditions.

```
Image{ src: http_resource(sys.photo("tokyo skyline clear sky")) fit: ImageFit.CropToFill width: Fill height: Fill }
```

## sys.satellite(lat, lon)

Returns a real satellite cloud-imagery URL (the 卫星云图). Pass the city's real
decimal lat/lon.

```
Image{ src: http_resource(sys.satellite(35.68, 139.65)) fit: ImageFit.CropToFill width: Fill height: 190 }
```

## sys.basemap(lat, lon) + sys.airmap(lat, lon)

The air-quality map (空气质量图) is two layers: a base map with an air-quality
overlay ON TOP. Stack `airmap` OVER `basemap` in a `height: 190 flow: Overlay` View
(fixed height, NOT Fill — Fill inside an Overlay wrongly resolves to the whole card):

```
View{ width: Fill height: 190 flow: Overlay
    Image{ src: http_resource(sys.basemap(35.68, 139.65)) fit: ImageFit.CropToFill width: Fill height: 190 }
    Image{ src: http_resource(sys.airmap(35.68, 139.65)) fit: ImageFit.CropToFill width: Fill height: 190 }
}
```

## Lat/lon

Pass the city's real decimal lat/lon (e.g. Tokyo 35.68, 139.65). Both maps —
`sys.satellite`, `sys.basemap`, and `sys.airmap` — take the SAME lat/lon.

---

# DATA helpers — LIVE weather numbers (NEVER hardcode)

`sys.weather(lat, lon, "path")` and `sys.airquality(lat, lon, "path")` return the
REAL, CURRENT value as a string, fetched live from the open-meteo API. **You MUST
use these for EVERY weather/air number in the card. Do NOT write literal numbers
you invented — you have no idea what the real weather is.** While the data loads
the helper returns "—", then the card auto-refreshes with the real value.

Use the returned string directly as a `Label`'s `text`, concatenating units:

```
Label{ text: sys.weather(35.68, 139.65, "current.temperature_2m") + "°" }
Label{ text: "湿度 " + sys.weather(35.68, 139.65, "current.relative_humidity_2m") + "%" }
Label{ text: "AQI " + sys.airquality(35.68, 139.65, "current.us_aqi") }
```

## sys.weather(lat, lon, "path") — forecast API

`path` is a dot-path into the JSON; a numeric segment indexes an array. Fields:

- `current.temperature_2m` — temp now (°C)
- `current.apparent_temperature` — feels-like (°C)
- `current.relative_humidity_2m` — humidity (%)
- `current.wind_speed_10m` — wind (km/h)
- `current.surface_pressure` — pressure (hPa)
- `current.weather_code` — WMO code (integer)
- `current.is_day` — 1 day / 0 night
- `daily.temperature_2m_max.N` / `daily.temperature_2m_min.N` — day N high/low
  (N = 0 today, 1 tomorrow … up to 6). Build a 7-day row by repeating with N=0..6.
- `daily.sunrise.N` / `daily.sunset.N` — returned as "HH:MM" (already formatted)
- `daily.uv_index_max.N` — UV index
- `daily.precipitation_probability_max.N` — rain chance (%)

## sys.airquality(lat, lon, "path") — air-quality API

- `current.us_aqi` — US AQI (the number for the 空气质量图 pane)
- `current.pm2_5`, `current.pm10`, `current.ozone`, `current.nitrogen_dioxide`

All weather/air data helpers take the SAME lat/lon as the map helpers.

## sys.stock("<TICKER>", "key") — LIVE stock quote (Yahoo Finance)

Returns the live value for an UPPERCASE ticker (`AAPL`, `TSLA`, `NVDA`, `MSFT`,
`AMZN`, `GOOGL`, `META`, …). Use directly as `Label` text.

- `symbol`, `name` (company), `exchange`, `currency`
- `price`, `prev` (previous close), `high` (day high), `low` (day low)
- `52wh` / `52wl` — 52-week high / low
- `vol` — volume, formatted (e.g. `41.4M`)
- `change` — price − previous close, signed, e.g. `+1.99`
- `changepct` — percent change, signed, e.g. `+0.63%`
- (`open` exists but Yahoo often omits it → shows `—`; prefer `prev`/`52wh`/`vol`)

```
Label{ text: "$" + sys.stock("AAPL", "price") }
Label{ text: sys.stock("AAPL", "change") + " (" + sys.stock("AAPL", "changepct") + ")" }
```

## sys.news(index, "key") — LIVE top headlines (Hacker News front page)

`index` 0 = the top story, 1 = next, up to ~11. Use directly as `Label` text.

- `title`, `url`, `author`, `points`, `comments`

```
Label{ width: Fill text: sys.news(0, "title") }
Label{ text: sys.news(0, "points") + " points · " + sys.news(0, "author") }
```

## sys.weathernum / sys.aqinum — LIVE numbers for script conditions

The same values as `sys.weather` / `sys.airquality` but returned as NUMBERS,
so `if` conditions can branch on live data — the primitive for COMPOSED cards
(activity choice by temperature, outdoor gating by AQI/rain, day/night
switches on `is_day`). While the fetch loads (or the path is absent) they
return `-9999`; guard with `>= -9998` and the card re-evaluates when data
lands:

```
if sys.weathernum(37.34, -121.89, "current.temperature_2m") >= 18 {
    Label{ text: "Great time for a walk — " + sys.weather(37.34, -121.89, "current.temperature_2m") + "°" }
} else {
    Label{ text: "Better indoors right now" }
}
```

## sys.places(lat, lon, "category", index, "field") — REAL nearby places (OpenStreetMap)

Returns live details of REAL venues near lat/lon, fetched from OpenStreetMap.
**You do NOT know what is near the user — every displayed venue name/distance
MUST be one of these bindings; NEVER type a venue name yourself.** `index` 0 =
the nearest venue, 1 = next, and so on. ONE fetch serves every index/field of
a (lat, lon, category) — extra bindings on the same category are free. Shows
"—" while loading, then the card auto-refreshes with the real venue.

Categories: `park`, `garden`, `museum`, `cafe`, `cinema`, `gym`, `library`,
`pool`, `viewpoint`, `playground`, `trail` (nature reserve).

Fields (case-insensitive):

- `name` — the venue's real name
- `distance` — formatted distance, e.g. `0.8 km`
- `lat` / `lon` — the venue's coordinates
- `count` — how many venues were found (a string; branch on `sys.placesnum` instead)

```
Label{ text: sys.places(35.68, 139.65, "park", 0, "name") }
Label{ text: sys.places(35.68, 139.65, "park", 0, "distance") + " away · quiet green space" }
```

## sys.placesnum(lat, lon, "category") — venue count for script conditions

The venue count as a NUMBER, so `if` conditions can gate on it (loading
guards, empty states) — the places twin of `sys.weathernum`. Returns `-9999`
while the fetch loads (same sentinel convention); guard with `>= -9998`, and
`0` means the area really has none. The card re-evaluates when data lands:

```
if sys.placesnum(35.68, 139.65, "park") >= -9998 {
    if sys.placesnum(35.68, 139.65, "park") == 0 {
        Label{ text: "No parks nearby" }
    } else {
        Label{ text: sys.places(35.68, 139.65, "park", 0, "name") }
    }
} else {
    Label{ text: "Finding places nearby…" }
}
```

### Concurrency limit — MANDATORY

The places data source allows only 2 concurrent requests per device and
rate-limits offenders (whole card shows "—" rows). A card may bind AT MOST
TWO distinct categories; draw multiple rows from the SAME category by index
(`i` = 0,1,2…) instead of adding categories.

## StockPlot{ symbol range } — the price chart widget (PREFERRED)

A real line/area price chart WIDGET — for any stock chart, use this instead of
composing `sys.stockbar` bars. It fetches the SAME live close series as
`sys.stockbar`/`sys.stockrange` (one shared fetch per symbol×range), so pairing
it with `sys.stockrange` scalars on the same card costs nothing extra.

```
StockPlot{ width: Fill height: 160 symbol: "AAPL" range: rng }
```

- `symbol`: the UPPERCASE ticker. `range`: the same tokens as `sys.stockbar`
  (`"1d"` — also the meaning of `""`/unset — `"1w"`, `"1m"`, `"6m"`, `"1y"`);
  pass the range state variable so the chips switch the chart client-side.
- The widget draws the COMPLETE chart itself: translucent area fill under a
  2dp close-price line, auto-colored green when the range is up / red when
  down (the same convention as `sys.stockrange(sym, rng, "up")`), a dashed
  baseline at the range's first close, hairline gridlines, price labels in
  the right margin, and time labels under the plot (HH:MM exchange-local on
  intraday ranges, M/D on longer ones). Do NOT add manual Y-axis high/low
  labels, gridline stacks, or time-label rows around it — only the textual
  change line (via `sys.stockrange`) remains the card's job.
- Shows a dim `—` until the fetch lands, then fills in (standard live-data
  semantics; no extra bindings needed).
- Optional style properties (defaults are tuned for the dark card gradient —
  transparent background; override only with design-system tokens):
  `up_color` (#30d158), `down_color` (#ff453a), `baseline_color`,
  `grid_color`, `text_color` (labels), `fill_alpha` (0.16), `line_width`
  (2.0), `show_baseline`/`show_grid`/`show_ticks` (all true),
  `tick_font_size` (9), `plot_margin` (Inset{left: 6, top: 8, right: 46,
  bottom: 18} — the right margin hosts the price labels).

## sys.stockbar("<TICKER>", index, count, maxh, "<RANGE>") — chart bar height (FALLBACK)

The bar-row fallback when `StockPlot` cannot be used. Returns a **number** (a
dp height, ~8–158) for one bar of the price path over
the selected range. Bind it to a bar's `height:` — NOT to text. Draw the whole
chart as a bottom-aligned `flow: Right` row of `count` thin `SolidView`s, bar
`i` = `sys.stockbar("<TICKER>", i, count, maxh, rng)`. Bars sit flat at 6 while
the fetch loads, then rise into the real shape:

```
View{ width: Fill height: 188 flow: Right align: Align{y: 1.0} spacing: 2
    SolidView{ width: Fill height: sys.stockbar("AAPL", 0, 40, 186, rng) draw_bg.color: #30d158cc draw_bg.border_radius: 1.5 }
    SolidView{ width: Fill height: sys.stockbar("AAPL", 1, 40, 186, rng) draw_bg.color: #30d158cc draw_bg.border_radius: 1.5 }
    // … repeat i = 0 .. count-1 (count ≈ 40). All bars of one range share ONE fetch.
}
```

- `maxh` (4th arg): the chart's pixel height so the area fills it exactly.
- `"<RANGE>"` (5th arg): `"1d"` (intraday 5-minute, the default — `""`/unset
  also means 1d), `"1w"`, `"1m"`, `"6m"`, `"1y"`. The series is resampled to
  `count` bars, so the SAME bar row serves every range — pass a state variable
  (e.g. `rng`) to make range chips switch the chart client-side.

Use the SAME `count` in every bar and `index` from `0` to `count-1`. Color the
bars green (#30d158) when the range is up, red (#ff453a) when down
(`sys.stockrange(sym, rng, "up") == "1"`).

## sys.stockrange("<TICKER>", "<RANGE>", "field") — range-aware chart scalars

Companion to the chart: scalars computed from the SAME close series the chart
draws — `StockPlot` and the fallback bars share this fetch (free). `sys.stock`'s
`high`/`low`/`change`/`changepct` are
DAY-only; when the chart's range is switchable, the change line (and, on the
fallback bars, the Y-axis labels) MUST use this instead, with the same `rng`
the chart uses. Fields
(case-insensitive): `high`, `low` (range extremes, 2dp), `change`, `changepct`
(signed, first→last close of the range), `up` (`"1"`/`"0"`, for color). Shows
`—` while loading.

```
Label{ text: "$" + sys.stockrange("AAPL", rng, "high") }
Label{ text: sys.stockrange("AAPL", rng, "change") + " (" + sys.stockrange("AAPL", rng, "changepct") + ")" }
```

## sys.movers(index, "field") — LIVE top gainers (Yahoo day_gainers, no auth)

`index` 0 = the biggest % gainer today, up to 9. ONE fetch serves all rows. Use
directly as `Label` text. Fields (case-insensitive): `symbol`, `name`, `price`,
`change` (signed), `changepct` (signed %), `high`, `low`, `prev`, `52wh`, `52wl`,
`vol`, `marketcap` (e.g. `1.31T`), `currency`, `exchange`.

```
Label{ text: sys.movers(0, "symbol") }
Label{ text: "$" + sys.movers(0, "price") }
Label{ text: sys.movers(0, "changepct") }        // e.g. "+21.12%"
```

Make each list row TAPPABLE to open that ticker's detail card: put a transparent
`Button` on top of the row (in a `flow: Overlay`) whose click fires `agent.notify`
with the row's ticker:

```
Button{ width: Fill height: Fill draw_bg.color: #00000000 text: ""
    on_click: || agent.notify("open", {ticker: sys.movers(0, "symbol")}) }
```

The app catches `notify("open", {ticker})` and generates the DETAIL card for that
ticker. The row container must have a FIXED height (not `Fit`) so the `Fill` Button
resolves inside the `Overlay`.
