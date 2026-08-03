# weather — requirements

Current conditions and a forecast for a place. Use it for any weather request,
including a bare city name ("Kyoto", "东京天气").

`exemplar.card` meets every requirement below.

---

## Data — mandatory

| what | source |
|---|---|
| the place | `sys.geocode(name: state.city)` |
| conditions and forecast | `sys.weather(lat: place.lat, lon: place.lon, days: state.days, fields: […])` |
| sunrise and sunset | `sys.daylight(lat:, lon:)` |
| the moon | `sys.moonphase(lat:, lon:)` |
| a backdrop | `sys.photo(query: place.name)` |

`sys.weather` depends on `place`, so declare it that way and the runtime fetches
in order. Never write a temperature, a condition word, a humidity or a wind
speed.

## State

```
state city  { shape: text, initial: "" }          # empty ⇒ device location
state units { shape: enum[c, f], initial: env.locale.temp_unit }
state days  { shape: number, initial: 7 }
```

`units` seeds from the device locale — a path-valued initial, not a guess. One
event cycles it:

```
event toggle_units { units: cycle(c, f) }
```

Pass `unit: units` to every temperature. Do **not** convert: the runtime formats
by the unit token, and a card that does arithmetic is not an L0 card.

## Structure

- **The page is a `Photo`** wrapping the whole card, with `src: scene`. That is
  the backdrop; the role is the container, not a leaf.
- **Current block** — place name as a caption, the temperature as `TextHero` with
  `unit: units` and `on_tap: toggle_units`, the condition as a `WeatherIcon`.
- **A forecast row per day** — `for d, i in week.days key d.dayname`, each with
  the day name, a `TempBar(lo:, hi:, min: week.min_lo, max: week.max_hi)` and
  both the low and the high. **Both**: a row showing only the high is missing
  half the forecast.
- **`SunArc(rise:, set:, now:)`** and **`MoonPhase(phase:, illum:)`**.
- **`AqiContour(lat:, lon:, span:)`** — it fetches its own field. Do not declare
  an air-quality source and pass values in.
- **A detail grid** of `Tile`s: feels-like, humidity, wind, pressure, UV,
  visibility.

## Loading

```
when now.$state == .pending { TextBody(text: copy.loading) }
```

## Failure conditions

- any temperature, condition or measurement written rather than bound
- a forecast row without its low, or without its `TempBar`
- unit conversion done in the card
- `AqiContour` fed data instead of a location
- any colour or font size
