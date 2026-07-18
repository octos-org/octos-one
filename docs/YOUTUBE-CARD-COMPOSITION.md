# Decomposing the YouTube app into app cards

## Where we are

Today the YouTube experience is **one monolithic `runhtml` card** (~42 KB): a
single-page app where Watch / Home / Search / Library / miniplayer are all one
HTML document, navigated by JS view-switching. It works and scores 9/10, but it
is the *opposite* of the app-card principle — the whole app is one giant card
the LLM must one-shot, and the octos agent is never involved after the first
generation. Per ARCHITECTURE.md the principle is: **each intent → one focused
card; the agent composes the experience from many small cards.**

## The two framework facts that shape everything

1. **One shared WebView.** All web cards render in a single overlay
   (`web_card_browser_id()` = `octos_web_card`); a new card `set_html`-reloads
   that same WebView. So there is exactly one live web document at a time.
2. **One shared origin.** Every card is loaded against base
   `https://octos-one.app/` (loadDataWithBaseURL). Same origin ⇒ **`localStorage`
   is shared across every card**. This is the key enabler: state (favorites,
   history, subscriptions, watch progress) composes across cards for free — a
   Watch card writes `yt.history`, the Library card reads it.

The catch that both facts create: **loading a new card destroys the current
document → playback stops.** Continuous playback + a persistent miniplayer only
exist today *because* it is one document.

## The card family (the decomposition)

Split the monolith into a family of focused card TYPES, each generated
per-intent by the youtube agent, all sharing the `yt.*` localStorage:

| Card | Intent that produces it | Contents |
|---|---|---|
| `youtube-home` | "youtube", "open youtube", "browse" | feed + filter chips |
| `youtube-watch` | "play/watch X", or tapping a video | one player + title/actions/comments/up-next |
| `youtube-search` | "search X on youtube", "find X" | results for a query (catalog + pasted link) |
| `youtube-library` | "my library / subscriptions / history" | You page (history/liked/watch-later/subs) |
| `youtube-channel` | "open <channel>" (optional) | one channel's videos |

Each card is small (fits the generation budget with room to spare), single
-purpose, and independently composable — the app-card principle.

## How the agent composes them

The AMA already routes video/music intents to the youtube agent. Add a **sub
-router**: the youtube agent (or a cheap classifier) maps the intent to a card
type, and each card's taps post a NEW intent back to the agent:

```
"play despacito"        → youtube-watch(despacito)
"open youtube"          → youtube-home
"search lofi"           → youtube-search("lofi")
"my subscriptions"      → youtube-library#subs
tap a video tile        → agent.notify → "play <id>" → youtube-watch(<id>)
tap Home icon           → agent.notify → "open youtube" → youtube-home
```

- **Coarse navigation = new intents = new cards** (home ↔ watch ↔ search ↔
  library). This is the composition.
- **Fine interaction stays in-card JS** (scroll, chips, like, save, comment,
  the kebab bottom sheet) — no agent round-trip for those.
- **Shared state via `localStorage`** (same origin): every card uses the same
  `yt.*` keys, so like/subscribe/history/watch-later persist across cards with
  zero plumbing. (Optionally mirror "now playing" into the shell's
  `a2app_state` via `octos.notify` so the agent can reason about it.)

## Compose each card from ONE component vocabulary

Even split into cards, they must look like one app. So the contract should
define a **component library** the LLM assembles from — the app-card principle
applied at the component level:

- `player`, `action-bar` (like/dislike/share/save/captions/translate),
  `channel-row`, `video-tile`, `comments`, `filter-chips`, `bottom-sheet`,
  `top-bar`, `pip`.

`youtube-home` = top-bar + filter-chips + video-tiles. `youtube-watch` = player
+ action-bar + channel-row + comments + up-next(video-tiles). Same building
blocks ⇒ visual consistency across independently generated cards.

## The one hard problem: playback continuity across cards

A card swap reloads the WebView, so naïvely, browsing Home while a video plays
would stop the video — a regression from today's monolith. Three ways to
resolve it, in order of increasing framework work / increasing correctness:

1. **Accept it** — navigating to a new card stops playback (like opening a new
   page). Simplest; acceptable for a first cut, poor for a media app.
2. **Persistent player overlay (recommended target).** Give the *player* its own
   **second, persistent WebView overlay** (a new framework id, e.g.
   `octos_web_player`) that the shell owns and does NOT reload when the content
   card changes. Content cards (home/search/library/watch-chrome) reload freely
   in the existing `octos_web_card` overlay; the player overlay keeps playing and
   is just repositioned — full-bleed 16:9 under a Watch card, or the square PiP
   under any other card. This is exactly how real apps are built (player = a
   persistent layer; screens compose above it) and it's the honest way to have
   BOTH "many cards" AND continuous playback. Framework cost: a second named
   SystemBrowser overlay + a tiny protocol for the content card to tell the
   shell "play id / go mini / go full / close" (via `octos.notify`).
3. **Keep the monolith just for the player-bearing surface** — i.e. only Watch
   stays a mini-SPA (player + its own chrome), while Home/Search/Library are
   separate cards. A middle ground, but it half-keeps the monolith.

## Recommended architecture

> **Content screens as separate cards, the player as a persistent shell-owned
> overlay, all cards built from one component vocabulary and sharing
> `localStorage`.**

- Home / Search / Library / Channel → **separate `runhtml` cards** the youtube
  agent emits per-intent (pure app-card principle, small, composable).
- Player → a **persistent `octos_web_player` overlay** that survives card swaps
  (framework add), so playback is continuous and the square PiP persists across
  cards. The Watch "card" then only carries the *chrome* (title/actions/
  comments/up-next) around the persistent player.
- State → shared `yt.*` `localStorage` (already same-origin) + optional
  `octos.notify` mirror of now-playing into `a2app_state`.
- Consistency → the contract's component vocabulary.

This turns "one giant card" into "a family of small cards + one persistent
player layer, composed by the agent per intent" — the app-card principle, without
losing the playback continuity we just built.

## Suggested first slice (no framework change)

To prove the model before the persistent-player overlay lands, split just
**Home + Search + Watch** into three cards sharing `localStorage`, using option
(1) for playback (a card swap restarts the player). Concretely:
1. Three `apps/youtube/cards/{home,search,watch}.md` fragments of the contract
   (or one contract with three "emit one of these" modes).
2. Sub-route in `app_splash_router_for("youtube", …)` on the intent.
3. Tiles/nav call `agent.notify` to post the next intent.
4. All three read/write the same `yt.*` keys.
Then land the `octos_web_player` overlay to restore continuous playback.
