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

Then SIX picks — three that answer "go out", three that answer "stay in".
Declare all six. A source is a declaration, not a call the view makes, so which
half is DISPLAYED is the verdict's business, and declaring both costs nothing
structural.

---

## The picks — the one thing you know that the host does not

`sys.places(category: "museum")` is a 4 km unranked radius around the city's
centroid. It answers *what is tagged museum nearby*, which is a different
question from *what is worth a day*. Asked about Beijing it returned a police
museum and a hospital's history hall, and never the Forbidden City — which is not
tagged `museum` at all, while the Summer Palace sits outside the radius entirely.
A proximity query cannot rank, and OSM cannot say what matters.

You can. So name the SUBJECTS and let the host resolve every fact about them:

```
source out1 sys.search(query: "Fushimi Inari Shrine Kyoto", count: 1, fields: [name, label])
```

**This is not §4 being bent.** A card already carries `state city { initial:
"Kyoto" }` and `state q { initial: "lofi hip hop radio" }` — a subject is the
same kind of thing. What §4 forbids is writing down what was ANSWERED. You are
writing down what to ASK.

The rules, and they are the whole of your licence here:

- **Well-known and stable only.** A landmark, a major museum, a famous market, a
  district worth an afternoon. Not a specific restaurant, not a pop-up, not a
  shop. If it might have closed since you last heard of it, leave it out.
- **Qualify every subject with its city** — `"Kiyomizu-dera Kyoto"`, not
  `"Kiyomizu-dera"`. The search is worldwide and unanchored, and an unqualified
  name finds the wrong Central Park. This is the same qualification the nav
  router does (`'nvidia headquarters' → 'nvidia santa clara'`).
- **THE ORDER IS YOUR RECOMMENDATION**, and it is the only judgement you are
  contributing. Best first.
- **Never write anything else about the place.** No distance, no opening hours,
  no "a 15-minute walk from the station", no description of what is inside. Those
  change, and a card that states one is stating a fact it cannot check. The row
  shows `hit.name` and `hit.label`, both from the search.
- `sys.search` does not answer `distance`. Do not ask for it and do not display
  it — a picked landmark is somewhere to plan a day around, not the nearest café.

**When the request names NO place, do not guess picks.** You cannot recommend for
a city you have not been told. Declare `sys.places(lat: place.lat, lon:
place.lon, category: …, count: 3, fields: [id, name, distance])` around the
device instead, as the `activity` app does, and show distances there — proximity
is the honest answer to "what is near me".

**Why an invented subject is survivable.** If you name a place that does not
exist, the search answers nothing and the row is ABSENT — a hole a reader can
see. That is the entire argument for letting you do this: a hallucinated number
renders as a plausible number and nothing catches it; a hallucinated subject
renders as nothing at all.

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
- a `component Pick(hit: record)` for the row, written ONCE — six sources of the
  same shape are six chances to drift apart, and the component is what stops it;
- each `for` as a SIBLING of the pending line, never inside a
  `when src.$state == .ready`. A card whose only reference to a source sits
  behind that source's own ready guard gates the render on a state the render
  produces: the `activity` card deadlocked on exactly that and drew "Finding
  places nearby…" forever;
- `theme photo` only if the request asks for that look — never otherwise.
