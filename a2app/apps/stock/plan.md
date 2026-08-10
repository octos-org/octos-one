# Stock app — plan spec

Emit EXACTLY ONE ```runplan fenced block containing JSON. Nothing else.

You do **not** write the card. You choose what it shows; the runtime builds it.

Market movers — no ticker needed, the market decides who is moving:

```runplan
{
  "plan": "stock",
  "locale": "en",
  "sections": [{ "block": "MoversList", "args": { "count": 10 } }]
}
```

A named company:

```runplan
{
  "plan": "stock",
  "locale": "en",
  "sections": [
    { "block": "QuoteHeader", "args": { "ticker": "AAPL", "range": "1d" } },
    { "block": "PriceChart",  "args": { "ticker": "AAPL", "range": "1mo" } },
    { "block": "StatGrid",    "args": { "ticker": "AAPL", "stats": ["price","prev","high","low"] } }
  ]
}
```

## Blocks

| block | args |
|---|---|
| `MoversList` | `count` 1–10 (default 10), `title`, `label` |
| `QuoteHeader` | `ticker` (required), `range` |
| `PriceChart` | `ticker` (required), `range` |
| `StatGrid` | `ticker` (required), `stats` — two or more |

`range`: `1d` `5d` `1mo` `6mo` `1y`.
`stats`: `price` `prev` `high` `low` `open` `currency`.

## What you decide

**The ticker.** Resolving "apple" → `AAPL`, "nvidia" → `NVDA`, "the search company"
→ `GOOGL` is world knowledge and exactly your job — the same work as resolving a
place name. Also which blocks, the wording, the language, and the range.

Give the **symbol**, never the company name. `"Apple"` fetches nothing and renders
dashes, so it is rejected and you are told to use `AAPL`.

## What you must NEVER write — there is no field for it

- **Any price, change, percentage, high, low or market cap.** All live.
- **The company name.** `sys.stock` knows it.
- **Whether the stock is up or down.** This is the subtle one: a plan asserting "up"
  paints a red day green, confidently, for as long as the card exists. The runtime
  reads `sys.stockrange(sym, range, "up")` when it draws, and picks both the colour
  and the ▲/▼ from it.

`MoversList` needs no ticker — asking for one would mean choosing who is moving,
which is the market's answer, not yours.

---

*This replaces the DSL-authoring spec in [`app.md`](app.md). Lowering is
`app/app/src/app/plan/stock.rs`.*

**One limit worth knowing:** a plan lowers to ONE view. The DSL version had a
list→detail flow with tappable range chips, which needs interactive state — and a
state write currently rebuilds the card, tearing down `StockPlot` on every chip tap.
Tracked in `docs/CARD-STATE-IDENTITY.md`.
