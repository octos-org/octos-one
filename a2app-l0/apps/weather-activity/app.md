# weather-activity — requirements

"What should I DO in this weather?" Use it when the weather decides the answer:
activities, plans, "should I go out", "这个天气适合做什么".

A request naming only a place is still `weather`. A request asking what is nearby
is still `activity`. This app is for the question that needs BOTH.

`exemplar.card` is a working card that meets every requirement below. Read it
first — the decision tree in it is the whole design.

---

## What you fill in

One state, and it is the entire brief:

```
state city { shape: text, initial: "" }   # "" ⇒ the device's own location
```

A bare "should I go out" names no place: leave `city` empty and the host uses the
device's location. Name it when the request does.

---

## The composition

This is a COMPOSED app: the current-conditions block comes from `weather`, the
place rows from `activity`. Reuse those parents' shapes rather than inventing new
ones — a composed app that redesigns its parents is two apps with one name.

```
source place sys.geocode(name: state.city)
source now   sys.weather(lat: place.lat, lon: place.lon,
                         fields: [temp, feels, cond, precip, wind])
source air   sys.airquality(lat: place.lat, lon: place.lon)
source scene sys.photo(query: place.name)
```

Then FOUR place sources — two that answer "go out", two that answer "stay in":
`park` and `viewpoint`; `museum` and `cafe`. Declare all four. A source is a
declaration, not a call the view makes, so which pair is DISPLAYED is the
verdict's business and declaring all four costs nothing structural.

---

## The verdict — a decision TREE, not a conjunction

The parent branched with `if temp >= 18 && aqi < 100 && precip < 40 { … } else
{ … }`. **L0 has neither `&&` nor `else`, and you must not fake them.** Two
guards that can both be true render two verdicts at once, and the card then says
"good day to be outside" directly above "better indoors".

Write COMPLEMENTARY SIBLINGS instead — at each level the two `when`s partition
one number, so exactly one leaf is reached:

```
when now.precip >= 40 { wet }
when now.precip < 40 {
  when air.aqi >= 100 { smoggy }
  when air.aqi < 100 {
    when now.temp < 12 { cold }
    when now.temp >= 12 { fine }
  }
}
```

The ORDER is the reasoning and it is mandatory: **rain, then air, then
temperature.** Rain rules a day out however warm it is; foul air rules it out
however dry. A conjunction hid that ordering; the tree shows it.

Each leaf is a NAMED VIEW carrying a verdict line, a one-line reason, and the
matching place rows. Write each verdict once, as a view — not inline in the
guard, where you would write "better indoors" three times and be able to drift
them apart.

Thresholds are **40 %** precipitation, **AQI 100**, **12°**. They are the card's
own judgement, not facts about the world, so they may be written — but nothing
else may: never a temperature, an AQI value, a venue name or a distance (§4).

---

## The card must also carry

- the §5.9 lifecycle guards on `now` (`.pending` / `.failed`), with copy that
  says which one it is, and a `.pending` guard inside each place panel;
- `Photo(src: scene)` as the root — the parent weather card's identity is a
  photograph of the place, and a plain surface is a different app;
- the current block: eyebrow, the place's name as title, `WeatherIcon(cond:
  now.cond)` beside `TextHero(value: now.temp, unit: units)`, and the two numbers
  the verdict turned on (rain % and AQI) as `Tile`s — so a reader can see WHY;
- `state units` with a `cycle(.c, .f)` event, as every temperature card has;
- `theme photo` only if the request asks for that look — never otherwise.
