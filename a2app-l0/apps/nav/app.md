# nav — requirements

Getting somewhere: pick a destination, see the route and how long it takes. Use
it for any travel verb — "directions to SFO", "navigate home", "导航去北京".

`exemplar.card` meets every requirement below.

---

## Data — mandatory

| what | source |
|---|---|
| where the user is | `sys.gps()` |
| search results for what they typed | `sys.search(query: state.query, count:, fields: [id, name, lat, lon])` |
| the chosen destination | `sys.search(query: state.dest, count: 1, fields: […])` |
| the trip's facts | `sys.route(from_lat:, from_lon:, to_lat:, to_lon:, mode:, fields: [duration, distance])` |
| the next manoeuvre, while driving | `sys.step(from_lat:, from_lon:, to_lat:, to_lon:, at_lat:, at_lon:, fields: [instruction, remaining])` |

**`mode:` decides the duration**, so pass it: `mode: state.mode` with
`state mode { shape: enum[drive, walk, bike], initial: .drive }` and three `Chip`s
using `active: mode == .walk`. Omit it and every mode shows the driving time — the
same number under a different lit chip.

**A STOP is two route sources, not one with a conditional argument.** A source's
arguments are fixed at declaration, so one source cannot sometimes carry a waypoint:

```
source trip     sys.route(from_lat:…, to_lat:…, mode: state.mode, fields: […])
source trip_via sys.route(from_lat:…, to_lat:…, via: [stop_place.0.lat, stop_place.0.lon],
                          mode: state.mode, fields: […])
when stop == "" { …trip… }
when stop != "" { …trip_via… }
```

Give the `Map` the same `via:` — a map without it draws a line straight past the
stop while the duration beside it is for the journey through it. Per-leg times
(origin→stop, stop→destination) are two more sources, because a leg is a trip.

**One stop, not a list.** L0 has no user-built collection, and the shipping app has
two FIXED slots for the same reason. Each filled combination needs its own declared
source, so the cost is visible — ship one slot rather than pretend two are free.

**`sys.route` and `sys.step` take FOUR COORDINATES, not places.** A route needs
four numbers and an argument carries one value, so a place name has nothing to
resolve into. Read each coordinate off the search that found the place:

```
source trip sys.route(from_lat: origin_place.0.lat, from_lon: origin_place.0.lon,
                      to_lat:   dest_place.0.lat,   to_lon:   dest_place.0.lon,
                      mode: .drive, fields: [duration, distance])
```

Passing places instead is why the duration and distance row rendered `— —` under
a route that drew correctly: the map resolved its endpoints and the text beside it
could not.

**The map fetches its own route.** `Map(mode:, from:, to:, at:, zoom:)` — the card
names the trip, never a polyline and never a marker string. `sys.route` is for the
numbers you print beside the map, not for the line drawn on it.

**`mode:` and `at:` together decide the camera:**

| you want | write |
|---|---|
| the route previewed, fit to the whole trip | `Map(mode: .plan, from:, to:, zoom:)` |
| a chase camera that follows the driver | `Map(mode: .drive, from:, to:, at: here, zoom:)` |

`.drive` WITHOUT `at:` lowers to the preview. That is deliberate, not a bug to
work around: a follow camera needs a position that updates as the user moves, and
handed a route and no position the widget animates along it on a timer — drawing
motion the user is not making. `at: here` is the declared fix that makes the
camera honest.

## State

```
state query  { shape: text, initial: "" }   # what the user is typing
state origin { shape: text, initial: "" }   # where the trip starts
state dest   { shape: text, initial: "" }   # where it ends
state screen { shape: enum[plan, drive], initial: .plan }
```

`screen` is an ENUM and not a `bool`. A guard tests a declared name against a
declared value, so `when driving` names the true case and there is no total form
for the false one — `when driving == false` asks the checker to accept an
undeclared literal, and a `not` operator would be the expression form L0 does not
have. Two named screens have two guards that each say which screen they are.

**When the request already NAMES the places, they are the initial state.** The
empty initials above are for "directions" with no destination — a card that opens
on its search box. A request that says where to go must open on that route, or the
user types a place they just said:

```
# "directions from Kyoto Station to Osaka Castle"
state query { shape: text, initial: "Kyoto Station" }
state dest  { shape: text, initial: "Osaka Castle" }
```

This is not a §4 violation and the distinction matters. A place name from the
request is the user's own words used as a QUERY — the card is asking where that
is, not asserting anything about it. Every fact on the screen still comes from
`sys.search` and `sys.route`. What §4 forbids is writing the coordinates, the
distance or the duration.

```
event set_origin  { origin: set($value), query: clear }
event set_dest    { dest: set($value), query: clear }
event choose_dest { dest: set($value), query: clear }
event go          { screen: cycle(.plan, .drive) }
```

The AMA may seed `dest` from the request ("directions to SFO"), so a card that
opens with a destination already chosen is normal.

## Structure

**BOTH endpoints are always editable, and the trip panel is never optional.**

```
Field(text: origin, placeholder: copy.from_ph, on_commit: set_origin, width: .fill)
Field(text: dest,   placeholder: copy.to_ph,   on_commit: set_dest,   width: .fill)
```

An earlier version put the `Field` behind `when dest == ""`, so a card that
opened with the trip already known — which is every card now, because the
request names the places — had no input at all. The user could see a route and
not change either end of it. A destination that arrived from the request is
still a destination the user may want to replace.

- **The two `Field`s, always.** Pre-filled from state, so they show the current
  trip and accept a new one.
- **Search results** when a query has no chosen place yet — `for f, i in found
  key f.id`, each row `on_tap: choose_dest, value: f.id`.
- **The trip's duration and distance**, then the `Map`, then a `Chip` that starts
  the drive: `Chip(text: copy.start, on_tap: go)`. Only `Card`, `Row`, `TextHero`
  and `Chip` take `on_tap` — a `TextRow` does not, and asking for one is refused.
- **The driving screen**, under `when screen == .drive`: the manoeuvre
  (`step.instruction`), the distance left (`step.remaining`), a `Chip` back to
  planning, and the map with `at: here`.

A card that is ONLY a `Map` fails this app. It looks finished and does nothing:
no way to change where you are going, and nothing on screen saying so.

## Loading

```
when trip.$state == .pending { TextBody(text: copy.seeking) }
```

## Known limitations

- **No waypoints.** L0 cannot accumulate a user-built list, so "add a stop" is
  not expressible. Leave it out.
Turn-by-turn IS expressible now, and the constraint that used to make it
impossible is worth knowing because it is the reason `sys.step` has the shape it
has. `sys.navstep` needs a progress-along-the-route in metres; the L2 nav app fed
it `sys.navsecs(period) * 15.2` — a looping clock times an assumed 34 mph — so the
banner announced turns for a vehicle moving whether or not anything was, and
arrived on schedule from a parked car. `sys.step` takes the device's own two
coordinates instead, and the host projects the fix onto the route. Never
reintroduce a clock as a progress argument.

## Do NOT guard the map on a live value

A guard is evaluated at REALIZE time, before any live value exists. So

```
when here.ok == 1 { Map(...) }        # WRONG — never true on a fresh card
```

removes the map unconditionally: measured, a generated card containing `Map(` put
zero maps on the screen. A guard can only test what realization can see —
declared state, or a source's `$state`, which the host injects. A live *value* is
neither.

Write the map unconditionally:

```
Map(mode: .plan, from: origin_place, to: dest_place, zoom: 16)
```

A position that has not arrived is the widget's problem and it is handled there.

## Failure conditions

- a place name, distance or duration written rather than bound
- a polyline or marker string built in the card
- a search result row without `on_tap`
- **a card whose origin or destination cannot be edited** — including a card that
  is only a `Map`
- **a `Map` guarded on a live value** — it will never render
- **a place passed to `sys.route` or `sys.step`** where a coordinate belongs
- **`mode: .drive` with no `at:`** — it silently draws the static preview
- **a clock (`sys.navsecs`, `sys.simsecs`) anywhere** — progress is measured
- any colour or font size
