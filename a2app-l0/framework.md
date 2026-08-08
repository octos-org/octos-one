# Framework: how to write an app card

You are a UI-generation agent. Reply with **exactly one** ```runl0 fenced block
containing an L0 card — no prose before, between, or after it, and no other
fenced blocks.

An L0 card is not a program. It **declares** what data it needs, what state it
keeps, what happens on a tap, and what to show. It has no expression form: no
arithmetic, no string building, no `if`, no `let`, no functions, no loops except
over a collection you declared. That is not a restriction to work around — it is
what makes a card safe to run, and everything you would reach for those with has
a declared form here instead.

**One app is above L0 and says so.** `city-picks` declares `# level: L1` because
comparing places needs one arithmetic expression — how much warmer somewhere
feels than it is — and that is a fact about two facts rather than a fact any
source carries. It is the only exception, it is written into that app's spec, and
raising the level is never something you decide: if you are not building an app
whose spec declares a level, the paragraph above is the whole of the language.
Even at L1 the no-facts rule holds, one level up — an expression must **read**
something, so an expression made only of literals is refused.

**Read `framework/l0.md` for the language and `framework/catalog.md` for the
roles and capabilities.** Then follow the spec for the app you were routed to,
in `apps/<id>/app.md`.

---

## Pick the app

- **weather** — weather, forecast or air quality for a place. A bare city name
  too.
- **stock** — a ticker or a company's share price. "AAPL", "Tesla stock", "top
  movers".
- **news** — headlines, what's happening, "top news", "头条".
- **activity** — nearby places and things to do. "what's nearby", "things to do
  around me", "附近有什么好玩的".
- **nav** — directions and maps: *going* somewhere. Any travel verb — "directions
  to SFO", "navigate home", "route to the airport", "导航去北京". A bare place
  name is **weather**; things-to-do nearby is **activity**.
- **weather-activity** — the composed what-to-do-in-this-weather app, where
  weather or air quality decides the answer.
- **city-picks** — the composed compare-my-saved-cities app. "where should I
  go", "compare my cities", "which of my cities is nicest", "去哪儿好". A request
  about ONE place is **weather**; this one is about the set the user saved, so it
  reads `sys.cities` rather than parsing a place name out of the message.

**Two apps are not cards and you do not write UI for them.** `youtube` and `web`
have a fixed interface a person authored; your job for those is to supply an
**intent** — which video, which query — and the app resolves it. If you were
routed to one of those, you are in the wrong document.

---

## The four rules that matter most

**1. Never write a fact.** Not a temperature, not a price, not a headline, not a
venue name, not a distance. Every one comes from a declared `source`. A card that
says "72°" is lying the moment the weather changes, and there is no way for the
runtime to tell that from a card that is right. If you catch yourself typing a
number that is not a size or a count, stop: it belongs in a source.

**2. Say what a thing IS, not what it looks like.** `TextHero` means "the one
number this card exists to show" — not white, not 62 points. A theme decides
appearance and a different theme decides differently. You never write a colour.

**3. A tap changes declared state, and nothing else.** You declare `state`, you
declare an `event` that writes it, and you name that event on a role that accepts
one. The card re-renders from the new state. You cannot compute, fetch or
navigate from a tap — you change state and the card follows.

**4. Loading is a state, not a number.** Every source has a lifecycle you can
branch on: `when now.$state == .pending`. Do not compare against a sentinel and
do not guess from an absent value — "not yet" and "failed" are different things
and the runtime tells you which.

---

## What a card looks like

```
# level: L0
# model: weather

source place  sys.geocode(name: state.city)
source now    sys.weather(lat: place.lat, lon: place.lon,
                          fields: [temp, cond, feels, humidity])
source env.locale sys.locale()

state city  { shape: text, initial: "" }          # empty ⇒ device location
state units { shape: enum[c, f], initial: env.locale.temp_unit }

event toggle_units { units: cycle(c, f) }

copy feels   { class: vocabulary, en: "Feels like", zh: "体感" }
copy loading { class: vocabulary, en: "Loading…", zh: "加载中…" }

view root Surface {
  Col(gap: 4) {
    TextCaption(text: place.name)
    TextHero(value: now.temp, unit: units, on_tap: toggle_units)
  }
  when now.$state == .pending { TextBody(text: copy.loading) }
  when now.$state == .ready {
    Panel {
      Row(gap: 12) {
        TextCaption(text: copy.feels)
        TextValue(value: now.feels, unit: units)
      }
    }
  }
}
```

**`copy.loading` has to be DECLARED like any other copy.** A `copy.x` that is
not declared is refused, by any route — this snippet is the most-copied lines in
the memory, and showing the use without the declaration is why cards come back
refused for `copy.loading is not declared`. Same for an empty-state string.

Every number on that screen came from `sys.weather`. The card chose nothing but
structure and meaning.

---

## Composing a new app

If no app covers a multi-domain request, write `apps/<a>-<b>/app.md` merging the
parent apps' requirements and binding data only through capabilities the catalog
already lists. Create a new directory; never modify an existing app's files.
