# stock — requirements

A market card: a **list** of top movers, and a per-ticker **detail** with a price
chart and a range picker. Both live in one card and the user moves between them
by tapping — no round trip to you.

Use it for any stock or market request: "top 10 stocks", "movers", "AAPL",
"Tesla stock", "英伟达股价", "top AI stock movers".

**A themed request needs `symbols`.** "top AI stock movers", "chip stocks today",
"bank stocks" — there is no market screener for a theme, so without `symbols` the
card shows the market-wide day gainers under whatever title you gave it, which is
confidently wrong. Pass the tickers you mean:
`sys.movers(count: 10, symbols: "NVDA,AMD,AVGO,SMCI,MU,TSM,MRVL,ARM,CRWV,PLTR,SNOW,AI,VRT,ANET,ORCL,MSFT,GOOGL,META", fields: [...])`.
Which companies count as AI is world knowledge and yours; who among them actually
moved is computed live from the symbols you name. Omit `symbols` for a plain
"top movers" request.

`exemplar.card` is a working card that meets every requirement below. Read it
first — it is shorter than this document.

---

## The state model

Two states, and they are the whole navigation:

```
state selected { shape: text, initial: "" }                      # "" ⇒ list
state range    { shape: enum[d1, w1, m1, m6, y1], initial: .m1 }
```

- `selected` empty shows the list; a ticker shows that ticker's detail.
- `range` is the chart window. The five chips write it.

```
event open_quote { selected: set($value) }
event back       { selected: clear }
event set_range  { range: set($value) }
```

Branch on `selected` with two complementary guards. There is no `else`.

---

## Data — mandatory

| what | source |
|---|---|
| the movers list | `sys.movers(count:, fields: [ticker, name, last, change, pct])` |
| a THEMED movers list | add `symbols: "NVDA,AMD,…"` — see below |
| the selected quote | `sys.quote(ticker: state.selected, fields: […])` |

**Never write a price, a percentage, a company name or a ticker.** Every one
comes from a source. A card with a number typed into it is wrong the moment the
market moves, and nothing downstream can tell that from a card that is right.

The chart fetches its own series — `StockPlot(symbol: selected, range: range)`.
Do **not** declare a series source and pass points in; the card names *which*
series and the widget gets it.

---

## LIST view

- A title from `copy`, not a literal.
- One `Row` per mover, in a `for … key m.ticker` — the key must be the ticker, so
  a row keeps its identity when the list reorders.
- Every row carries `on_tap: open_quote, value: m.ticker`.
- Each row shows: ticker, company name, last price, and percent change.
- The percent change carries `tint: m.change` — that is what makes a fall red and
  a rise green. It is **meaning**, not decoration: without it every row looks the
  same and the card cannot say which way a stock moved.
- `format: .signed_pct` on the percentage, `.money` on the price. Do not write
  the `$` or the `%` yourself.

## DETAIL view

- A back affordance carrying `on_tap: back`.
- The company name and the current price as the `TextHero` — the one number the
  screen exists to show.
- Change and percent change beside it, both tinted by `quote.change`.
- `StockPlot(symbol: selected, range: range)`.
- A `Row` of five `Chip`s — `1D 1W 1M 6M 1Y` — each with
  `on_tap: set_range, value: .<token>` and `active: range == .<token>`.
  **`active` is required.** Without it every chip renders identically and the
  card cannot show which range is selected.
- A `Grid` of `Tile`s for open, high, low, volume, market cap and P/E.

---

## Loading

Guard the detail on the quote's lifecycle:

```
copy loading { class: vocabulary, en: "Fetching the quote…" }
copy offline { class: vocabulary, en: "Can't reach the market feed" }
when quote.$state == .pending { TextBody(text: copy.loading) }
when quote.$state == .failed  { TextBody(text: copy.offline) }
```

**`copy.loading` has to be DECLARED like any other copy.** A `copy.x` that is
not declared is refused, by any route — this snippet is the most-copied lines in
the memory, and showing the use without the declaration is why cards come back
refused for `copy.loading is not declared`. Same for an empty-state string.

Do not test whether a field is empty and do not compare against a sentinel.

---

## Failure conditions

Any of these makes the card wrong, not merely imperfect:

- a price, percentage or company name written into the card rather than bound
- a mover row without `on_tap` — the list becomes a dead picture
- a `for` without `key`, or keyed on the index rather than the ticker
- a percentage without `tint`, or chips without `active`
- a series declared as a source and passed to `StockPlot`
- any colour or font anywhere in the card (layout numbers the catalog admits —
  `gap:`, `cols:` — are fine)
