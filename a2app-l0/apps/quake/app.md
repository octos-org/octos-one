# quake — requirements

The USGS live feed of M2.5+ earthquakes in the last 24 hours, newest first.

Use it for any seismic request: "earthquakes", "recent quakes", "any earthquakes
today?", "地震".

`exemplar.card` meets every requirement below, and this app has almost nothing
for you to fill in — which is the point. The feed is worldwide and it is already
sorted; there is no place to resolve and no option to choose.

---

## What you fill in

Nothing, in the ordinary case. Emit the exemplar.

The feed takes no location argument: USGS publishes one worldwide list and the
card shows the top of it. A request naming a region ("earthquakes in Japan") gets
the same worldwide feed — do NOT invent a filter argument, and do NOT drop rows
that look far away. Filtering you cannot do in the source is filtering the card
would have to fake.

---

## The shape

```
source lead sys.quakes(count: 1, fields: [id, mag, place, depth, ago, lat, lon])
source feed sys.quakes(count: 6, offset: 1, fields: [id, mag, place, ago])
```

One feed, read twice. **`offset: 1` is why the list starts below the lead** —
without it the newest event appears both as the hero and as the first row.

`ago` is already humanized ("now", "12m ago", "3h ago") and `depth` already
carries its unit. Do not compose either from parts; do not write a timestamp.

---

## What you must NOT do

**Never write a magnitude, a place or a depth.** Every one comes from the source.
A card with M6.1 typed into it is wrong minutes later and nothing downstream can
tell it from a card that is right (§4).

**Do not use `Map`.** Its arguments are trip-shaped — `from`/`to`/`via` — and an
epicenter is a point, not a route. `Satellite(lat:, lon:)` is the role that
answers "where", and it fetches its own imagery.

---

## The card must also carry

- the §5.9 lifecycle guards on `lead` (`.pending` / `.failed`) with copy that
  says which one it is;
- the newest event as a `TextHero` of its MAGNITUDE, with the place beneath it —
  the magnitude is what a reader wants first and the place is what tells them
  whether to care;
- a `Satellite` view of the lead event's coordinates;
- the earlier events as rows, each carrying magnitude, place and age;
- copy for a quiet day. An empty feed is an ANSWER, not a failure, and a card
  that renders nothing for it is indistinguishable from one that failed to load.
