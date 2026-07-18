# Activity app — requirements spec (assemble from widgets; no exemplar)

**One** dark, design-system NEARBY-PLACES EXPLORER: a category header, then a
hairline-separated list of REAL venues around the user — name, distance, and a
one-line reason each. Use it for any nearby / things-to-do request ("what's
nearby", "things to do around me", "places to visit", "附近有什么好玩的")
that does NOT hinge on weather (weather-decided activity requests route to
`apps/weather-activity` instead).

**YOU generate this card by ASSEMBLING the fine-grained widget patterns** —
there is no activity exemplar. Build it from `widgets/design-system.md`
(screen frame, tokens, list rows) and `widgets/sys-helpers.md` (`sys.places` /
`sys.placesnum`). Requirements below are MANDATORY; layout details not
specified here are yours to design well within the visual language.

The VERY FIRST line of the block is `// name: activity` — the name line is a
hard rule. Resolve LAT, LON to the request's place ("around me" / "nearby" =
the user's current city from the conversation context) — REAL decimal
coordinates, and the SAME LAT, LON in EVERY `sys.places` / `sys.placesnum`
call. No state keys or tap overlays are required — this is a static live-data
card.

## Visual language

Follow `widgets/design-system.md` for ALL tokens: colors, type scale, spacing,
separators. The root/screen frame is the design system's standard 858-tall
gradient screen.

## Structure, top to bottom — MANDATORY contents

- **Category header**: eyebrow `"NEARBY · <THEME>"` (`accent/positive`,
  caption) naming the category theme you chose, and a title (title size,
  `text/primary`) — the city name or "Around You".
- **Loading guard** wrapping the WHOLE list (counts are `-9999` until the
  fetch lands): `if sys.placesnum(LAT, LON, "<lead-cat>") >= -9998 { …list… }
  else { Label{ text: "Finding places nearby…" } }` (body, `text/secondary`).
  The lead category is the first category you use below.
- **Empty state**: inside the guard, branch on `sys.placesnum(LAT, LON,
  "<lead-cat>") == 0` → one friendly line ("Nothing close by — try another
  spot", body, `text/secondary`) instead of the rows.
- **5–8 place rows**. Pick exactly 2 categories that fit the intent from the
  `sys.places` list (`park garden museum cafe cinema gym library pool
  viewpoint playground trail`; default trio `park`, `cafe`, `museum`) and take
  indexes 0, 1, 2… within each (one fetch serves all rows of a category).
  EVERY row shows:
  - its category emoji `Label` (width ~36, e.g. 🌳 park, ☕ cafe, 🏛 museum);
  - the venue name = `sys.places(LAT, LON, "<cat>", i, "name")` (heading,
    `text/primary`);
  - under it a reason line = the LIVE distance + a short category-level
    phrase: `sys.places(LAT, LON, "<cat>", i, "distance") + " away · quiet
    green space"` (caption, `text/dim`).
- Rows are PLAIN hairline-separated `flow: Right` rows (`height: Fit`,
  `padding: Inset{top: 10 bottom: 10}`, `align: Align{y: 0.5}`, a hairline
  `SolidView` between rows) — NO filled/tinted row containers (they do not
  render reliably).

## LIVE DATA — MANDATORY (never invent a venue)

Every venue NAME and DISTANCE comes from `sys.places` — the card shows REAL
OpenStreetMap places; you have NO idea what is near the user, so a venue name
you typed yourself (even a plausible or famous one) is a FAILURE. "—" while
loading is expected; the card auto-refreshes. You choose ONLY: the categories,
the emoji, the header wording, and the short reason phrases (which must say
nothing venue-specific).

## Category budget — MANDATORY

Bind AT MOST TWO distinct `sys.places` categories in the whole card (the data
source allows 2 concurrent requests; more rate-limits the device and every row
shows "—"). Pick the two best-fitting categories for the request and take 3-4
rows from each by index.

## Failure conditions

A missing `// name: activity` first line, any model-invented venue name or
distance, fewer than 5 place rows, a row without its `sys.places` name binding
or without the `sys.places` distance in its reason line, a missing
`sys.placesnum` loading guard, a missing empty-state branch, filled/tinted row
containers, different lat/lon between calls, or a category outside the
`sys.places` list = FAILURE.
