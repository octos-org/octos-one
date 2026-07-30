# Weather app — plan spec

Emit EXACTLY ONE ```runplan fenced block containing JSON. Nothing else.

You do **not** write the card. You choose what it shows; the runtime builds it.

```runplan
{
  "plan": "weather",
  "locale": "en",
  "place": { "query": "Kyoto" },
  "photo": "kyoto city cloudy sky",
  "sections": [
    { "block": "CurrentConditions" },
    { "block": "Forecast", "args": { "days": 7 } },
    { "block": "AirQualityField" },
    { "block": "SunMoon" },
    { "block": "Details", "args": { "tiles": ["aqi","uv","humidity","wind"] } }
  ]
}
```

That is a complete card. It is about 600 bytes; the card it produces is about 16 KB.

## Fields

| field | value |
|---|---|
| `plan` | always `"weather"` |
| `locale` | `"en"` or `"zh"` — the language of the REQUEST. Everything on the card follows it; you never write the labels themselves. |
| `place.query` | the place NAME, in the request's language. `"上海"` is correct on a Chinese card. |
| `photo` | a backdrop search phrase, your words |
| `sections` | the blocks, in order |

## Blocks

| block | args |
|---|---|
| `CurrentConditions` | none |
| `Forecast` | `days` 1–7 (default 7) |
| `AirQualityField` | none |
| `SunMoon` | none |
| `Details` | `tiles` — two or more of `aqi` `uv` `humidity` `wind` `pressure` |

## What you decide, and what you must not

**Yours** — which place the request means (resolving "nvidia" to Santa Clara is
world knowledge and your job), which sections and in what order, which tiles, the
photo phrase, the locale. In short: **what the user asked for, and what the card
should look like.** You know their preferences and you may be composing with another
app; that judgement is exactly what you are for.

**NOT yours, and there is no field for them:**

- **Coordinates.** `place` takes a name. A latitude you recall is an invented number
  exactly like a temperature you recall — plausible for a famous city, fabricated
  anywhere else, and silently the wrong place when a name is ambiguous. The runtime
  geocodes, in the right language for the script you wrote.
- **Any weather value.** Every temperature, AQI, UV and wind figure is fetched at
  render time. You never see them, so you never type them.
- **The weather itself.** Not the condition word, not the icon, not a per-day
  forecast icon. `weather_code` is already in the fetch, so the runtime derives both
  the icon and the word from it — and they therefore cannot disagree with each other
  or with the sky. Stating "cloudy" for weather you have never observed is the same
  mistake as stating a coordinate, just harder to notice.
- **The week's temperature range.** Derived from the fetched forecast. You cannot
  know it — the values are a live fetch — and guessing it flattens every gradient
  bar to one colour.
- **Weekday names.** Derived from the forecast's own dates, so they belong to the
  FORECAST'S place rather than the phone's, and they stay correct when a saved card
  is reopened tomorrow. Writing them yourself produced an off-by-one in every card
  ever generated, and it looked completely normal.
- **Fonts, colours, sizes, spacing, layout, scrolling.** The theme owns them.
- **Shader uniforms.** Widgets fetch what they need.

## If your plan is wrong

It is rejected before anything renders, with a message naming the field and the
permitted values. Fix that field and re-emit. You are never asked to find one bad
line in a 16 KB card.

## Not this app

A composed "what should I DO in this weather" request routes to
`apps/weather-activity/`. A style keyword (`dark`, `glass`, 深色, 毛玻璃) still means
weather — pass it through as the theme once themes are selectable; today every plan
renders the immersive look.

---

*This spec replaces the DSL-authoring one in [`app.md`](app.md), which is 448 lines
of which 28 are MUST/NEVER prohibitions — one for each bug someone hit. Lowering
lives in `app/app/src/app/plan.rs`; the prohibitions became properties of that code
and of the plan schema, so they cannot be violated rather than merely being
forbidden. `app.md` stays until the generator prompt is switched over and verified
on a device.*
