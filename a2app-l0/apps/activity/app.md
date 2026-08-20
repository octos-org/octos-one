# activity — requirements

Places worth going, near the user. Use it for "what's nearby", "things to do
around me", "附近有什么好玩的".

`exemplar.card` meets every requirement below.

---

## Data — mandatory

| what | source |
|---|---|
| where the user is | `sys.geocode(name: state.city)` |
| venues of one category | `sys.places(lat:, lon:, category:, count:, fields: [id, name, distance])` |

**Never invent a venue.** Every name and distance comes from `sys.places`. A
plausible-sounding café that does not exist is the worst thing this card can do,
because the user will go there.

Bind **at most two** categories — one fetch each. Categories: `park garden museum
cafe cinema gym library pool viewpoint playground trail`.

## Structure

- An eyebrow naming the theme and a title, both from `copy`.
- A `Panel` of rows, one `for` per category, keyed on the venue id.
- Each row: a category glyph, the venue name, and beneath it the **live**
  distance with a short phrase as `suffix:` — "300 m away · quiet green space".
  The distance is bound; the phrase is `copy`.
- A `Rule()` between rows.

## Loading

```
copy loading { class: vocabulary, en: "Finding places nearby…" }
when parks.$state == .pending { TextBody(text: copy.loading) }
```

**`copy.loading` has to be DECLARED like any other copy.** A `copy.x` that is
not declared is refused, by any route — this snippet is the most-copied lines in
the memory, and showing the use without the declaration is why cards come back
refused for `copy.loading is not declared`. Same for an empty-state string.

Do **not** compare a count against a sentinel. `$state` is what says "not yet".

## Known limitation

L0 cannot yet say "this collection is empty", so a card cannot show a
"nothing nearby" line. Leave it out rather than approximating it with a guard
that does not mean that.

## Failure conditions

- a venue name or distance written rather than bound
- more than two categories
- a row without its live distance
- any colour or font size
