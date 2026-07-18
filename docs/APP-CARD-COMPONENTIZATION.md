# App-card componentization

We proved componentization for one app: the YouTube monolith became a 14 KB card
composing the framework-injected `octos.widgets` kit (down from 42 KB
hand-rolled). This doc generalizes that into a **system**: how every app card —
in either substrate — is composed from a shared, data-bound, catalogued
component library.

## The three component layers

```
  app card            youtube-player, weather-shanghai, stock-aapl, my-todo …
      │  composes
  domain components   octos.video(id) · octos.stock("AAPL") · octos.forecast(lat,lon)
      │  built on          (data-bound: render + live fetch, self-contained)
  core primitives     octos.card/button/list/avatar/sheet/toast/chips/chart/state/theme
                          (universal; the web `glass.*`)
```

1. **Core primitives** — universal, domain-agnostic: theme, state
   (`get/set`), `card`, `button`, `list`/`row`, `avatar`, `chips`, `sheet`,
   `toast`, `chart`, `icon`, http/`oembed`. The web counterpart of Splash
   `glass.*`. EVERY card (weather/stock/news/web/youtube) uses these.
2. **Domain components** — the real unit an app card composes from, and the
   key idea: **a component that binds its own live data**. `octos.stock("AAPL")`
   fetches the quote AND renders the tile; `octos.forecast(lat,lon)` fetches
   open-meteo AND renders the 7-day strip; `octos.video(id)` is the whole
   IFrame-API player. This is the **fusion of a widget and a `sys.*` helper** —
   Splash keeps them separate (a `glass.Card` whose `Label` calls
   `sys.stock(...)`); componentization fuses them so the card writes ONE call.
3. **App cards** — thin compositions of domain components + layout. The
   monolith and per-screen cards are both just compositions (see
   `APP-CARD-GRANULARITY.md`).

`octos.widgets` today is layers 1+2 fused for the media domain. Componentizing
the system = **split core out of it** (so weather/stock/web reuse core) and add
**data-bound domain components per app**.

## Substrate parity: `glass.*` (native) ∥ `octos.*` (web)

Two substrates, ONE component vocabulary:

| Concept | Splash (native) | Web (`octos.*`) |
|---|---|---|
| surface | `glass.Card` | `octos.card` |
| button | `glass.GlassButton` | `octos.button` |
| list row | `glass.ListRow` | `octos.row` |
| avatar | (image) | `octos.avatar` |
| sheet | — | `octos.sheet` |
| live data | `sys.stock("AAPL")` | `octos.stock("AAPL")` |

The agent's mental model becomes **substrate-agnostic**: "compose a card from the
component catalog," and the substrate (runsplash vs runhtml) is a rendering
detail. Same component names, two implementations. New concepts get added to
both.

## Why componentization *reinforces* the app-card principle

The principle avoids exemplars because a low-resource DSL needs the model shown
what good looks like. Components remove that need a different way: a component is
a **stable API the LLM composes calls to**, not code it authors. Quality lives
in the component (tested once), so:

- **No exemplar** — the contract documents the component *catalog* (API +
  one-line semantics), the LLM writes `octos.stock("AAPL")`. Compact, correct.
- **Consistency for free** — every card that uses `octos.stock` looks/behaves
  identically.
- **Composability** — the same components render the monolith or N screen-cards.
- **Independent evolution** — fix/upgrade a component once; every card benefits.

So componentization is the mechanism that lets the app-card principle **scale**
past a handful of hand-tuned apps.

## Composition mechanics

- **Today**: render functions returning HTML strings + kit-owned behavior
  (`octos.tile(v, onTap)`), state in namespaced `localStorage`, handlers as
  global fn names. Simple, works, tiny cards.
- **Next (optional)**: real **Web Components** — `<octos-tile video-id="…">`,
  `<octos-stock ticker="AAPL">` — with attributes + slots for true
  encapsulation and declarative composition. The LLM emits tags; the kit's
  `customElements` render + fetch. Most declarative, closest to Splash's DSL
  feel. Cost: a component registry the kit defines once.
- **Data flow**: domain components fetch on mount (deduped, cached in the shared
  origin), re-render on arrival — the web echo of Splash's `DATA_FETCH_EPOCH`
  re-eval. Shared `localStorage` composes state across cards.

## Shell-native components (beyond the document)

Some components can't be in-document JS: the **persistent player overlay**
(survives card swaps), a **native map**, a **native camera**. Componentization
gives them the SAME API surface (`octos.player`, `octos.map`) while the shell
owns the native overlay underneath. A card composes them identically; the
framework routes to native. This unifies "web widget" and "native overlay" under
one catalog (the third category from `APP-CARD-GRANULARITY.md`).

## The component catalog (single source of truth)

One document lists every component — core + per-domain — with signature +
one-line semantics + which substrate(s) implement it. It is the contract
vocabulary the agent composes from (like the Splash manual + `glass.*` reference,
unified). `OCTOS-WIDGETS.md` is the seed of this for web; extend it to a
cross-substrate catalog.

## Recommendation & first steps

1. ✅ **DONE — Split `octos.core` out of the kit.** `octos_widgets.js` became
   three modules the framework injects in order: `octos_core.js` (theme/state/
   avatar/icons/sheet/toast/`http`, domain-agnostic) + `octos_media.js` (YouTube
   widgets on core) + `octos_finance.js`. The YouTube card runs unchanged
   (verified on-device: the watch card renders identically).
2. ✅ **DONE — One data-bound component end-to-end: `octos.stock("AAPL")`.**
   It fetches Yahoo v8 (via `octos.http.getJSONx`, the CORS-proxy path) AND
   renders the tile (name/price/change/sparkline). A stock card is now literally
   `TICKERS.map(t => octos.stock(t))`. Verified live on the OnePlus 6T — every
   tile fetched a real quote and drew its sparkline. This is the **web twin of
   the native `sys.stock()`** (substrate parity, proven both ways).
3. ✅ **DONE — Catalog published** in `OCTOS-WIDGETS.md` (core / media / finance /
   weather layers + `octos.stock` + `octos.forecast`).
4. ✅ **DONE — Second data-bound component: `octos.forecast("Tokyo")`** (module
   `octos_weather.js`). Geocodes + fetches the 7-day open-meteo forecast +
   renders (current + icons + 7-day strip). open-meteo is CORS-open, so it fetches
   **direct, no proxy** — showing the same component pattern with a different data
   path than stock. A weather card = `CITIES.map(c => octos.forecast(c))`.
5. **Next (optional):** Web-Components form (`<octos-stock ticker>`); `octos.news`;
   shell-native components (persistent player, native map) under the same
   `octos.*` API.

Net: the same move we made for YouTube (hand-rolled → composed from a kit),
applied as a **layered, data-bound, cross-substrate component system** — that is
app-card componentization, and it is what makes the app-card principle scale.
Steps 1–3 are shipped and verified on-device.
