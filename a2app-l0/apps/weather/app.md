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
| the moon | `sys.moonphase()` |
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
event toggle_units { units: cycle(.c, .f) }
```

Pass `unit: units` to every temperature. Do **not** convert: the runtime formats
by the unit token, and a card that does arithmetic is not an L0 card.

## Structure

- **The page is a `Photo`** wrapping the whole card, with `src: scene`. That is
  the backdrop; the role is the container, not a leaf.
- **Current block** — place name as a caption, the temperature as `TextHero` with
  `unit: units` and `on_tap: toggle_units`, the condition as a `WeatherIcon`.
- **A forecast row per day** — `for d in week.days key d.dayname`, each with
  the day name, a `TempBar(lo:, hi:, min: week.min_lo, max: week.max_hi)` and
  both the low and the high. **Both**: a row showing only the high is missing
  half the forecast.
- **`SunArc(rise:, set:, now:)`** and **`MoonPhase(phase:, illum:)`**.
- **TWO map panes, in this order: `Satellite(lat:, lon:)` then
  `AqiContour(lat:, lon:, span:)`** — the sky (卫星云图) then the air (空气质量图),
  each in its own `Panel` with a caption. Both fetch their own image from a
  LOCATION. Do not declare a source for either and pass values in: a card that
  carried the image would be carrying an observation.
- **A `Row` inside a centred `Col` needs `width: .fit`.** A row FILLS by default,
  because a list row must, and a filling child ignores its parent's alignment —
  so the `↑ ↓ ≈` row under the hero sits hard left without it. `align` on a row
  means the cross axis (vertical) and cannot centre its contents horizontally.
- **A detail grid** of `Tile`s: feels-like, humidity, wind, pressure, UV,
  rain probability. Not visibility: open-meteo serves it hourly only, so no
  call answers it and the tile would render an em dash forever.

## Saved cities

The user's saved places are a **durable collection** (§5.12): read as a source,
written through declared transitions on it, joined to live readings by the host.

```
source cities sys.cities(fields: [name, temp])
source found  sys.search(query: state.query, count: 5, fields: [name, label, query])
state  query   { shape: text, initial: "" }
state  editing { shape: enum[none, add], initial: .none }
event  open_city { city: set($value) }
event  add_city  { editing: set(.add), query: clear }
event  typing    { query: set($value) }
event  pick_city { city: set($value), cities: append($value), query: clear, editing: set(.none) }
event  drop_city { cities: remove($value) }
```

- **A strip of saved rows** — `for c, i in cities key c.name`, each showing the
  stored name and a live `c.temp`, `on_tap: open_city, value: c.name` so a tap
  re-points the whole card. A small remove chip per row fires
  `drop_city` with the row's name as `value:`.
- **An add row** using the editor pattern: a tappable row until tapped, a
  `Field(text: query, placeholder: city, on_commit: pick_city, on_change: typing)`
  only while `editing == .add`. Results are bare rows over `found` gated on
  `query != ""`; a result row's payload is `f.query` — name plus label, the
  text that finds the hit again.
- Never store a temperature or coordinates: the collection keeps **names only**
  and every reading beside one is fetched at read time.

## Loading

```
copy loading { class: vocabulary, en: "Getting the weather…" }
copy offline { class: vocabulary, en: "Can't reach the weather service" }
when now.$state == .pending { TextBody(text: copy.loading) }
when now.$state == .failed  { TextBody(text: copy.offline) }
```

**`copy.loading` has to be DECLARED like any other copy.** A `copy.x` that is
not declared is refused, by any route — this snippet is the most-copied lines in
the memory, and showing the use without the declaration is why cards come back
refused for `copy.loading is not declared`. Same for an empty-state string.

## Failure conditions

- any temperature, condition or measurement written rather than bound
- a forecast row without its low, or without its `TempBar`
- unit conversion done in the card
- `AqiContour` fed data instead of a location
- any colour or font size
