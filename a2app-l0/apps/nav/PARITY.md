# nav at L0 — parity with the L2 app, requirement by requirement

The L2 nav app ships against 56 requirements in
`a2app/apps/nav/REQUIREMENTS.md`. This is what the L0 rewrite
(`exemplar.card`) does about each of them, measured on a **OnePlus 6**.

**Not a summary of intent.** Every ✅ below was seen on the device screen, or is
marked `inherited` where the requirement belongs to the widget the L0 card drives
unchanged. The point of the L0 rewrite is that its cost is visible, so a gap recorded
here is worth more than a gap papered over.

**All 56 are now mapped, and that is new.** This file claimed to be a 56-requirement
mapping while covering 30: §3 (plan-screen map quality), §9 (architecture), §10
(robustness) and §11 were absent entirely — 26 requirements with no row, which reads
from the inside exactly like 26 requirements met. Adding them found four real gaps
that nothing else had surfaced: no route pins, no zoom or my-location controls, a
plan map that does not frame its route, and no GPS fallback or GPS-defaulted origin.

Standing: **47 ✅ · 6 🔷 different-by-design, untested, or partly landed · 1 ⬜ gap ·
2 ⏳ deferred (deferred in the L2 app too)**.

Two of those rows moved after being written, and both directions are worth noting.
R3.12 (route pins) was a real gap and is fixed. R3.1 (whole-route framing) was NOT a
gap: instrumenting the widget showed the fit runs and picks a sensible zoom, and the
actual defect — a framed band anchored ~187 dp too high — belongs to the shared plan
camera and affects the L2 card equally. A screenshot supported either reading; only
the numbers distinguished them.

## The comparison

| | L2 card | L0 card |
|---|---|---|
| declarations | 664 lines | **~140 lines** |
| classification | L2 (`fn tick()`, 30 `let`, 83 assignments, 128 conditionals, 606 operators) | **L0** — no expression form at all |
| drive screen | `fn tick()` calling `ui.<id>.set_*` every frame, must never rebuild | declared sources; the camera follows a declared position |

## Measured on device (OnePlus 6, `SEED_L0_FILE`, live routing)

| scenario | CPU | RSS |
|---|---|---|
| app, no card (baseline) | ~0% | 226 MB |
| plan screen, settled | 0–3.7% | 1.0 GB |
| drive screen, fix moving, camera FROZEN (invalid) | 0–11% | 1.0 GB |
| drive screen, fix moving, camera tracking at 50 fps | 66–76% | 1.7 GB |

**The 0–11% figure was a frozen camera and should not be quoted.** A live follow
view keeps its frame pump armed, and on this device that is drawing cost, which is
what a moving map is. What the card no longer pays is the per-fix rebuild (realize +
lower + evaluate + widget tree).

### Like-for-like against the L2 card (2026-08-06)

Both cards navigating the SAME trip on the same device, driven by the same fake-GPS
track, 30 s of settle before sampling, runs ALTERNATED so tile-cache drift cannot
favour one:

| | CPU, mean of 8 | range | PSS |
|---|---|---|---|
| L2 `nav.navigate` | 77.3% | 71–86% | 1.70 GB |
| L0 `nav.card` | 79.0% | 75–82% | **1.17 GB** |

**CPU is parity** — 1.7 points apart with overlapping ranges. **Memory is 31%
lower.**

That last number is a correction, and the story behind it is the point. This file
reported the L0 card as 10% HEAVIER (1.89 GB against 1.72), which was true and was
not a fact about the architecture: the L0 card asked for `zoom: 17` on its chase
camera where the card it replaces asks for **15**. A zoom-17 tile covers a quarter
the ground, so the same view needs about four times as many, and the whole gap was in
native heap — the tile store — with graphics moving the other way. At the reference
app's own zoom the L0 path uses half a gigabyte less.

Two lessons worth keeping. A single measurement pair had them 35 MB apart and would
have been reported as parity; alternating runs showed a consistent 190 MB. And
"performance does not match" was a CARD asking for something different, not a
pipeline costing more — the breakdown said native heap, and native heap said tiles,
and tiles said zoom.

The plan screen still settles to **0%**, so the cost is confined to driving.

## §1 App identity & routing

| ID | | note |
|----|:--:|---|
| R1.1 first-class app | ✅ | registered in `L0_APPS` |
| R1.2 direct-serve | ⬜ **deliberately not** | the L0 card is GENERATED from `app.md` + this exemplar. R1.2 exists because the on-device model truncates a 664-line card; ~140 lines is inside what it writes reliably, which is the point of the rewrite |
| R1.3 AMA intent routing | ✅ | unchanged — routing is the app's, not the card's |
| R1.4 "from A to B" | ✅ | the router seeds `origin`/`dest` state |
| R1.5 intent seeding | ✅ | a named trip opens on its route, not an empty box |

## §2 Screens & flow

| ID | | note |
|----|:--:|---|
| R2.1 search | ✅ **works end to end** | four defects, each of which alone made it inert. `query` was only ever CLEARED so the search always ran on `""`. The results panel was guarded on `dest == ""` while the field that fills it binds to `dest`, so results could only show before anything was typed. The host never put fetched ROWS in the data — a `for` iterates the data and a list's length is structural, so the loop had nothing to walk. And the rows carried `f.name`, so picking the third "Stanford" set state to "Stanford" and routed to the first.<br><br>Fixed in that order. `sys.search` now admits `label` and `query` — both of which the helper had always answered — so a row shows WHICH Stanford and carries the text that finds that one again. Verified on device: five results (Palo Alto, Kentucky, England, Montana), tapping Kentucky sets TO to "Stanford, Kentucky, United States" and routes **2585 min / 3951.9 km**, which is California to Kentucky |
| R2.2 preview | ✅ **fixed this pass** | a screen of its own now: the chosen trip framed, its duration and distance, and one button that commits — and none of the editing controls, since a preview that still offers a search box is the planning screen under another name. It cost a third enum member and two guarded branches, because a trip through a stop is a different trip. ONE event still drives the whole journey: `cycle(.plan, .preview, .drive)` names an order, and the order IS the flow, so "Go", "Start" and "End" are one transition seen from three places rather than three events that could disagree about which screen follows which. Walked on device with real taps: plan → Go → preview (30 min, 27.6 km away, Start) → Start → drive (turn banner, 31 min) → swipe → End → plan |
| R2.3 plan | ✅ | |
| R2.4 find overlay | 🔷 **different by design** | both endpoints are always-visible `Field`s, so editing is in place rather than in an overlay. Same capability, fewer screens |
| R2.5 drive | ✅ | verified end to end: 27.6 km → 17.5 km → "Arrived at destination", 0 m |

## §3 Planner map · §9 Rendering · §10 Constraints

**All ✅, and none of it is the card's.** R3.1–R3.13, R9.1–R9.4, R10.1–R10.3 are
`MapView` behaviour — labels, pinch, pan, my-location, eased camera, constant-width
route line, pins, vector tiles, both projections, determinism. The L0 card names a
trip and inherits every one of them. That is the whole argument for a role: `Map`
is a declaration, and the quality lives in the widget where it can be shared.

Two of them needed the card to stop getting in the way, and both were device-found:
every `MapView` needs `use_network: true use_local_mbtiles: false` (without it the
map drew the land fill and nothing else), and a card holding a map must lay it out
as the bottom layer of an overlay (in a column it drew straight over the card).

## §4 Editing the trip

| ID | | note |
|----|:--:|---|
| R4.1 editable origin | ✅ | |
| R4.2 editable destination | ✅ | |
| R4.3 add a stop | ✅ **control now device-verified** | it was "outcome verified, control not": the trip through a stop had only ever been proven by SEEDING `stop`, which exercises the routing and never touches the control. A review had already found the original chip could not work at all — it sent `value: ""`, and an empty value becomes no payload. It is a `Field` now, and the FIELD's own event has been exercised on device with a payload: VIA reads "Palo Alto", the `Remove` chip appears (guarded on `stop != ""`), the sources go stale, and the trip redraws at **33 min / 31.2 km** against 30 / 27.6 direct, with legs 29 min · 29.3 km — VIA — 4 min · 2.0 km summing to the total |
| R4.4 remove a stop | ✅ | `Remove` chip carries no payload and needs none — `stop: clear` |
| R4.5 up to TWO stops | ✅ **fixed this pass** | the first attempt was reverted because `Map(via:)` named ONE source: the line went through one waypoint while the duration beside it was for a trip through two — the drawn-versus-reported mismatch this profile exists to catch, and invisible in a screenshot where both are plausible lines.<br><br>`Map` has a second slot, `via2`, and not a list. Role arguments route through the expression grammar, so admitting `[a, b]` there is a change to the whole grammar for one argument; the app being replaced has exactly two waypoint slots and hides "add stop" when both are full. `sys.route`'s `via:` already carried N pairs.<br><br>The test counts SEPARATORS in the route call alone — both stops appearing somewhere is not the claim, the claim is that the helper is handed two waypoints. Verified on device: Saratoga → Cupertino → Mountain View → Stanford, **38 min / 28.7 km** against 30 min / 27.6 km direct, with a green origin pin, TWO blue stop pins and the line detouring through both |
| R4.6 tappable names | 🔷 | a `Field` shows the value and accepts a replacement in place |

## §5 Search experience

| ID | | note |
|----|:--:|---|
| R5.1 clean search box | 🔷 | rounded field, no drawn magnifier |
| R5.2 From/To stay visible | ✅ | always, which is stronger than "while searching" |
| R5.3 current value visible | ✅ | it is the field's value |
| R5.4 "your trip" default items | ✅ | the defaults row renders and is tappable, and what a RESULT row carries is now tested on device: `f.query` — name plus label — which re-finds that hit and not the first one sharing its name |
| R5.5 live results | ✅ **fixed this pass** | `Field` admits `on_change` as well as `on_commit`, because the two are different questions: a keystroke asks "what am I looking for", a return says "this is where I am going". `TextInput` has always called both; only the catalog was short one. Verified on device — the partial word "Stanf" listed "Nelson Road, Stanford, California" and "Stanford, Kentucky" with the destination still EMPTY and no route drawn, which is the distinction the two events exist for.<br><br>I nearly rejected this on a remembered number: a re-resolve per keystroke sounded like the 327 ms map rebuild. That figure is the DRIVE screen, which has a follow camera and a route to re-tessellate. Measured on the planning screen it is **18–19 ms**, and that is what makes as-you-type expressible rather than merely declarable |
| R5.6 trip total in search header | ✅ | the total is always on screen |

## §6 Time & distance · §7 Modes

| ID | | note |
|----|:--:|---|
| R6.1 trip total | ✅ | |
| R6.2 per-leg breakdown | ✅ | verified: 29 min·29.3 km + 4 min·2.0 km, summing to 33 min·31.2 km |
| R6.3 per-mode ETA | ✅ | verified: Drive 30 min → Walk 331 min, distance unchanged |
| R7.1 drive/walk/bike chips | ✅ | on their own row — a four-chip row clipped "Add a stop", the identical defect R7.1's own verify pass found |
| R7.2 per-mode routing | ✅ | `mode:` now reaches the helper. It was accepted and dropped, so every mode showed the driving time |

## §8 Turn-by-turn

| ID | | note |
|----|:--:|---|
| R8.1 3D chase view | ✅ | `Map(view: .tilted)` — verified: tilted horizon, world-space ribbon, puck |
| R8.2 turn guidance | ✅ | `sys.step`, and progress is MEASURED. The L2 card fed `sys.navstep` a clock (`sys.navsecs × 15.2`), so it announced turns from a parked car |
| R8.3 2D/3D toggle · recenter · End | ✅ **complete** | all three, by three different routes. `End` is a `Chip` in the swipe sheet. Recenter is `Map(controls: .all)` — the card names the affordance and the backend emits the call. And the 2D/3D toggle is card STATE: `view:` admits a path as well as a token now, like `unit` and `width`, so `Map(view: view)` follows a `state view { shape: enum[tilted, flat] }` and the toggle is a state, an event and two chips.<br><br>The alternative was guarding a whole `Map` per value, which multiplied with the `origin` branches and cost 18 lines for one choice — the card went to 236 against a 230 bound, and that was the signal. Admitting a path costs 3 lines and no branches. Verified on device: the chip reads "2D" over a tilted map and "3D" over a flat one, and tapping it flips both.<br><br>**That verification was wrong for one build, and the way it was wrong is the point.** The chip relabelled on every tap and the camera never moved. Its label comes from a guard reading `view` directly, so the half that is easy to see was live while the half that matters was dead — I read the label flip as proof of the whole. `view:` admits a path, and REALIZE ERASES WHICH FORM WAS USED: a written `.tilted` survives as `Token`, a followed state arrives as `Text`, and every reader in the lowering matched `Token` alone, so `Map(view: view)` lowered to the flat camera on both settings. Fixed by one `token_arg` helper that both modules read every `TokenOrPath` argument through — `view`, `unit`, `width`, `controls`, `range` — because the bug was the class, not the argument. `a_token_argument_may_be_written_or_followed` asserts both spellings of all three |

## §3 Plan-screen map quality

Mostly the WIDGET's, not the card's — and that is the refactor working: the card
shrank and the host kept the capability. "Inherited" below means the L0 card drives
the same `MapView` with the same properties, so the requirement holds for the same
reason it held before. Where it does NOT hold, it is because the L2 card drew the
thing ITSELF, in a way L0 cannot express.

| ID | | note |
|----|:--:|---|
| R3.1 whole-route framing | ✅ **inherited, and my note was wrong** | I recorded this as "fits, anchored wrong — the band sits ~187 dp too high". Reading the camera rather than the screenshot: it fits the route into `rect.size.y * 0.50` and centres that band in the TOP of the map deliberately, because the summary sheet overlays the bottom — "so BOTH endpoints show" is the code's own comment. Measured pins at 32 dp and 418 dp of an 812 dp map are exactly that band. What I read as clipping is the destination pin sitting under the system STATUS BAR, which is a different and much smaller thing than the fit being wrong, and is not what this requirement asks about |
| R3.2 street & place labels | ✅ inherited | verified on the L0 plan screen: "Page Mill Road", "Bayshore Freeway", "West Valley Freeway", "85", "20", "22", haloed |
| R3.3 stable labels, no flicker | ✅ inherited | a settled L0 plan screen measures 0.0% inter-frame diff |
| R3.4 label fade-in | ✅ inherited | widget |
| R3.5 labels clear of controls | ✅ **fixed** | the diagnosis in the note this replaces was wrong in an instructive way. It said the widget's keep-out "should fire by arithmetic and does not", and by arithmetic it should — but it is not the code that draws that label. `draw_nav_labels`/`draw_nav_labels_plan` place the NAV names; "…yshore Freeway" is a base-map name, placed by the vector-tile label engine with its own `scratch_accepted_bounds` and no keep-out at all. Two collision systems, one of them told about the chrome. Both now reject a placement in the control corner through one shared `nav_label_under_controls`, and the base-map engine only applies it while `nav_kind() != 0` — a routed map has a control column in both cards, and every other map in the app has none, so reserving a corner of those would drop names for buttons nobody drew. The corner also grew from 250 to 300: the column used to sit at a fixed offset inside the map and now flows under the instruction banner, reaching ~285 with a 2D/3D chip under the ring |
| R3.6 zoom buttons | ✅ **fixed this pass** | `Map(controls: .zoom \| .all)` is the declared capability this needed. The card NAMES the affordance; the theme draws the pill and the backend emits `ui.l0map.nav_zoom_by("0.7")` — the same 0.7 step the L2 card uses. A test asserts no lowering of a card contains `ui.` at all, which is the half that matters: the L2 form is a card writing that call itself. Verified on device — two taps of `+` took the map from freeway shields to street names |
| R3.7 pinch-to-zoom | ✅ inherited | the widget's own raw `TouchUpdate` handling; no card involvement |
| R3.8 drag-to-pan | ✅ inherited | same handler |
| R3.9 my-location button | ✅ **fixed this pass** | comes with `controls: .all`, wired to `set_nav_recenter`. Verified on device: after zooming in twice, one tap returns the map to within **1.3%** of the original fitted overview (against 49.7% different from the zoomed state). The ring is DRAWN as two circles rather than typed — "◎" is U+25CE and the bundled Roboto has no glyph for it, so it rendered a tofu box; the L2 card gets away with the character only because it never names a font |
| R3.10 smooth camera | ✅ inherited | including the eased +/− and glide-to-origin, now that R3.6/R3.9 reach them: `nav_zoom_by` and `set_nav_recenter` set the animation targets the plan camera eases toward |
| R3.11 constant-width route line | ✅ inherited | `nav_route_width: 40` is emitted for a plan map; verified visually — constant-width core with white casing |
| R3.12 route pins | ✅ **fixed this pass** | `nav_markers` was filled ONLY by `set_route_markers`, a method call, so on the L0 path it stayed empty and `draw_nav_route_pins` returned on its first line — a correct route with no pins, which looks like nothing at all is wrong. `Attrs::markers` is now a value carried on the node and `route_markers` a live property, derived from the SAME resolved endpoints as the polyline so a pin cannot mark a place the line does not pass. Verified on device: green origin at the route's start, red destination at its end. A CHASE map carries none — R3.12 is a plan-screen requirement, a follow camera already draws the puck, and in 3D the widget appends pin geometry to the ribbon rather than drawing it separately: a `follow3d` map handed markers rendered no route and no tiles at all. That cost a build, and the reason it got through is worth keeping: I verified pins on the screen the requirement names and not on the other one |
| R3.13 UX quality ≥ 9/10 | 🔷 **not re-scored** | pins and controls have landed since this was written, so the two named reasons it could not score are gone. It still has not been scored, and I am not going to score my own screen |

## §9 Architecture

| ID | | note |
|----|:--:|---|
| R9.1 native MapView | ✅ inherited | the same widget |
| R9.2 vector tiles, coarse layer below z14 | ✅ inherited | `use_network: true use_local_mbtiles: false` emitted |
| R9.3 `sys.*` helpers, no hardcoded places | ✅ **stronger** | the card DECLARES sources and the host answers them. A hardcoded place is not merely absent, it is unsayable: §4 refuses a literal where a measurement belongs |
| R9.4 two projections in sync | ✅ inherited | `nav_kind` 1 for `follow3d`, 2 for `plan`/`follow` — the same gate |
| R9.5 the "freeze" pattern | ✅ **this is what the backend emits** | top-level `let`s freeze at build, so the place lookups hoist there and only `sys.gps` stays inside `fn tick()`. The L2 card had to write that by hand and R9.5 exists because getting it wrong is silent; here it is the compiler's output, and a test asserts the card itself holds no `tick` |
| R9.6 interactive direct-serve | ✅ | taps reach the host on `agent.notify` keyed by the card's item id — verified today for `pick_mode` and `go` |

## §10 Robustness

| ID | | note |
|----|:--:|---|
| R10.1 runs with no device network | 🔷 **untested this pass** | the `OCTOS_PROXY` + `adb reverse` path still exists and is unchanged; everything here was measured over the phone's own Wi-Fi |
| R10.2 live GPS with fallback | ✅ **fixed, on the second attempt** | `sys.gps` answers **-9999** for a coordinate it does not have, and a camera aimed at that is a blank map off the coast of Africa with the trip drawn nowhere near it. My first fix guarded the card on `here.ok`, and that is the one mistake this card already carries a note about: a guard is evaluated at REALIZE time, so it can only test what realization can see — declared state, or a source's `$state`. A live value reads nothing on a freshly generated card, so BOTH branches were false and the drive screen lowered **with no map at all**. The test passed because it seeded `here`, which is exactly the case where the bug is invisible. The sentinel is refused in the WIDGET now (`is_a_place`), where the fix and the route are both known, and a position that is not a place puts the camera at the start of the route. The replacement test omits `here` entirely |
| R10.3 deterministic rendering | ✅ | 0.0% inter-frame diff on a settled plan screen |

## What is actually left

**The host now writes fetched values into a card's data** — list rows
(`fetched_rows`), which was the gap behind R2.1 and R5.4, and the device's own scalars
(`fetched_scalars`), verified by a card with an EMPTY data blob capturing lat and lon
from the phone. It asks the capability how many hits there are and
writes one row per hit carrying only its KEY — identity, never a fact, exactly as
`with_durable` already did for saved collections. Everything else a row shows still
lowers to a live call.

That is the narrow half of §5.9. The broad half is still absent: fetched SCALARS are
not written back either, which is why R11.3 cannot capture the device's position —
there is no fix in the data to capture, at realize or at a tap.

So the remaining work is:

Nothing on the list is a nav feature any more. What remains is R1.2, which is a
deliberate non-goal — the L0 card is GENERATED rather than direct-served — and two
items deferred in the L2 app as well.

## §11 Deferred

| ID | | note |
|----|:--:|---|
| R11.1 tile fade-in | ⏳ same as L2 | scoped out there too — needs a shader change, since tiles render in one batched pass |
| R11.2 route alternatives | ⏳ same as L2 | deferred there too. Would be N route sources: expressible, not built |
| R11.3 live GPS origin | ✅ **fixed on the fourth attempt** | three failures first, each a different disguise of the same question — WHEN is the value taken. Referencing the fix (`from_lat: here.lat`) makes the route chase the driver and pins progress at zero, since `navprog`'s route-start and device-position become the same expression. `initial:` alone fires before the first fix and freezes the -9999 sentinel. Capturing on the Go tap is right after the tap and wrong before it: the cell is unwritten, so the initial re-resolves every realization and the origin follows the device.<br><br>The missing rule was small once named: **an `initial:` taken from a SOURCE is a capture, and someone has to write it down.** Realization now reports what it took (`RealizeReport::captured` — it owns the precedence) and the host writes it once (it owns the store). Until written it re-resolves, which is what made a state follow its source. And `sys.gps` is no longer answered into the data at all when it has no fix: absent is honest, and it stops a card capturing a coordinate nobody has.<br><br>Verified on device, stationary — which is what planning a trip actually looks like: a destination-only trip shows FROM empty, TO "Stanford University", **30 min / 27.6 km away**, with the route drawn from the device's own position and both pins on it |
| R11.4 typing a new place | ✅ **device-verified, by finger-path** | the caveat this row carried since the first pass — "adb cannot inject text into a makepad `TextInput`" — is retired: the verification taps GBOARD'S OWN KEYS by coordinate, so the text travels the real IME composition path (InputConnection → `full_state_sync`), not the hardware-key shortcut `adb input text` takes. Typing c-u-p-e-r into FROM echoes per key, fires `changed=["query"]` per key, lists Cuperly/Cuper/Cupertino live, and picking one reroutes. The same flow works for TO and the via stop. What it took is recorded in the aichat commit this pins: the makepad UI thread was DYING on the first tap of any card field (a cross-isolate use-after-free in the animator), and after that three IME-session bugs in a row — focus orphaned by the per-keystroke rebuild, the cursor snapping to 0, adoption echoing stale text at the keyboard |

## Camera motion — how it was actually verified

Twice I reported this working when it was not, so the method matters as much as the
result.

| attempt | result | what was wrong |
|---|---|---|
| baked centre, ~15 cards in the conversation | "46% changed → MOVED" | several follow-map instances; the frames compared were not the instance being reasoned about |
| `center_lat: sys.gps("lat")` as a live expression | 0.0% over 21 fixes | **nothing re-evaluates a widget property.** The centre became a constant, and the epoch threshold had just been raised on the same wrong theory |
| fix read per frame, track from `geometries=geojson` | 0% then 32% in jumps | the track's line (28041 m) was not the widget's (27577 m), so fixes sat off the route and the projection landed erratically |
| fix read per frame, track decoded from `polyline5` | 30.7 / 0.0 / 0.0 / 28.1 / 5.2% | the track routed between coordinates I chose; the app routes between the GEOCODER's, so the fixes still sat off the widget's line |
| track from the app's own Photon endpoints | 24.8 / 27.7 / 16.0 / 9.1 / 10.9 / 11.5%, 0 dead of 6 | nothing about the route — but the pump had to stay armed. `follow_moved` is computed in the DRAW path, so once the settle tail expired nothing could notice the next fix; the camera parked until a card re-resolve every 40 m. Bursts of 30% separated by exact 0.0% is what "static" looked like |
| easing between fixes | **6.6 6.1 6.9 6.7 6.9 7.0 6.5 5.3 5.2 5.6 4.8% back-to-back, 50 fps** | nothing — smooth |

What the failures had in common: a static camera and a working one produce the same
screenshot unless something varies between frames that you control. The last row is
the only one where the fix positions came from the same geometry the widget draws.

**A static map is often correct.** The plan screen is a route preview and never
moves. The drive screen moves only when the device does — on a desk it is right to
sit still — and a finished synthetic track looks identical to a broken camera.

## The stutter, and why it is architectural

Reported three times as "一顿一顿的", and correctly. The camera is smooth and the
frame rate is 50 fps, so the jank is not the camera — it is that the map **stops**.

Measured on a OnePlus 6 while driving: frame hitches and card re-resolves correlate
**1:1**, and the hitches were 40, 43, 101 and **327 ms**. A third of a second of
frozen map is the lurch, and no amount of camera smoothing hides it, because what
stalls is the whole UI thread.

**Every value change escalates into a structural rebuild.** An epoch change
re-resolves the card — realize → lower → evaluate → rebuild the widget tree — and it
runs on the UI thread, inside a frame. The epoch is bumped by GPS movement *and by
every completed `script_data_fetch`* (`res.rs`'s `finish_data_fetch`), so routes,
places and retries all land as rebuilds too.

Rate-limiting the GPS source was tried and **did not work**: 4 re-resolves in 50 s at
both 40 m and 250 m, because that source was never the main one. It was reverted
rather than kept as a cost that bought nothing.

### Camera smoothness — four attempts, each a different kind of jank

Screenshot sampling could not see any of this: a capture interval spans several
fixes, so it averages the pulse away and reports "smooth" every time. Only a
per-frame log of the camera's own position showed what was happening.

| how the camera followed | per-frame advance | what it looked like |
|---|---|---|
| snap to each fix | one step per fix | a visible lurch, ~1.4 Hz |
| ease toward the fix (`+= diff * 0.12`) | large then decaying | smooth in POSITION, lumpy in VELOCITY — pulses at the fix rate |
| advance at the measured rate | uniform, then 0 | uniform until it catches the latest fix, then stalls against the clamp |
| **interpolate the last two fixes, one interval behind** | **0.182 m mean, stdev 0.029, 1 stalled frame in 984** | smooth |

The last one has no rate to estimate, no target to catch and no clamp to hit: there
is always a closed segment underfoot, so the sweep is exactly linear. It never leaves
the measured path — every rendered position is between two real fixes — which is the
line the simulated drive mode crosses. The cost is a one-interval delay, and a camera
a second behind the truth is honest where one ahead of it is invented.

Residual variance (0.17-0.23 m per frame) is frame pacing at ~49 fps, not the camera.
Reducing it means reducing render cost, which is a different problem.

### Copying the old app's tick — reverted once, then shipped

**The numbers in the table below are wrong about why, and the tick is enabled now.**
A review caught both. `fn tick()` is started by the host on a **1-second interval**
(`aichat/widgets/src/splash.rs`, `start_interval(1.0)`), not per frame — so the 644%
CPU I attributed to per-frame evaluation cannot have been that. It was almost
certainly the startup tile load, which I only learned to wait out later in the same
session: the settled figure for that build was ~79%.

Hoisting the constant lookups is still right — it removes redundant work and it is
what the old card does — but it was not the performance fix I claimed, and the
measurement below does not support the claim it was attached to.

### The original attempt, for the record

The mechanism was implemented exactly as `trip-planner.splash` does it: name the live
text widgets (`l0v0 := Label{…}`) and set them from `fn tick()`. It worked — the
instruction and the distance updated in place, and the 327 ms stalls became 46-62 ms.

It was still reverted, because it was a net loss:

| | hitches in 50 s | worst | CPU |
|---|---|---|---|
| before the tick | 4 | 327 ms | 71% |
| with the tick | 27 | 315 ms | **644%** |

`fn tick()` runs every frame, and the expression emitted into it was
`sys.navstep(searchnum×4, sys.navprog(searchnum×4, gps×2), "instr")` — eight geocoder
lookups and a route projection, sixty times a second. Memoising the polyline decode
did not dent it. The old app's tick is cheap because its calls are `sys.coord` on an
already-resolved string; this one re-resolved the place names every frame.

**The mechanism is right and the expression is wrong.** The endpoint coordinates do
not change while driving, so the tick should carry them as LITERALS and keep only
`sys.gps` live:

```
ui.l0v0.set_text(sys.navstep(37.2656, -122.0294, 37.4313, -122.1694,
                             sys.navprog(37.2656, -122.0294, 37.4313, -122.1694,
                                         sys.gps("lat"), sys.gps("lon")), "instr"))
```

The kit has those numbers — it evaluated the searches — but emits the call text
rather than the value, so bridging that is the remaining work. Nothing about L0
blocks it: `fn tick()` is the backend's, exactly as `sys.navstep` is, and the card
still says only `TextRow(text: step.instruction)`.

**So the fix is to update a changed value in place instead of rebuilding** — with
constants baked, not re-resolved per frame. That is
exactly what the L2 app did — `ui.instr.set_text()` from `tick()` — and why its
contract states, in capitals, never to introduce anything that forces a rebuild while
driving. R9.5 and R8.3 are both about this. The machinery exists in
`widget_tree.rs` (`test_property_patch_no_structural_rebuild`); what is missing is
routing an L0 re-resolve through it when only values changed, rather than swapping
the tree. Two of that module's tests are currently failing on this branch, unrelated
to this work.

Until then: the camera is smooth, and the card hitches when data lands.

## Where L0 is ahead

- **The turn banner is honest.** R8.2's L2 implementation drove `progress_m` from a
  looping clock times an assumed 34 mph, so it announced turns and arrived on
  schedule from a stationary vehicle. `sys.step` takes the device's own coordinates
  and the host projects the fix onto the route.
- **The camera is honest.** The widget's `2d`/`3d` modes drive a simulated vehicle.
  `follow`/`follow3d` move only when the device does.
- **CPU parity and 31% less memory**, measured like-for-like with alternating runs:
  79.0% against 77.3%, and 1.17 GB against 1.70 GB. The earlier "10% heavier" figure
  was the card asking for a closer chase camera than the one it replaces. See the
  table.

## Remaining, in priority order

1. **R2.2 preview** as its own step — a screen state plus a guard. All of its
   content (ETA, distance, a commit button) is already on screen; what is missing
   is the separate step.
2. **R4.5 second stop** — blocked on a list-valued constructor attribute, not on
   anything about L0's totality. One more route source is easy; `Map(via: [a, b])`
   is the missing piece, and without it the drawn route disagrees with the
   reported one.
3. **~~R8.3 on-map 2D/3D toggle~~** — landed as `Map(view: view)` over a declared
   `state view`, no widget control needed. Kept here only to record that it stood
   on this list because a `TokenOrPath` argument read as `Token` alone, so the
   toggle worked in the written form and silently did nothing in the followed one.
