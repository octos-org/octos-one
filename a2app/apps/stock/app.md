# Stock app — requirements spec (assemble from widgets; no exemplar)

**One** dark, iOS-Stocks-style splash app that holds BOTH a top-gainers **list**
and a per-ticker **detail** view, with **client-side navigation** between them
and **client-side chart range switching** — no LLM round-trips. Use it for any
stock/market request ("top 10 stocks", "movers", "AAPL", "Tesla stock",
"英伟达股价").

**YOU generate this card by ASSEMBLING the fine-grained widget patterns** —
there is no stock exemplar to copy. Build it from `widgets/interaction.md`
(tappable rows, chip row, state reads/writes), `widgets/containers.md`
(views/pills/gradients), and `widgets/sys-helpers.md` (live data bindings).
`framework/splash-manual.md` has the full DSL if you need it. Requirements
below are MANDATORY; layout details not specified here are yours to design
well within the visual language.

## State model (full-script body)

The VERY FIRST line of the block is `// name: stock-app` — the name line is a
hard rule; a card without it cannot be saved or refined. Then:

```
let sel = "{{state.selected}}"
let rng = "{{state.range}}"
if sel == "0" || sel == "" { /* LIST */ } else { /* DETAIL for `sel` */ }
```

- `selected`: `""`/`"0"` → LIST; a ticker symbol → DETAIL for it. Every list
  row writes it (row overlay pattern) with `sys.movers(i, "symbol")`; the
  detail back button writes `""`.
- `range`: the DETAIL chart range; `""` ≡ `"1d"`. The five chips write it.

## Visual language (both views)

Follow `widgets/design-system.md` for ALL tokens: colors, the type scale,
spacing, radii, layering (`new_batch`), separators, and emphasis rules. The
root/screen frame is the design system's standard 858-tall gradient screen.

## LIST view — MANDATORY contents

- Eyebrow `"TODAY · TOP GAINERS"` (green, 13) and title `"Movers"` (white, 34).
- ALL 10 rows (`i` = 0..9), hairline-separated; EVERY row shows: rank `i+1`
  (dim), `sys.movers(i, "symbol")` (white ~22) with `sys.movers(i, "name")`
  (dim 13) under it, and right-aligned `"$" + sys.movers(i, "price")` (white
  ~22) over green `sys.movers(i, "changepct")` (15). No row may drop a field.
- Each row is `height: Fit` with `padding: Inset{top: 10 bottom: 10}` — the
  name/changepct line must NEVER be clipped by the next row.
- EVERY row is tappable (interaction.md overlay) →
  `agent.notify("set", {key: "selected", value: sys.movers(i, "symbol")})`.

## DETAIL view — MANDATORY sections, top to bottom

1. Back button `"‹  Movers"` (green, transparent Button) →
   `agent.notify("set", {key: "selected", value: ""})`.
2. Header: `sys.stock(sel, "symbol")` (32); under it
   `sys.stock(sel, "name") · exchange · currency` (dim 13).
3. Price: `"$" + sys.stock(sel, "price")` (42).
4. Change line — RANGE-AWARE: `sys.stockrange(sel, rng, "change") + "  (" +
   sys.stockrange(sel, rng, "changepct") + ")"`, colored green when
   `sys.stockrange(sel, rng, "up") == "1"` else red; beside it a dim caption
   per range: `""`/`1d` → "Today", `1w` → "Past week", `1m` → "Past month",
   `6m` → "Past 6 months", `1y` → "Past year".
5. Chart — PREFERRED: `StockPlot{ width: Fill height: 160 symbol: sel
   range: rng }` (see `widgets/sys-helpers.md`). It draws the full chart
   itself — line + area auto-colored by the range direction, dashed baseline,
   gridlines, price labels in the right margin, and time labels under the
   plot — so add NOTHING around it: no manual Y-axis labels, no gridline
   stack, no time-label row.
   FALLBACK (only if `StockPlot` is unavailable, ~116 tall): LEFT Y-axis
   labels `"$" + sys.stockrange(sel, rng, "high")` (top) and `…"low"`
   (bottom), dim 10; the plot is 68 bottom-aligned bars
   `SolidView{ height: sys.stockbar(sel, i, 68, 114, rng) }` (i = 0..67,
   Overlay above 3 hairline gridlines); bars green when
   `sys.stockrange(sel, rng, "up") == "1"`, red otherwise.
6. Time labels (FALLBACK bars only — `StockPlot` draws its own): UNDER the
   plot ONLY when `rng` is `""` or `"1d"`: `09:30 · 12:45 · 16:00`. Other
   ranges show NO time labels.
7. Range chips — ALL FIVE (`1D 1W 1M 6M 1Y`) ALWAYS visible in one row, per
   the interaction.md chip pattern (label + underline + overlay Button); every
   chip writes `{key: "range", value: …}` with values `"1d" "1w" "1m" "6m"
   "1y"`; the ACTIVE chip (matching `rng`, `""` ≡ `"1d"`) is bright `#30d158`
   text with the 2dp accent underline; inactive chips `#ffffff66` text with the
   transparent spacer. No filled pills.
8. Stat grid: a frosted `RoundedView` (`#ffffff0d`) with 3 rows × 2 columns —
   `PREV CLOSE  $prev`, `VOLUME  vol`, `DAY HIGH  $high`, `DAY LOW  $low`,
   `52W HIGH  $52wh`, `52W LOW  $52wl` (labels dim 12, values white ~22),
   hairline row separators.

## LIVE DATA — MANDATORY (never write a number)

- LIST: `sys.movers(i, "field")` — one fetch serves all 10 rows.
- DETAIL header/price/stat grid: `sys.stock(sel, "key")` (`symbol`, `name`,
  `exchange`, `currency`, `price`, `prev`, `high`, `low`, `52wh`, `52wl`,
  `vol`). **Do NOT use `open`** (Yahoo omits it → `$—`).
- DETAIL chart: `StockPlot{ symbol: sel range: rng }` (preferred; live by
  itself) or, on the fallback path, `sys.stockbar(sel, i, 68, 114, rng)`.
- DETAIL change line (and fallback Y-axis labels):
  `sys.stockrange(sel, rng, "high"/"low"/"change"/"changepct"/"up")`.

## Failure conditions

A missing `// name: stock-app` first line, a missing branch, fewer than 10
list rows, a missing detail section, static (non-Button) range chips, chips
writing any state key other than `range`, any hardcoded market number, a
change line whose color contradicts `sys.stockrange(sel, rng, "up")`, or any
dropped field = FAILURE. A `StockPlot` chart wrapped in manual Y-axis labels
or a manual time-label row = FAILURE (the widget draws its own). On the
FALLBACK bar chart only: a missing Y-axis label (BOTH high and low are
required) or time labels on a non-1d range = FAILURE. Length is expected
(~25–35 KB); do not abbreviate or "simplify".
