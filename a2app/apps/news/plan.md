# News app — plan spec

Emit EXACTLY ONE ```runplan fenced block containing JSON. Nothing else.

You do **not** write the card. You choose what it shows; the runtime builds it.

```runplan
{
  "plan": "news",
  "locale": "en",
  "sections": [
    { "block": "Masthead",  "args": { "title": "Top Stories", "label": "HACKER NEWS" } },
    { "block": "LeadStory" },
    { "block": "StoryFeed", "args": { "count": 7, "label": "LATEST" } }
  ]
}
```

## Blocks

| block | args |
|---|---|
| `Masthead` | `title`, `label` — your wording |
| `LeadStory` | none |
| `StoryFeed` | `count` 1–20 (default 7), `label` |

## What you decide

The **editorial voice**: the masthead wording, the section labels, how many rows,
the language. Also whether this is the right app at all, and whether to compose it
with another.

## What you must NEVER write — there is no field for it

**Not one word of story content.** No headline, no author, no points, no comment
count, no url, no rank. Every one of those is `sys.news(i, "key")`, fetched from the
live front page when the card draws.

This matters more here than anywhere else. A wrong coordinate sends you to the wrong
city; **a headline you wrote is a fabrication.** It is indistinguishable from a real
one, it is quotable, and nothing downstream can tell it apart. So the schema has no
place to put one, and a plan that tries is rejected with the field named.

`LeadStory` takes no arguments at all — it is always the current top story.

## Ranks

`LeadStory` is index 0 and `StoryFeed` starts at index 1, so the top story is not
shown twice. You do not number the rows; the runtime does.

---

*This replaces the DSL-authoring spec in [`app.md`](app.md). Lowering is
`app/app/src/app/plan/news.rs`.*

**One limit worth knowing:** a plan-lowered feed is READ-ONLY. The DSL version had
tappable rows opening a story detail, which needs interactive state — and a state
write currently rebuilds the whole card, which would lose the reader's scroll
position on every tap. Tracked in `docs/CARD-STATE-IDENTITY.md`.
