# `octos.yt.provider` — a data-source seam for the YouTube card

**Design sketch (not yet wired in).** Goal: let the *same* card UI run on either the
official Google backend (what ships today) **or** a Piped backend, by putting a thin
**provider interface** between the UI and the data source. The UI never changes; you
swap the provider.

Context for *why* this seam (one-app-vs-two, ad-free tradeoffs, the OAuth-verification
constraint) is in the discussion that produced this doc; the short version:

- The card UI (rails, tiles, player chrome, account view, Liked rows) is **~90%
  identical** between Google and Piped. Only the **data**, **auth**, and **playback**
  differ. That difference is exactly what this interface isolates.
- ⚠️ **You cannot ship a Google-OAuth-*verified* app that also does Piped ad-free
  playback** — Google audits the whole app and will terminate the OAuth client. So:
  **one app with a source toggle** is fine for private/test/sideload; **public +
  verified official** must be a *separate* app from the Piped/ad-free one. The
  provider seam makes either outcome cheap — it's the line you cut on.

Today the card is **hardwired to Google** (`octos.auth` + the Data API calls in
`renderYou`). This sketch is the refactor target: move those calls behind
`octos.yt.provider` so Google becomes *one* implementation and Piped can be added as
a second without touching the UI.

---

## The interface

```js
// octos.yt.provider — the contract the card renders against.
// Every method returns a Promise. Shapes are normalized (below) so the UI is
// provider-agnostic.
const Provider = {
  id: "google",                 // "google" | "piped"
  label: "My account",          // shown in the source picker
  capabilities: {               // UI hides what a provider can't do
    account: true,              // real sign-in + subscriptions/playlists/liked
    adFree:  false,             // playback strips ads (Piped=true, Google=false)
    history: false,             // real server-side watch history (both: false)
  },

  // ---- auth (provider-specific mechanics, common method names) ----
  login(onEvent),               // onEvent({phase:'code'|'done'|'error', ...})
  accounts(),                   // [{id, name, avatar}]
  active(),                     // {id, name, avatar} | null
  setActive(id),
  signOut(id),

  // ---- data (all normalized to the shapes below) ----
  search(query),                // -> [Tile]
  subscriptions(),              // -> {total, items:[Channel]}
  playlists(),                  // -> [Playlist]
  playlistItems(id),            // -> [Tile]
  liked(),                      // -> [VideoRow]   (may be [] if unsupported)

  // ---- playback ----
  play(id, mountEl),            // Google: official IFrame embed (ads).
                                // Piped: fetch stream URL -> own <video> (ad-free).
};

// Normalized shapes (what the UI consumes — identical across providers):
// Tile      = { id, t, ch, live }
// Channel   = { name, thumb, cid }
// Playlist  = { id, title, count, thumb }
// VideoRow  = { id, t, ch, views, age, dur }   // dur/views/age already formatted
```

The card calls, e.g., `octos.yt.provider.subscriptions().then(render)` — with **no
idea** whether the data came from Google or Piped.

---

## `GoogleProvider` — maps 1:1 onto what already ships

Everything below already exists; this is just the adapter that presents it through
the interface. Source: `octos.auth` (in `octos_media.js`) + the Data-API calls
currently inline in `youtube-player-reference.html`.

```js
octos.yt.GoogleProvider = {
  id: "google", label: "My account",
  capabilities: { account: true, adFree: false, history: false },

  login: octos.auth.start,        // device-code flow — already built
  accounts: octos.auth.accounts,
  active: octos.auth.active,
  setActive: octos.auth.setActive,
  signOut: octos.auth.signOut,

  search: (q) => octos.ytSearch(q),          // already Piped-backed today; for a
                                             // *verified* official app, swap to the
                                             // Data API search.list (costs quota).
  subscriptions: () => octos.auth.subscriptions().then(j => ({
    total: (j.pageInfo && j.pageInfo.totalResults) || 0,
    items: (j.items||[]).map(it => {
      const s = it.snippet||{}, t = (s.thumbnails&&(s.thumbnails.default||s.thumbnails.medium))||{};
      return { name: s.title||"", thumb: t.url||"", cid: (s.resourceId||{}).channelId||"" };
    })
  })),
  playlists: () => octos.auth.playlists().then(j => (j.items||[]).map(p => {
    const s = p.snippet||{}, th = ((s.thumbnails&&(s.thumbnails.medium||s.thumbnails.default))||{}).url||"";
    return { id: p.id, title: s.title||"", count: (p.contentDetails||{}).itemCount, thumb: th };
  })),
  playlistItems: (id) => octos.auth.playlistItems(id).then(j => (j.items||[]).map(it => {
    const s = it.snippet||{}; return { id:(s.resourceId||{}).videoId, t:octos.strip(s.title||""), ch:s.videoOwnerChannelTitle||"", live:0 };
  }).filter(v => v.id)),
  liked: () => octos.auth.liked().then(j => (j.items||[]).map(it => {
    const s=it.snippet||{}, st=it.statistics||{}, cd=it.contentDetails||{};
    return { id:it.id, t:octos.strip(s.title||""), ch:s.channelTitle||"",
             views:fmtViews(st.viewCount), age:fmtAge(s.publishedAt), dur:fmtDur(cd.duration) };
  })),

  play: (id, mountEl) => octos.player.load({ id }),   // official IFrame embed (ads)
};
```

**Note:** the current card does exactly these transforms *inline* in `renderYou`.
The refactor is mechanical — lift them into this adapter, then have `renderYou` call
`P.subscriptions()` etc. where `P = octos.yt.provider`.

---

## `PipedProvider` — the second implementation (stub)

Same interface, different backend. `octos.ytSearch` already proves the Piped search
path; the rest follows the same keyless-fetch pattern (with a Piped account token for
the personalized calls).

```js
octos.yt.PipedProvider = {
  id: "piped", label: "Ad-free (Piped)",
  capabilities: { account: true, adFree: true, history: false },

  // Auth is a Piped-instance login (username/password), NOT Google device-code.
  // login: POST {instance}/login -> token; store like octos.auth's account store.
  login: (onEvent) => { /* TODO: Piped /login form -> token; onEvent({phase:'done',...}) */ },
  accounts: () => octos.get("piped.accounts", []),
  active:   () => /* active Piped account */ null,
  setActive:(id) => {},
  signOut:  (id) => {},

  search: (q) => octos.ytSearch(q),   // ALREADY WORKS (octos.ytSearch is Piped)

  // Personalized calls need a Piped auth token (Authorization header):
  subscriptions: () => pipedGET("/subscriptions", true).then(a => ({
    total: a.length, items: a.map(c => ({ name:c.name, thumb:c.avatar, cid:c.url.replace("/channel/","") }))
  })),
  playlists:     () => pipedGET("/user/playlists", true).then(a => a.map(p => ({
    id:p.id, title:p.name, count:p.videos, thumb:p.thumbnail })) ),
  playlistItems: (id) => pipedGET("/playlists/"+id).then(p => (p.relatedStreams||[]).map(v => ({
    id:v.url.split("v=")[1], t:v.title, ch:v.uploaderName, live:0 })) ),
  liked: () => Promise.resolve([]),   // Piped has no Google "liked" — [] hides the section

  // Ad-free: resolve a direct stream and play it in our OWN <video>, not the embed.
  play: (id, mountEl) => pipedGET("/streams/"+id).then(s => {
    /* pick s.videoStreams / s.audioStreams -> set mountEl.<video>.src (ad-free) */
  }),
};
```

Caveats to carry from the discussion: Piped instances **rot** (keep a fallback list
like `octos.ytSearchInstances`); it's **ToS-gray**; and it's a **Piped** account, not
your Google one (subscriptions there start empty unless imported).

---

## Wiring: how the card picks a provider

```js
// one shared selection point; default Google.
octos.yt.setProvider = (p) => { octos.yt.provider = p; octos.set("yt.source", p.id); };
octos.yt.provider = (octos.get("yt.source") === "piped")
  ? octos.yt.PipedProvider : octos.yt.GoogleProvider;
```

- **One app + toggle** (private/test/sideload): add a "Source: My account / Ad-free"
  switch to the account view; `setProvider` flips it; `renderYou` re-runs. The UI is
  identical, so nothing else changes. `capabilities.adFree`/`account` let the UI hide
  what a provider can't do.
- **Two apps** (public + verified official): ship two cards that each hard-wire one
  provider — the official card drops `PipedProvider` and switches `search` to the
  Data API; the Piped card drops `octos.auth` entirely (no Google OAuth to verify).
  No UI rewrite, because both render against this same interface.

---

## Refactor checklist (to make this real)

1. Move the inline transforms in `renderYou` (subscriptions/playlists/liked/search)
   into `GoogleProvider` as above; add `octos.yt` + `setProvider` to `octos_media.js`.
2. Change `renderYou` / `openSearch` / `openPlaylist` to call `octos.yt.provider.*`
   instead of `octos.auth.*`/`octos.ytSearch` directly.
3. Gate account UI on `provider.capabilities.account`; gate an "ad-free" affordance
   on `capabilities.adFree`.
4. (Later) implement `PipedProvider` bodies + a Piped-instance fallback list; add the
   source toggle.

Step 1–3 are a no-behavior-change refactor of the shipping Google path; step 4 is the
net-new Piped work. Nothing here touches the shared UI — that's the whole point.
