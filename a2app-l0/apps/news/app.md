# news — requirements

Headlines, with a lead story and a feed. Use it for "top news", "what's
happening", "头条", "tech news".

`exemplar.card` meets every requirement below.

---

## Data — mandatory

| what | source |
|---|---|
| the lead story | `sys.news(count: 1, fields: [id, title, author, points, comments, url])` |
| the feed | `sys.news(count:, offset: 1, fields: […])` |
| one story's detail | `sys.news_item(id: state.selected, fields: […])` |

**Never write a headline, an author or a score.** A model-written headline is a
fabricated fact about the world, and it is indistinguishable on screen from a
real one. This is the app where that matters most.

## State

```
state selected { shape: text, initial: "" }   # "" ⇒ the feed, else a story id
```

```
event open_story { selected: set($value) }
event back       { selected: clear }
```

Two complementary guards, feed and detail.

## FEED view

- An eyebrow and a masthead, both from `copy`.
- The lead story in a `Panel`, tappable, showing title, author, points and
  comments.
- The rest as rows in `for s, i in feed key s.id` — the key is the story id, so a
  refreshed feed keeps each row's identity.
- Points and comments carry `suffix:` from `copy` — "412 pts", not "412".

## DETAIL view

- A back affordance.
- The title as `TextTitle`, the byline as a caption, the body as `TextBody`.
- Do not summarise, rewrite or continue the article. If the source has no body,
  show what it has.

## Loading

```
copy loading { class: vocabulary, en: "Fetching headlines…" }
when feed.$state == .pending { TextBody(text: copy.loading) }
```

**`copy.loading` has to be DECLARED like any other copy.** A `copy.x` that is
not declared is refused, by any route — this snippet is the most-copied lines in
the memory, and showing the use without the declaration is why cards come back
refused for `copy.loading is not declared`. Same for an empty-state string.

## Failure conditions

- any headline, author, score or body text written rather than bound
- a story row without `on_tap`, or keyed on the index
- a summary or rewrite of article text
- any colour or font size
