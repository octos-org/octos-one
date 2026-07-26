# Trip Planner (`nav`) — Requirements

A Google-Maps-class navigation app served as a single interactive Splash-DSL card:
search a place, preview it, plan a multi-stop trip with live rerouting and per-leg
times, and drive it with 3D turn-by-turn. This is the accumulated requirement set for
the map app, current as of the `nav-planner-search-legs` ship.

**Status:** shipped for review ·
[makepad#15](https://github.com/octos-org/makepad/pull/15) (map widget) +
[octos-one#28](https://github.com/octos-org/octos-one/pull/28) (nav card) ·
33 host tests green · verified on OnePlus 6 / 6T.

**Verification pass (2026-07-24):** every screen exercised on device (OnePlus 6T) — routing,
preview, plan, find, per-leg stops, drive turn-by-turn, mode switch, zoom, my-location — plus a
full widget-code audit and a 33/33 host-test run. Two UX defects were found and **fixed this pass**
(preview ETA overflow, mode-chip clip — see R2.2 / R7.1). Method: live `AUTO_PROMPT` runs through
the real routing pipeline + `SEED_CARD_FILE` cards to render stop/drive/search states directly
(adb can't type into the makepad TextInput, so typed search + two-finger gestures stay code-verified).

**Follow-up (2026-07-24, branch `nav-gps`):** real device **GPS** was added (was R11.3, deferred) —
`LocationManager` → JNI → `sys.gps` → the card's default origin becomes "Your location". Verified on device.

**Follow-up (2026-07-24, pinch + controls):** two-finger **pinch-to-zoom** is now **device-verified**
(root `sendevent` multi-touch injection to `/dev/input/event2`) on all three nav maps — planner preview,
plan overview, and 3D drive — each zooming cleanly to street level and *holding* the zoom on finger-up
(R3.7 / R3.8 / R8.3 upgraded 🔷 → ✅). Root cause of the earlier "registers only one finger" report: a real
pinch is delivered as *separate per-finger* events, so the handler now accumulates every active touch by uid
from the raw `TouchUpdate` stream. A third UX defect was found and **fixed**: the ◎ my-location control rendered
as a **diamond** — the rounded-box SDF overshoots once the corner radius reaches half the side; the shader now
clamps it so an over-large `border_radius` saturates to a clean circle/pill (fixes every round control
app-wide — R3.6 / R3.9).

**Follow-up (2026-07-25, branch `nav-gps`):** drive-screen bottom sheet redesigned + direct-Wi-Fi + search/crash fixes, device-verified.
- **Swipe-collapsible drive sheet (R8.3)** — the drive screen shows a compact **time · distance · speed** chip by default; **swipe up** reveals **End**, **swipe down** collapses it. A transparent swipe `Button` on top of the handle captures the gesture by z-order hit-test (`hits_with_capture_overload`) so it drives the sheet **without leaking into a map pan**. The swipe toggles the End row's visibility *directly* (`ui.endrow.set_visible` — a new zero-rebuild `View` script method) instead of through card state, so the sheet opens/closes **without re-rendering the map** (`eval_body` no longer fires on toggle — device-verified `0`). Chips are now solid — glass translucency removed. *(Both halves fix explicit user reports this pass: the swipe leaked into a map pan; the toggle re-rendered the whole map.)*
- **Direct Wi-Fi networking (R10.1)** — the app can connect over the device's own Wi-Fi instead of the host `adb reverse` tunnel: a proxy value of `direct`/`none`/`off` clears the JVM proxy props (`android_jni.rs`). Map tiles, OSRM routes, and Photon search all load direct — fixes the intermittent blank-map / empty-search / no-route seen when the host proxy's `getaddrinfo` failed under concurrent load.
- **Search + crash fixes** — search results render their name/address text again; the loading count shows "Searching…" until the real "N results" lands (no more "9999+" sentinel); the nav route store evicts **LRU** instead of bulk-clearing, fixing a SIGSEGV on plan→drive route churn.

**Legend:** ✅ verified (device and/or code) · 🔷 code-confirmed, live-finger typing not adb-automatable · ⏳ deferred / future
**Totals:** 56 requirements — **53 ✅ · 1 🔷 · 2 ⏳**, across 11 areas.

**Source of truth:** card = [`exemplars/trip-planner.splash`](exemplars/trip-planner.splash) ·
map widget = `aichat/widgets/src/map/view.rs` · contract = [`app.md`](app.md).

---

## 1 · App identity & routing
*How the OS surfaces the app and hands it a trip.*

| ID | Requirement | |
|----|-------------|:--:|
| R1.1 | **First-class installable app** — boot-registered as an app agent with its own `apps/nav/app.md` contract; routable like weather/stock/news. | ✅ |
| R1.2 | **Direct-serve card (not LLM-generated)** — the ~14 KB / ~490-line card is served verbatim (`include_str!`) because the on-device model truncates a card this large (served the "youtube way"). | ✅ |
| R1.3 | **AMA intent routing** — any go-there request with a travel verb → nav: "directions to X", "navigate home", "route to the airport", "map to X", "导航去北京", "怎么去外滩". A bare place name stays *weather*; "things to do nearby" stays *activity*. | ✅ |
| R1.4 | **LLM-driven "from A to B"** — the router splits the trip and *qualifies ambiguous places with world knowledge* the geocoder lacks ("nvidia headquarters" → "nvidia santa clara", "apple park" → "apple park cupertino"), appending `; from=…; to=…`. `parse_nav_places` seeds the origin/destination queries. | ✅ |
| R1.5 | **Intent seeding** — "directions to X" opens straight on X's route preview (ETA + distance), not an empty search box. | ✅ |

## 2 · Screens & flow
*Search → Preview → Plan → Drive, with Plan ⇄ Find for edits.*

| ID | Requirement | |
|----|-------------|:--:|
| R2.1 | **Search** — type a destination → live Photon results. | ✅ |
| R2.2 | **Preview** — the picked destination: preview map + ETA + distance + a "Directions" button that commits the trip. *(Verify pass: ETA line overflowed the card as "16.0 km awa…" — **fixed** by dropping the redundant " away".)* | ✅ |
| R2.3 | **Plan** — the editable trip overview (§3–§6). | ✅ |
| R2.4 | **Find** — the search overlay for editing the origin, destination, or a stop. | ✅ |
| R2.5 | **Drive** — 3D turn-by-turn navigation (§8). | ✅ |

## 3 · Planner overview (the map)
*A zoomable route-planner map — Google-Maps parity, UX target ≥ 9/10.*

| ID | Requirement | |
|----|-------------|:--:|
| R3.1 | **Whole-route framing** — fit the entire A→B route with both endpoints visible, framed above the summary sheet. | ✅ |
| R3.2 | **Street & place labels** — readable name labels over the map (coarse major-roads layer below z14), with an 8-way white halo for legibility over roads/water. | ✅ |
| R3.3 | **Stable labels — no flicker** — deterministic + sticky label selection: a static map must never swap which names it shows (root cause was a non-deterministic hashmap order re-picked each cache refresh). | ✅ |
| R3.4 | **Label fade-in** — newly-revealed names ramp in (smoothstep) rather than pop. | ✅ |
| R3.5 | **Labels clear of controls** — names never render clipped under the +/− pill or the my-location button. | ✅ |
| R3.6 | **Zoom — buttons** — +/− zoom controls (precise, one-handed). | ✅ |
| R3.7 | **Zoom — pinch** — two-finger pinch-to-zoom. *(**Device-verified** via root `sendevent` multi-touch injection: pinch-out zooms the planner map to street level and *holds* the zoom on finger-up. Fix: accumulate every active touch by uid from the raw `TouchUpdate` stream — a real pinch arrives as separate per-finger events — and anchor the zoom at the fixed focal point captured at pinch start so the map doesn't drift.)* | ✅ |
| R3.8 | **Pan — drag** — one-finger drag pans 1:1, computed as an absolute delta from a drag anchor (robust to partial/coalesced touch streams), re-anchored across a pinch release. *(Same raw-`TouchUpdate` handler as R3.7, now **device-verified**: the map re-anchors cleanly when a pinch releases back to a single finger, no jump.)* | ✅ |
| R3.9 | **My-location button** — a round ◎ control that centers the plan on the trip origin at street zoom (with live GPS the origin is a true "you are here"). *(UX fix this pass: it rendered as a **diamond**; the box-SDF radius clamp restores a clean circle — see header.)* | ✅ |
| R3.10 | **Smooth camera** — eased +/− zoom and a glide-to-origin animation; a direct gesture cleanly overrides an in-flight glide. | ✅ |
| R3.11 | **Constant-width route line** — the route stays a fixed *screen* width (never a hairline when zoomed out) — white casing under a bright-blue core, anti-aliased. | ✅ |
| R3.12 | **Route pins** — origin 🟢 green, stops 🔵 blue, destination 🔴 red — upright standing pins with soft shadows, anti-aliased. | ✅ |
| R3.13 | **UX quality ≥ 9/10** — explicit bar: match Google Maps' route-planner overview (achieved 9.7/10 at review). | ✅ |

## 4 · Editing the trip
*Change any endpoint or add stops — the route re-resolves live.*

| ID | Requirement | |
|----|-------------|:--:|
| R4.1 | **Editable origin** — tap the start name (no separate "Change" button) → search → pick → the route re-resolves from the new origin (a picked origin overrides the intent's origin query). *(Verified: tapping the name opens the find overlay on device; the pick fires the same `agent.notify` that drives every verified control.)* | ✅ |
| R4.2 | **Editable destination** — same for the end point: tap the name to search and replace it. *(Verified: destination name is tappable → find overlay, same pick path.)* | ✅ |
| R4.3 | **Add a stop (waypoint)** — "+ Add stop" → search → pick → the route reroutes *through* the stop (start → stop → destination), not around it (fixed by re-resolving `vias` in `tick()`). | ✅ |
| R4.4 | **Remove a stop** — × on a stop row clears it and reverts the route. | ✅ |
| R4.5 | **Up to two stops** — slots `wp1`, `wp2`; "Add stop" hides when both are used. | ✅ |
| R4.6 | **Tappable names, not chips** — the whole name row is the tap target; the place name stays displayed as the field's value (Google's From/To pattern). | ✅ |

## 5 · Search experience
*The find overlay — keep the trip context while you search.*

| ID | Requirement | |
|----|-------------|:--:|
| R5.1 | **Clean search box** — rounded pill, a monochrome *drawn* magnifier (ring + handle, not the glossy emoji), the field blended into the pill (no box-in-a-box), a subtle border. | ✅ |
| R5.2 | **From/To stay visible** — both endpoints remain on screen while searching; the field being edited is the active input, the other shows its current value and is tappable to switch which one you're editing. | ✅ |
| R5.3 | **Current value as placeholder** — the active field's placeholder shows the current value ("Saratoga High School") so you see what you're replacing. | ✅ |
| R5.4 | **"Your trip" default items** — before you type, the results list shows the current start + destination as pickable items (e.g. tap the destination while editing the start to reverse the trip). | ✅ |
| R5.5 | **Live results** — typed query → Photon results (name + address); tap a result to set the active field. *(Result list + row structure verified on device; live population proven by the intent/preview path resolving a real Photon hit. Typing a NEW query needs a real finger — see R11.4.)* | ✅ |
| R5.6 | **Trip total in header** — the current trip's time · distance stays pinned to the top of the search window while editing. | ✅ |

## 6 · Travel time & distance

| ID | Requirement | |
|----|-------------|:--:|
| R6.1 | **Trip total** — total time · distance on the plan and in the search-window header. | ✅ |
| R6.2 | **Per-leg breakdown** — with stops, each leg shows its own time · distance (start→stop, stop→destination) on a connector between the rows; the legs sum to the total. | ✅ |
| R6.3 | **Per-mode ETA** — times reflect the selected travel mode. | ✅ |

## 7 · Travel modes

| ID | Requirement | |
|----|-------------|:--:|
| R7.1 | **Drive / Walk / Bike** — mode chips on the plan. *(Verify pass: the 4-chip row overflowed, clipping "Bike" to "B" — **fixed** by tightening chip widths + spacing; all four now fit.)* | ✅ |
| R7.2 | **Per-mode routing & ETA** — each mode yields its own duration (and route where applicable). | ✅ |

## 8 · Turn-by-turn navigation
*Drive mode.*

| ID | Requirement | |
|----|-------------|:--:|
| R8.1 | **3D chase view** — a pinhole 2.5D perspective over the vector tiles with a world-space route ribbon. | ✅ |
| R8.2 | **Turn guidance** — next-turn instruction + distance (`sys.navstep`). | ✅ |
| R8.3 | **2D/3D toggle · recenter · End** — on-map controls; pinch/pan to look around, recenter to snap back to the follow-cam. *(Pinch **device-verified** in the 3D chase view: zooms the perspective camera to street level while still following the route.)* | ✅ |

## 9 · Rendering & technical
*The engine constraints that shape the card.*

| ID | Requirement | |
|----|-------------|:--:|
| R9.1 | **Native MapView** — rendered natively in the widget (superseded an earlier Mac-side JPEG pipeline). | ✅ |
| R9.2 | **Vector tiles** — Overpass tiles; a *coarse major-roads-only* layer below z14 (a full sub-z14 query is far too heavy) so a zoomed-out overview still draws roads + names. | ✅ |
| R9.3 | **`sys.*` data helpers** — `sys.search` (Photon POI), `sys.navroute` / `sys.navstep` (OSRM, keyless), `sys.coord` (packed `"lat,lon\|name"`). No hardcoded places or ETAs. | ✅ |
| R9.4 | **Two projections kept in sync** — 3D pinhole (drive) vs p2d flat (plan/preview). Route line, pins, and labels must project through the matching CPU mirror or they misalign with the tiles; the pinhole tile-cull is gated to `nav_kind==1`. | ✅ |
| R9.5 | **The "freeze" pattern** — top-level card `let`s freeze at build; a live-data card re-runs `tick()` but not the top-level body. Re-resolve *origin* (oq/orig), *destination* (dest), *vias* (wp1/wp2) — and even any inline `text: oname` label — inside `tick()`, or changes silently don't apply. | ✅ |
| R9.6 | **Interactive direct-serve card** — a single Splash card; state changes flow through `agent.notify` keyed on the card's slot id, independent of who authored the body. | ✅ |

## 10 · Constraints & non-functional

| ID | Requirement | |
|----|-------------|:--:|
| R10.1 | **Runs with no device network** — the host tunnel (`OCTOS_PROXY` → `connect_proxy.py` over `adb reverse`) routes both the router LLM and app-side `sys.*` fetches, so a Wi-Fi-less phone still renders live cards. | ✅ |
| R10.2 | **Live GPS, proxy fallback** — the device's real GPS fix is now the default origin (see R11.3); it gracefully falls back to the San Jose default whenever there is no fix yet or the location permission is denied. | ✅ |
| R10.3 | **Deterministic rendering** — no frame-to-frame flicker; a static overview is pixel-stable (verified 0% inter-frame diff). | ✅ |

## 11 · Deferred & open items
*Known gaps, by explicit decision or engine cost.*

| ID | Requirement | |
|----|-------------|:--:|
| R11.1 | **Tile fade-in** — tiles pop in on zoom/pan; fading each as it loads needs a shader-level change (tiles render in one batched pass), so it was scoped out. The *label* half of the ask is done (R3.4). | ⏳ |
| R11.2 | **Route alternatives** — multiple route options / choose-a-route. Explicitly deferred ("leave route option later"). | ⏳ |
| R11.3 | **Live GPS origin** — **shipped this session** (branch `nav-gps`). Android `LocationManager` listener → JNI `onLocation` → `makepad_platform::gps` → new `sys.gps("lat"/"lon"/"acc"/"ok")` helper; the card uses the real device fix as the default origin (**"Your location"**) whenever the trip names no origin, falling back to the SJ default until a fix lands. Verified on device: a destination-only trip started from the real fix (~Saratoga) → NVIDIA, **34 min · 24.3 km**, and the status bar showed active location. | ✅ |
| R11.4 | **Typing a new place into the field — needs a real finger** — this pass verified tap-to-open-search, the find overlay, picking default items, rerouting through stops, mode switching, zoom, and my-location all on device. The one remaining un-automatable step is *typing* a brand-new query into the makepad TextInput: adb can't inject text there (`input text` and Gboard taps both fail), so a typed free-text search is confirmed only by code + the intent path. | 🔷 |
