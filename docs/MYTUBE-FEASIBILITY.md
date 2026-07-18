# "MyTube" — a personal YouTube UX as a web app card (feasibility)

**Question.** Can we build a personal, YouTube-like UX — *my* subscriptions, my
playlists, my likes, search, and a player — as an octos-one web app card wired
to the user's own YouTube account?

**Verdict: YES — cleanly and officially — via YouTube Data API v3 + OAuth
"device code" flow, entirely inside the existing `runhtml` card runtime.** No
native changes are required beyond what already shipped (WebView overlay,
`loadDataWithBaseURL` https origin, fullscreen video). The two things the
official API cannot give are watch **history** and the **algorithmic home
feed** — the personal feed is therefore built from subscriptions + playlists +
likes (same approach as every third-party client).

---

## 1. Evidence — probed from a card ON the OnePlus 6T (2026-07-17)

A `mytube-probe` card ran these fetches from the card's `https://octos-one.app`
origin through the device WebView (screenshot in session log):

| Probe | Result | Meaning |
|---|---|---|
| `googleapis.com/youtube/v3/search` (no key) | **readable 403** "unregistered callers" | Data API is **CORS-open** from the card — works once a key/token is attached |
| `oauth2.googleapis.com/device/code` (POST) | **readable 400** invalid_request | **Device-code OAuth can run in-card** |
| `oauth2.googleapis.com/token` (POST) | **readable 400** unsupported_grant_type | Token polling + refresh work in-card |
| `i.ytimg.com` thumbnail `<img>` | **rendered** 320×180 | Thumbnails work (already proven by the player card) |
| `youtube.com/youtubei/v1/browse` (innertube) | **CORS-blocked** | The unofficial API is NOT usable from a card — do not build on it |
| `youtube.com/feeds/videos.xml` (RSS) | **CORS-blocked** | Channel RSS not usable in-card |
| `yt3.ggpht.com` avatar `<img>` | failed with a synthetic URL | inconclusive — retest with a real `snippet.thumbnails` URL from the API (expected to work; `<img>` is not CORS-bound) |

Playback + fullscreen are already proven on-device by the YouTube player card.

## 2. Why OAuth works here (and how)

Google **blocks the normal OAuth web flow inside embedded WebViews** — that
trap is avoided entirely by the **TV & Limited-Input Device flow**, which is
exactly designed for our shape:

1. Card POSTs `device/code` with the user's `client_id` + scope
   `https://www.googleapis.com/auth/youtube.readonly` (or `youtube` for
   like/subscribe actions).
2. Card displays: `Go to google.com/device and enter: ABCD-EFGH`.
   User approves on their laptop/phone browser (any device, 30 seconds).
3. Card polls `token` until it gets `access_token` + `refresh_token`,
   stores them in `localStorage` (`mytube.tokens`), refreshes silently
   thereafter. Login is a **one-time** ceremony.

**One-time user setup (~10 min, free):** create a Google Cloud project → enable
"YouTube Data API v3" → OAuth consent screen (type: TVs & Limited Input
devices; add yourself as test user) → create OAuth client id of type **TVs and
Limited Input devices** → note `client_id` + `client_secret`. These go into the
card (personal use of one's own client id in one's own device is the intended
model for this flow).

## 3. What the personal UX can and cannot contain

| YouTube-like feature | Feasible? | How |
|---|---|---|
| Subscriptions feed (new uploads per channel) | ✅ | `subscriptions.list` → channel ids → `channels.list` (uploads playlist id) → `playlistItems.list` (1 quota unit each, batchable 50/page) |
| My playlists + Watch Later-style saving | ✅ (own playlists) | `playlists.list` / `playlistItems.*`; native Watch-Later list itself is API-inaccessible — use an own "MyTube WL" playlist instead |
| Liked videos | ✅ | playlistItems of the special `LL` playlist |
| Search | ✅ | `search.list` (⚠ 100 units/call — debounce, cache) |
| Watch page + fullscreen | ✅ proven | iframe embed player (`playsinline`, fullscreen via the new custom-view support) |
| Like / subscribe from the card | ✅ | `videos.rate`, `subscriptions.insert` (needs full `youtube` scope) |
| Watch history | ❌ | removed from the Data API years ago — approximate with a local "recently watched in MyTube" list (`localStorage`) |
| Algorithmic home recommendations | ❌ | not exposed; substitute = interleaved recent uploads from subscriptions (chronological feed) |
| Comments (read/post) | ✅/⚠ | `commentThreads.list` works; posting needs full scope + brand-account caveats |

**Quota budget** (default 10,000 units/day): a 50-channel subscription feed
refresh ≈ 1 (subs page) + 1 (channels batch) + 50 (one playlistItems call per
channel) ≈ **52 units** → dozens of refreshes/day are free; searches at 100
units each are the only thing worth rate-limiting.

## 4. Architecture inside the existing card system

- **One `runhtml` card ("mytube")**, generated/refined by the web agent per the
  contract: state object + view functions (Feed / Playlists / Search / Watch
  tabs in-card), `fetch()` with `Authorization: Bearer`, `localStorage` for
  tokens + cache + local history, embed iframe for playback.
- Nothing new is needed from the framework. Optional niceties later:
  - `octos.notify` bridge (§3.5 of WEBVIEW-AGENT.md) to let the agent see
    "now playing" state;
  - a `sys.secret("youtube_client_id")`-style helper so keys live in the octos
    profile instead of the card text (cleaner than baking the client id into
    generated HTML).
- **a2app**: add an `apps/mytube/app.md` **contract** (still no exemplar):
  documents the device-flow snippet, the endpoints table above, quota rules,
  and the embed/watch conventions. The LLM then composes/refines the UX on
  demand ("add a dark red theme", "sort feed by channel") — the personal-
  YouTube app becomes *content*, faithful to the app-card principle.

## 5. Risks / limitations

- **Embed-restricted videos** (some music/label content, error 150): show a
  clear in-card notice + skip; (opening the YouTube app would require allowing
  external intents — currently blocked by design).
- **Unverified OAuth app**: personal client in "testing" mode works for the
  owner (refresh tokens for test users expire after ~7 days unless the consent
  screen is set to "in production" — set production + unverified is fine for
  personal scopes-readonly use; document during setup).
- **Quota on search-heavy use**: cache queries; prefer `playlistItems` over
  `search` wherever possible.
- **Live-stream ids rotate** (lesson learned): resolve `/@channel/live` via
  the API (`search.list eventType=live channelId=…`, 100u) only on demand.
- WebView 92 devices need the WebView update treatment documented in
  WEBVIEW-AGENT.md §8 (the 6T is already on Beta 151).

## 6. Recommended next steps

1. User creates the Google Cloud OAuth "TV" client (10 min, once).
2. Build the `mytube` card v1 **seeded** (deterministic, like the player demo):
   device-flow login → subscriptions feed → tap-to-watch → fullscreen.
3. Promote the patterns into `a2app/apps/mytube/app.md` and let the web agent
   own the UX from then on (LLM-composable personal YouTube).
4. Optional: `sys.secret` helper + `octos.notify` bridge for polish.

---

## 7. Policy check — is Google OK with this? (researched 2026-07-17)

**Yes, an own-account personal client on the official Data API is squarely
within the YouTube API Services Terms** — they explicitly contemplate building
and even distributing "API Clients", subject to the Developer Policies. The
rules that shape OUR design:

- **Never block, modify, or replace ads** ("must not restrict ads from playing…
  must not block, modify or replace advertisements"). → We ship the stock embed
  player untouched; ad-free comes from Premium (Google's own mechanism), not
  from interference. Any ad-blocking hack would be a ToS violation — we don't
  need one.
- **No background playback** ("must not allow background play… when the API
  service window is closed or minimized"). → Cards only play while visible;
  our overlay already detaches on app switch. (This is why NewPipe-style
  clients are non-compliant; we are not that.)
- **Keep YouTube attribution intact**; link YouTube's ToS in the client. → The
  embed shows its YouTube branding; add a ToS link line to the card contract.
- Formal compliance audits apply to clients seeking elevated quota /
  distribution; a personal, single-user client on default quota is the
  intended "development use" shape.

## 8. Premium "no ads" — does it carry into our app?

**Yes, conditionally — and we control the conditions.** Google's guidance and
reporting agree: Premium suppresses ads on **embedded** players when the
viewer is **signed in to youtube.com in that browser context and YouTube
cookies (including third-party) are allowed**. In PowerPoint-style isolated
webviews (no cookies) Premium does NOT apply — which is exactly the failure
mode to engineer away:

1. **Third-party cookies**: the embed iframe sees youtube.com as third-party.
   Android WebView blocks 3P cookies by default — enable per-view:
   `CookieManager.getInstance().setAcceptThirdPartyCookies(web, true);`
   (one line in `ensureSystemBrowser`).
2. **Signed-in session**: the WebView cookie jar needs a one-time
   youtube.com/accounts.google.com login (cookies persist per-app).
   Caveat: Google sometimes rejects WebView logins ("This browser or app may
   not be secure"); the standard mitigations are a Chrome-like user-agent
   string on the login WebView + modern WebView (the 6T now runs Beta 151) +
   cookies enabled. This is a 10-minute empirical test on the device; if
   blocked, fallback is QR-to-phone-browser login → no cookie share → Premium
   ad-free would NOT carry (Custom Tabs cookies don't reach WebView), so the
   UA-mitigated WebView login is the path that matters.
3. **Important distinction**: the Data-API OAuth token (device flow) does NOT
   affect the player — API auth and player cookies are separate identities.
   MyTube needs both: OAuth for your subscriptions/playlists data, cookie
   sign-in for Premium ad-free playback.

Recommended verification (next session, needs your hands for the login): add
the 3P-cookie line + a "Sign in to YouTube" view in the card, you log in once
on the phone, then play a known ad-heavy video and confirm no pre-roll.

**Sources:**
- https://developers.google.com/youtube/terms/developer-policies
- https://developers.google.com/youtube/terms/developer-policies-guide
- https://developers.google.com/youtube/terms/api-services-terms-of-service
- https://developers.google.com/youtube/terms/required-minimum-functionality
- https://support.google.com/youtube/answer/132596
- https://www.androidauthority.com/remove-youtube-premium-ads-3384953/
- https://news.ycombinator.com/item?id=40144199
- https://www.javathinking.com/blog/android-google-login-not-working-inside-webview/
- https://www.windowscentral.com/software-apps/youtube-blocks-background-play-on-third-party-mobile-browsers

## 9. Q&A addenda (2026-07-17)

**Why is there no mass-market third-party YouTube app on the official API?**
Because the API deliberately withholds what an alternative client needs: no raw
video streams (playback only via the unmodifiable embed player), no
recommendations, no watch history, no offline; policy forbids exactly the
features people want alternatives for (ad-free without Premium, background
play, downloads — Vanced was legally shut down over these, and Google stated
2024 enforcement against ad-blocking clients); and quota economics make
consumer scale impossible (10k units/day, no purchase path, ~52 units per
user-feed-refresh → a few hundred users saturate a project; extensions need a
compliance audit). Hence compliant API users are niche tools (uploaders,
analytics, TV apps), while "alternative clients" (NewPipe/FreeTube/Invidious)
scrape the internal innertube API outside the ToS. **A single-user personal
client is the one shape the official API serves well** — quota irrelevant,
stock player, Premium-based ad-free, personalization from subs/playlists.

**Same idea with Google Maps?** Yes, and more first-class: Maps Platform is
*built* for third-party apps (Maps JavaScript API/SDKs, Places, Routes).
Differences: a billing account (card) is mandatory even at $0; since March
2025 the $200 credit was replaced by per-SKU free tiers (~10k calls/month
Essentials — a personal app stays free; set budget alerts + quota caps as a
runaway guard). The card contract needs a documented exception to load the
maps.googleapis.com script. Caveat mirroring YouTube history: **no API exposes
your saved places/lists/Timeline** — one-time Google Takeout export
("Saved Places.json") imported into the app's localStorage is the workaround.
