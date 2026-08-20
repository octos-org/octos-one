# city-picks — requirements

**A composed app.** Parents: `weather` (how a place is right now) and the saved
places the user already keeps. Use it for "where should I go", "compare my
cities", "which of my cities is nicest", "去哪儿好".

Composed rather than routed to `weather` because the question is not about *one*
place. `weather` answers "what is it like in Kyoto"; this answers "of the places
I care about, how do they compare right now" — which needs the user's saved set,
not a place name parsed out of the message.

`exemplar.card` meets every requirement below.

---

## Data — mandatory

| what | source |
|---|---|
| the saved cities, each joined to a live reading | `sys.cities(fields: [name, temp, feels, humidity, wind, cond])` |
| the device locale, for the unit seed | `sys.locale()` |

`sys.cities` is a **durable collection** (profile §5.12). The store holds names
and nothing else; every reading beside a name is fetched when the card draws. So
a saved city can never show yesterday's temperature, and you must not try to keep
one — there is nowhere to put it and there is not meant to be.

Ask for exactly the fields you render. A field outside the `fields:` list reads
as an em dash, which on screen is indistinguishable from a value still arriving,
and the checker now refuses it.

## This app is L1, and why

```
# level: L1
```

It is the one app in this memory that declares a level above L0, and it needs a
single thing from it: **one arithmetic expression**.

```
TextCaption(value: c.feels - c.temp, unit: units, glyph: "≈")
```

"How much warmer or cooler it feels than it is" is not a fact any source
carries — it is a fact *about* two facts the card already declared. That is the
whole of what L1 buys, and §9.3's rule is what keeps it honest: an expression
must **read** something. A coefficient is fine (`c.temp * 9 / 5 + 32` is a
formula); an expression made only of literals is a fabricated number wearing
arithmetic, and is refused.

Do not reach for L1 for anything else here. There is no grouping and no unary
minus, so precedence is fixed — `(a + b) * c` cannot be written.

## What this app deliberately does NOT do

**It does not rank.** L0 and L1 have no sort, no count and no comparison across
loop items. The order on screen is the order the cities were saved, which §5.12
says is the user's order and the only ordering the store carries.

So do not write a card that claims a winner, a "best", or a position. Show each
city and the numbers that let a person decide. A card that says "Kyoto is best"
is asserting something the language cannot compute and the runtime cannot check —
the §4 failure with a superlative instead of a number.

If ranking is ever wanted it needs a language change (an ordering form, or a
capability that returns rows pre-sorted), not a cleverer card.

## Adding a city

The store starts empty, so the card must be able to fill it or the app is a list
nobody can populate:

```
state draft { shape: text, initial: "" }
event add   { picks: append($value) }
Field(text: draft, placeholder: copy.add, on_commit: add, width: .fill)
```

`append($value)` is a §5.12 write: the target is the SOURCE, not a state cell,
and the runtime hands it to the host's store rather than writing anything in the
card. What is stored is the NAME and nothing else — every reading beside it is
fetched when the card draws. `on_commit` carries what was typed, which is the one
payload that does not exist until the moment of commit.

## State

```
state units { shape: enum[c, f], initial: env.locale.temp_unit }
state draft { shape: text, initial: "" }
event toggle_units { units: cycle(.c, .f) }
```

`units` seeds from the device locale — a path-valued initial, not a guess. Pass
`unit: units` to every temperature and never convert in the card.

## Structure

- **A `TextTitle` and a `TextCaption`** saying what the list is.
- **A `Field`** to add a city, committing to `add`.
- **A `Panel`** holding one row per city, `for c, i in picks key c.name`.
- **Each row** is a `Row(align: .center)` of: a filling `Col` with the city name
  and its humidity; a `WeatherIcon(cond: c.cond, size: .row)`; the temperature as
  a `TextValue(unit: units)`; and the L1 feels-difference as a `TextCaption`.
- **The row carries `on_tap: toggle_units`**, so the whole list switches units
  from any row.
- **A `Rule()` between rows.**

## Empty

The store starts empty and the card must survive that: with no saved cities the
loop realizes nothing and the title and caption still render. Do not write an
"empty" branch that compares a count — there is no count. §8 question 10 records
this as open, and it is why the caption has to make sense on its own.

## Failure conditions

- a temperature, humidity, wind or condition written rather than bound
- a claim that one city is better, best, or first
- unit conversion done in the card
- a field read that the `fields:` list does not ask for
- an expression built only of literals
- any colour or font size
