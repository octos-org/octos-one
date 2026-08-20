# youtube — requirements

A watch card: a live YouTube **search**, its results as rows, and a tap that
plays one over the card.

Use it for any video / music / live-stream / watching request: "play
despacito", "lofi music", "watch news live", "放点音乐".

`exemplar.card` is a working card that meets every requirement below. Read it
first — it is shorter than this document.

---

## What you fill in

One state, and it is the entire brief:

```
state q { shape: text, initial: "lofi hip hop radio" }
```

**`q` is a SEARCH QUERY, not a video id.** "play despacito" → `"despacito"`;
"lofi music" → `"lofi hip hop radio"`; "watch bloomberg live" → `"bloomberg
live"`. Write the words a person would type into YouTube. Adding "official
video", "live", or "full album" when the request implies it is good; inventing
a channel name is not.

**You are NOT the search engine.** The card searches YouTube itself and shows
what actually exists right now. This is the whole reason it replaced the old
web card, which had to ask you to remember video ids — ids you cannot verify
and that go stale silently. **Never write a video id into a card.**

---

## The shape

```
source hits sys.video(query: state.q, count: 12,
                      fields: [id, title, channel, length, views, age, thumb, embed])
source page sys.link(fields: [url])
```

One row per result. Each carries `id`, `title`, `channel`, `length`, `views`,
`age`, `thumb` (an image url) and `embed` — a player url the helper builds,
because L0 has no string concatenation and a card must not assemble a URL.

```
event play { page: set($value) }
...
Row(on_tap: play, value: v.embed) { Thumb(src: v.thumb) ... }
```

The row carries its OWN player url, so playing is one tap: the host opens its
native overlay on that url and closes it on system back. The card never learns
how a video is shown.

The card must also carry:

- the §5.9 lifecycle guards (`hits.$state == .pending` / `.failed`) with copy
  that says which one it is;
- a **search** affordance — a chip that opens a `Field` writing `q` on commit,
  with a `×` that closes it without searching;
- the current query as the card's title, so the user can see what they are
  looking at.
