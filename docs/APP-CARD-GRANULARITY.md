# App-card granularity: how fine is a "widget"?

## The confusion to dissolve first

"How fine-grained should an app card be?" feels like one question but is really
**three different boundaries** that people collapse into one word ("card"):

1. **Reload / state boundary** — where a new document loads and in-memory state +
   playback are lost. (For webview: the HTML document. For Splash: a VM
   generation.)
2. **Composition / authoring boundary** — the unit the LLM assembles a card
   *from*. (A widget.)
3. **Agent-navigation boundary** — what a new *intent* produces. (A routed card.)

These want **opposite** granularities, so trying to pick one number is why it
feels stuck:

| Boundary | Wants to be… | Because |
|---|---|---|
| Reload/state | **COARSE** (app or screen) | crossing it kills playback / state / costs a reload |
| Composition | **FINE** (widget) | small units fit the LLM budget, reuse, stay consistent — the app-card principle |
| Agent-nav | **MEDIUM** (screen) | each *destination* is an intent; in-screen taps are not |

The monolith and the fine-grained principle are not rivals — they live on
**different boundaries**. Keep the monolith at the reload boundary; go fine at
the composition boundary. That's the whole trick.

## Yes: we now have a two-substrate, two-layer model

The observation is exactly right. There are two **substrates** for a card, and
each has a native fine-grained unit:

| Substrate | Card = | Fine unit ("widget") | Compose without reload? |
|---|---|---|---|
| **Splash** (`runsplash`) | native widget tree | a **Splash widget** (`View`, `glass.Card`, `sys.*`…) | yes — one VM/tree, shared state |
| **Webview** (`runhtml`) | one HTML document | a **web widget** (reusable HTML+JS render unit) | yes *within* a document; **no** across documents |

So "webview + web-widget based app card" is a real, clean thing: a webview card
is **one document composed of web widgets**, the way a Splash card is one tree
composed of native widgets. The difference that matters: Splash widgets compose
across the whole shell tree with shared state and no reload; web widgets compose
freely *inside one document* but the **document is a hard reload wall**.

## So: how fine should a web widget be?

A web widget is the **composition** unit, so make it as fine as *reuse +
LLM-budget + a single clear responsibility* justify — and no finer.

A thing deserves to be a widget when it is:
- **One recognizable UI concept** with a single responsibility (a video *tile*,
  the *action-bar*, the *comments* panel, the *filter-chips*, the *bottom-sheet*,
  the *player*, the *PiP*).
- **Data-bindable**: takes a small object and renders — `tile(video)`,
  `actionBar(state)` — the web echo of Splash's `sys.*` binding.
- **Reused in ≥2 places _or_ complex enough to be worth naming** (the player and
  the sheet are used once but are complex; the tile is used everywhere).
- **Self-contained**: its own markup + style + behavior, depending only on its
  data + the shared `yt.*` localStorage keys.

Too **fine** (anti-pattern): a widget per `<label>`/`<button>` — pure overhead,
no reuse, more coordination than inlining. Too **coarse**: a "watch screen"
widget — that's a *card/screen*, not a widget; it can't be reused and it owns
navigation.

Rule of thumb: **coarser than a DOM element, finer than a screen, nameable as a
noun a designer would use.** For YouTube that's ~9 web widgets: `top-bar`,
`filter-chips`, `video-tile`, `player`, `action-bar`, `channel-row`, `comments`,
`bottom-sheet`, `pip`. That is the right resolution — not 3 (too coarse to
reuse), not 40 (noise).

## Three runtime categories fall out (the clean taxonomy)

1. **App card** — coarse, agent-routed, reloadable. An *app* (the monolith) or a
   *screen* (home/watch/search/library). Composed of web widgets.
2. **Web widget** — fine, in-document composition unit. Reusable render function
   over data + shared state. Never routed, never reloaded on its own.
3. **Persistent overlay** — a singleton that must *survive* card swaps: the
   **player**. Not a card (it isn't a destination) and not an ordinary widget
   (it can't live inside a reloadable document if it must keep playing). It is a
   second, shell-owned WebView overlay (`octos_web_player`) — the honest home for
   anything requiring continuity across the reload wall.

Mapping the reload wall onto this: put the wall between **app cards**; keep
**web widgets** inside a card; lift the **player** out of the wall entirely into
the persistent overlay.

## The concrete lever: an `octos.widgets` web-widget kit

Splash cards get `glass.*` in scope — a native widget kit. Give webview cards the
same: a tiny JS module (`octos.widgets`) injected into **every** `runhtml` card
(bundled once, like MEMORY — not per-card exemplar), exposing the ~9 render
functions:

```js
octos.tile(v)            // a video tile
octos.actionBar(state)   // like/dislike/share/save/captions/translate
octos.channelRow(v)      // avatar + name + @handle + subscribe
octos.comments(id)       // device-local comments panel
octos.chips(active)      // home filter chips
octos.sheet(items)       // material bottom sheet
octos.player(v, opts)    // the IFrame-API player (or drives the persistent overlay)
octos.pip()              // the square PiP
octos.avatar(channel)    // real avatar via unavatar + fallback
```

Now **both** the monolith and the screen-cards are composed from the *same* web
widgets — so:
- The **monolith** = `home()+watch()+search()+library()` assembled from the kit
  in one document (continuity, instant nav, one big-but-cheap generation because
  the widgets carry the weight).
- **Screen-cards** = each screen assembled from the same kit in its own document
  (the app-card principle, small cards, agent-composed) — and they look
  identical to the monolith because it's the same widgets.
- The LLM's job shrinks to *layout + which widgets + data binding* — exactly the
  Splash story, now on the web.

This is the payoff of "webview + web-widget based app card": the kit is the
shared substrate, and you can render it as a monolith OR as many cards from the
*same* source of truth, choosing per situation instead of committing globally.

## Recommendation

- **Keep the monolith** as one app card — it stays the best UX for a single
  focused session (continuity, zero-latency nav).
- **Factor its guts into an `octos.widgets` kit** (the ~9 web widgets). No
  behavior change to the monolith; it just gets *composed* instead of hand-rolled.
- **Then** screen-cards become nearly free: same kit, one screen per document,
  agent-routed — turn them on where multi-card composition helps (e.g. deep-link
  "search X", "open channel Y") while the monolith remains the default surface.
- **Player continuity across cards** → the `octos_web_player` persistent overlay
  (framework add), used by both the monolith's PiP and the screen-cards.

Net: the monolith and the fine-grained principle **coexist**, because they sit on
different boundaries. The web-widget kit is what makes them the same thing viewed
at two zoom levels.
