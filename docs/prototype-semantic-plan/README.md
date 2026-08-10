# Semantic-plan prototype — results

*2026-07-30. The proof-or-kill test for "the LLM emits intent, the runtime emits
the card". Everything here ran on real devices against live data.*

The idea, modelled on octos's DOT pipeline (`octos/examples/dora-bridge-config/`):
the LLM emits a small **typed plan** over a closed block vocabulary; the runtime
**lowers** it to a real card. DOT does exactly this — a closed set of handler
kinds, `dora_tool_map.json` binding abstract names to concrete tools, and the
runtime owning the constraint language. The runtime is deliberately dumb.

## What is here

| file | what |
|---|---|
| `blocks.json` | the registry — closed vocabulary, MCP-shaped typed declarations |
| `theme-immersive.json` | the `dora_tool_map.json` analogue: every value the model used to be asked for |
| `plan-kyoto.json` | a weather plan, 604 bytes |
| `plan-stocks.json` | a stocks plan, 893 bytes |
| `expand.py` | plan → makepad Splash DSL (holds the shared validator) |
| `expand_oh.py` | plan → OpenHarmony/ArkUI DSL |
| `expand_stock.py` | stocks plan → makepad Splash DSL |

```
python3 expand.py       plan-kyoto.json  blocks.json theme-immersive.json > card.splash
python3 expand_oh.py    plan-kyoto.json  blocks.json                      > card.splash
python3 expand_stock.py plan-stocks.json blocks.json                      > card.splash
```

## Result 1 — the runtime does NOT need to be smart

817 lines of Python across three lowerings, and **not one heuristic**. It is
validation, table lookup and string building. The intelligence is spent once when
a block is written, not per request — exactly as DOT spends it once writing the
`codergen` handler.

604-byte plan → 27,703-byte card. A 46× reduction in what the model must get right.

## Result 2 — one plan drove two backends

The same `plan-kyoto.json` rendered on **makepad** (OnePlus 6T) and **native ArkUI**
(Huawei Mate 70 Air). Those dialects share nothing:

| | makepad | ArkUI |
|---|---|---|
| nodes | `Label{…}` `RoundedView{…}` | `{t:"text"}` `{t:"column"}` plain records |
| colour | `draw_text.color: #ffffffe6` | `argb(230,255,255,255)` — hex evaluates to 0 |
| attrs | `width`/`height`/`padding` | `w`/`h`/`pad` |
| data | `sys.weather(lat, lon, path)` | `fetch_fmt(URL, path, idx, unit)` |
| place | resolved at render time | resolved at lowering time — no helper there |

Both produced the same city, temperatures, forecast rows, AQI and UV.

**The porting cost is the host capability surface, not the widgets.** makepad
exposes thirty-odd `sys.*`; Splash-OH injects five (`fetch_num`, `fetch_fmt`,
`fetch_weekday`, `invoke`, `sget`). So `SunMoon` is not *degraded* on ArkUI, it is
*impossible* — nothing there yields a moon phase or a daylight fraction. It renders
an explicit "unavailable on this backend" surface rather than being dropped.

## Result 3 — it generalises to a second domain, and shows exactly where it stops

A stocks plan renders the movers list with live data (HURN $170.37 +40.37%, …).

- **Blocks: 0 of 5 reused.** Expected — blocks *are* the domain.
- **Infrastructure: reused entirely.** Same validator (imported, not copied), same
  text roles, same theme-token shape, same invariant rules.
- **Three new schema concepts were forced**, none of which weather needed:
  **state**, **actions**, **views**. Weather is a pure function of (place, locale)
  rendering one screen; stocks has a user-changed ticker and range, and two screens
  chosen by that state.

That third point is where the honest limit sits. A plan can *declare*
`onTap: select`, and octos-one already has the mechanism it lowers to —
`agent.notify("set", {…})` to write and `let x = "{{state.x}}"` to read, with no
LLM round-trip. What does **not** exist is identity-preserving update: writing a key
**re-renders the whole card body**, which destroys widget instances. The host
already works around this by suppressing re-evaluation for `fn tick()` cards
because rebuilding destroys `MapView`. Keyed patching is the remaining project.

## Kill criteria — none met

The test was set up to fail on any of: needing raw spacing/colours in the plan; the
expander accumulating per-card special cases; the schema growing mainly by
mirroring widget properties. None happened. The schema grew by three *principled*
concepts, and the one real defect found was mine — the validator was
weather-shaped and rejected a valid stocks plan, which is why plan shape is now
declared in the registry rather than assumed.

## What it fixed in the real product

Every bug the prototype was built to prevent was a live bug, and each is now fixed
in octos-one itself rather than only in the prototype:

| bug | why the model got it wrong | fix |
|---|---|---|
| 71 hallucinated coordinate pairs per card | recalled from training data | `sys.geocodenum` |
| `wmin: 10 wmax: 35` for a 27–39° week | the temperatures are a live fetch it cannot see | `sys.weekmin`/`sys.weekmax` |
| weekdays off by one, in **every** card | does not reliably know the date | `sys.dayname` |
| 上海/多云 as tofu boxes | an explicit `font_family` REPLACES the CJK chain | text roles |
| all data `n/a` on Chinese cards | `geocode` hardcoded `language=en`; open-meteo indexes per language | script-detected language |
| `draw_bg.a0`…`a15` in card text | shader-only widget had no `draw_walk` to fetch in | `AqiContour` is a Rust widget |

The common shape: **the spec asked the model for something it could not verify.**
It writes blind — it never sees the rendered result — so every such request failed
silently. The things it got right (which city, which icon, which words, which
language) are all things it could reason about without seeing output.
