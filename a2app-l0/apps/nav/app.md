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
| the trip's facts | `sys.route(from:, to:, mode:, fields: [duration, distance])` |

**The map fetches its own route.** `Map(mode: .drive, from:, to:, zoom:)` — the
card names the trip, never a polyline and never a marker string. `sys.route` is
for the numbers you print beside the map, not for the line drawn on it.

## State

```
state query { shape: text, initial: "" }   # what the user is typing
state dest  { shape: text, initial: "" }   # what they chose
```

```
event choose_dest { dest: set($value), query: clear }
event clear_dest  { dest: clear, query: clear }
```

The AMA may seed `dest` from the request ("directions to SFO"), so a card that
opens with a destination already chosen is normal.

## Structure

- A trip panel: a FROM row bound to `here.name`, and a TO row that is either a
  `Field` (when `dest == ""`) or the chosen name with `on_tap: clear_dest`.
- **Search results** when `dest == ""` — `for f, i in found key f.id`, each row
  `on_tap: choose_dest, value: f.id`.
- **When `dest != ""`**: the trip's duration and distance, then the `Map`.

## Loading

```
when trip.$state == .pending { TextBody(text: copy.seeking) }
```

## Known limitations

- **No waypoints.** L0 cannot accumulate a user-built list, so "add a stop" is
  not expressible. Leave it out.
- **No turn-by-turn.** This is the planning screen only.

## Failure conditions

- a place name, distance or duration written rather than bound
- a polyline or marker string built in the card
- a search result row without `on_tap`
- any colour or font size
