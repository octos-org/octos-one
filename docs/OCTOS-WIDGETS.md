# octos.* — the web-widget kit (web counterpart of `glass.*`)

`octos.*` is a JS kit **auto-injected by the framework into every `runhtml` web
card** (inlined at the head of each document by `WebCard`/`inject_widget_kit`).
It is the web analogue of Splash's `glass.*`: a card COMPOSES from `octos.*`
instead of hand-rolling the theme, the player, the bottom sheet, toasts,
avatars, state, or live-data tiles. A card supplies **data + layout + thin
handlers**; the kit owns the parts that must stay consistent.

Proven: the YouTube reference card dropped from ~42 KB hand-rolled to a **14 KB**
card composing the kit, rendering the identical 9/10 UI.

## Componentized into layers (load order = dependency order)

The kit is split into `aichat/widgets/src/`:

| Module | Namespace | What it is |
|---|---|---|
| `octos_core.js` | `octos.core` (+ root aliases) | **domain-agnostic primitives** every card uses: theme tokens + reset, state, avatar, shared icon map, toast/sheet/dim, `http` (with a CORS proxy). The web `glass.*` core. |
| `octos_media.js` | `octos.media` / `octos.*` | the **YouTube/video** kit built on core: player+captions+PiP, tiles, action-bar, channel-row, comments, chips. |
| `octos_finance.js` | `octos.finance` / `octos.stock` | the **finance** kit + the data-bound component `octos.stock()`. |
| `octos_weather.js` | `octos.weather` / `octos.forecast` | the **weather** kit + the data-bound component `octos.forecast()`. |

`core` is loaded first so the domain kits can reference its theme/state/icons/
http. New domains (news…) add another `octos_<domain>.js` built on core.

## API (window.octos)

### core — `octos.core.*` (common ones also aliased at `octos.*`)

State / helpers
- `octos.ns(prefix)` · `octos.get(k,d)` · `octos.set(k,v)` — namespaced localStorage (shared across cards, same origin)
- `octos.esc(s)` · `octos.strip(s)` (drops emoji)
- `octos.ic(name)` — SVG icon (shared map; domain kits add more); `octos.core.avatar(name,size,fontSize[,imgUrl])` — generic gradient/monogram avatar + optional image
- `octos.toast(msg[, actionWord])` · `octos.sheet(title, items[])` · `octos.closeSheet()` — dark Material bottom sheet
- `octos.http.getJSON(url)` · `.getText(url)` · `.getJSONx(url)` — **getJSONx** routes a keyless, CORS-less API through the shared CORS proxy so it's fetchable from the card origin
- CSS: theme tokens (`--o-bg/--o-fg/--o-mut/--o-card/--o-line/--o-up/--o-down/--o-accent`) + generic primitives (`.oc-card`, `.oc-row`, `.oc-btn`, `.oc-btn.pri`)

### media — `octos.*` (YouTube/video)

- `octos.thumb(id)` · `octos.oembed(id, cb)` · `octos.ytId(q)` · `octos.handles = {channelName:"youtubeHandle"}` (real avatars) · `octos.avatar(name,size,fs)` (youtube-flavored)

Player (IFrame API, captions, translate, PiP, gestures) — `octos.player`
- `.mount(hostId)` · `.load(video)` · `.toggle()` · `.stop()` · `.state()` · `.onState(cb)`
- `.captions({on, lang, size})` — on/off, translate to any `lang`, `size` -1..3 (2–3 = large/bold); `.LANGS` (~30)
- `.fs()` — fullscreen · `.mini(bool)` — toggle the square PiP · `.gestures(onMinimize)` — **swipe-DOWN on the player → onMinimize** (+ tap = play/pause), like the real app

Visual widgets (return HTML; handler args are global fn names)
- `octos.topbar({onHome,onBell,onSearch,onYou})` · `octos.chips(list,active,onPick)`
- `octos.playerHtml({onMin,onMax,onToggle,onClose})` — the player element (host + minimize chevron + PiP overlay; gesture overlay + fullscreen button are added by `.gestures()` after mount, since the IFrame API eats static siblings)
- `octos.tile(v,onTap)` · `octos.feedCard(v,onTap)` · `octos.channelRow(v,subbed,onSub)`
- `octos.actionBar(v,state,handlers)` — like/dislike/share/save/remix/report/captions/translate
- `octos.comments(id,list,expanded,handlers)` · `octos.sec(label,viewAllFn)`
- `octos.setKebab(fnName)` — global kebab handler used by tiles/feed cards

### finance — the data-bound component

`octos.stock(ticker[, {onTap}])` — the reference **data-bound component**: it
FETCHES its own live quote (Yahoo v8 chart via `octos.http.getJSONx`) AND RENDERS
the tile (name, price, direction-colored change, intraday sparkline). Returns a
skeleton immediately and fills after the fetch. The card writes **one call** per
ticker — the web fusion of a widget + a `sys.*` data helper, i.e. the web twin of
the native `sys.stock()`. Also: `octos.finance.quote(ticker) → Promise<{name,
price,change,pct,cur,spark}>` if you only want the data.

```html
<div id="mk"></div>
<script>
  document.getElementById("mk").innerHTML =
    ["AAPL","MSFT","NVDA","TSLA"].map(t => octos.stock(t)).join("");   // that's the whole card
</script>
```

Proven on-device (OnePlus 6T): all tiles fetch live quotes through the CORS
proxy and render with sparklines.

### weather — a second data-bound component

`octos.forecast(place[, {unit:"f", lat, lon, onTap}])` — GEOCODES the place name
AND fetches the 7-day forecast AND renders the card (current temp/condition/icon,
feels-like + humidity + wind, and a scrollable 7-day strip with weather icons).
Both calls are keyless open-meteo and **CORS-open, so no proxy** (contrast
`octos.stock`, which needs the proxy). Pass `{lat,lon}` to skip geocoding, or
`{unit:"f"}` for Fahrenheit/mph. Also: `octos.weather.geocode(place)` and
`octos.weather.data(lat,lon,opts)` for the raw data; `octos.weather.wcode(code)`
maps a WMO code to `[label, iconName]`.

```html
<div id="wx"></div>
<script>
  document.getElementById("wx").innerHTML =
    ["Shanghai","Tokyo","London"].map(c => octos.forecast(c)).join("");   // whole card
</script>
```

## Composing a card

```html
<body>
<div id="topbar"></div><div id="watch"></div><div id="home" class="o-hidden"></div>…
<script>
octos.handles = {…};
var CAT = [ {id,t,ch,live,sub,tags}, … ];
/* view functions call octos.* widgets; handlers mutate octos state + rerender */
</script>
</body>
```

Every card is one document composed of `octos.*` widgets (the "webview +
web-widget" model). The same kit renders the monolith OR per-screen cards — see
`APP-CARD-GRANULARITY.md`.
