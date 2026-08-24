# Plan: give L0 the expressive range the HTML twin has

**Status: revised after review (`REVIEW-codex.md`). Phase 0 is safe to start;
the accent implementation is gated on it.**

## The problem, measured

120 specimens through mockup → {HTML twin, L0 card}, each judged against the
mockup (`FINDINGS.md`). HTML twins reach **6.67** mean fidelity, L0 cards
**2.76**. The gap is not design comprehension — the same model wrote both from
the same image — it is **vocabulary the profile does not have**:

| missing | judge named it in | mean score when named |
|---|---|---|
| colour / accent | 92% | 2.8 |
| serif / display face | 62% | 2.8 |
| icon style | 52% | 3.0 |
| card & panel chrome (border, elevation, texture, rules) | 34% | 2.8 |
| truncation / ellipsis | 31% | 3.0 |
| letterspacing / caps | 23% | 3.0 (shipped: `TextEyebrow`) |
| chart / sparkline | 15% | 2.6 |
| gradient fill | 13% | 2.3 |

**Read those percentages as complaint frequency, not as expected lift.** Fixing
accent removes one complaint from 92% of cards; it does not raise 92% of cards.

Root cause of the top line: `THEMES = [dark, light, glass, photo]` and the
constructor catalog has no colour vocabulary at all. All 120 generated cards
could only declare `light` (77), `dark` (24) or `photo` (15). **Style diversity
is hard-capped at four looks**, whatever the generator does.

## The principle this must not break

§1.1: *a card names ROLES and never presentation.* That is what keeps a card
re-themeable by the host. The fix must NOT put colours, fonts or pixels in cards.

The opening already exists: `theme <mood>` is a card naming an **intent** from a
closed vocabulary. This plan widens that one line; nothing else about the
contract changes.

## Proposal: the theme line gains axes

    theme photo                                   # today, unchanged
    theme light accent: .amber                    # phase 1
    theme light accent: .amber type: .serif       # phase 4
    theme light surface: .cards density: .airy    # phase 5

Every value is a TOKEN from a closed set. A card still cannot say `#ff6a00`,
`16px`, or `Georgia`.

## Scope correction: this is a Makepad-path pilot

The plan originally claimed three-backend parity. **Verified false.**
Splash-OH depends on `splash-core`, not `splash-ui-l0` or the shared
`splash-node` (`Splash-OH/crates/splash-oh-native/Cargo.toml:16`), and
Splash-Android's shipping path states "No DSL is involved on this path at all"
(`Splash-Android/catalog/rust/src/plan.rs:6`) with its own hardcoded theme.
Neither consumes this kit today. Axes are therefore a **Makepad-path feature**
until those backends adopt the L0 pipeline — stated up front rather than
discovered later.

## Architecture: axes as ordered palette deltas

The host assembles `PALETTE_BASE + mood delta + PALETTE_DERIVE + KIT_BODY`
(`l0_card::kit_for:69`), and the order is load-bearing: deltas precede derive so
a delta's factor change propagates. Axes slot into the same chain:

    BASE + mood + accent + type + surface + density + DERIVE + KIT

Rebinding works — `_palette_photo` already redefines `l0_scrim`, `l0_fill` and
`weight_semi` from the dark base, in production. But **rebinding is last-wins,
not composition**: two deltas writing the same `let` silently resolve by order.
So each axis must own a disjoint token set, recorded in a **token-ownership
table** and enforced by a test that asserts no two axis deltas write the same
name. (Assignment ≠ rebinding in this VM: `fill = …` fails, `let fill = …`
shadows — `_kit.splash:408`.)

### Output identity, stated correctly

The earlier claim "existing cards render byte-identically" was wrong as written.
Today's roles read heterogeneous tokens — `l0_active` (selected chip),
`l0_soft` (eyebrow), `l0_go` (primary action), `l0_up`/`l0_down` (direction),
and `l0_bar == 0` meaning "legacy multicolour bar" in three of four moods.

The rule that actually preserves output: **an omitted axis emits no delta, and
an accent delta REBINDS those existing role tokens** rather than introducing an
`l0_accent` that roles read unconditionally. No card that omits `accent:` can
move, because nothing in its assembled kit changed.

## Phases, reordered after review

**Phase 0 — the contract (safe to start now).**
Grammar and data model, no visual change: a `ThemeSpec` replacing
`Option<(String, line)>`; parser support for same-line axes (the lexer discards
newlines — `lib.rs:481` — so "same line" must be enforced via `Token.line`, not
by grammar shape); a normative axis catalog with code/spec agreement tests
(today the profile documents only `theme dark`, and the constructors TOML is
explicitly only the argument contract); `card_theme()` → `card_theme_spec()`
with its one consumer updated (`l0_card.rs:70`); host override resolution
(the existing light-over-Photo substitution generalized); the render-cache key
extended to include the effective spec (`main.rs:5124` keys on item, message,
card state and fetch epoch — a theme override would otherwise serve a stale
tree); the token-ownership table and its test; and **immutable per-mood
baselines captured before anything ships** (the four device goldens are
weather/stock-list/stock-detail/news — card goldens, not mood goldens — so mood
baselines must be created, not assumed).

**Phase 1 — accent, Makepad-only pilot.** Palette deltas rebinding the existing
role tokens; a legacy branch when the axis is absent; the accent-consuming role
list written down (temp bar, rank/lead numerals, selected chip, links, eyebrow)
before code.

**Phase 2 — truncation.** Promoted: it is the most bounded slice and half-built.
`TextRow` gains `lines`, `Attrs.lines` already exists but the Makepad emitter
never reads it, and the Label widget already supports `max_lines` + ellipsis
overflow (`label.rs:236`). Catalog + emitter, no palette involvement.

**Phase 3 — a general icon role.** The measured 52% is avatars and line-art
icons; `WeatherIcon` is the *only* icon constructor in the catalog, so an
`icons:` axis over it addresses a fraction of the category. Define an icon role
first; the axis follows.

**Phase 4 — typography.** Needs a backend-neutral font ROLE on `Attrs` (today
`l0_txt` carries size/weight/colour only, and the emitter picks a Roboto face
from weight alone, hardcoding `crate_resource("makepad_widgets:resources/…")`)
plus faces placed in the actual dependency — `aichat/widgets/resources`, which
is what `makepad-widgets` resolves to (`app/app/Cargo.toml:23`). `.mono` ships
today (JetBrains/Liberation are bundled); `.serif` and `.condensed` need one
face each. The emitter's deliberate refusal to name Roboto for CJK, arrows and
live text (`l0_widgets.rs:127`) must survive — glyph-coverage tests required.

**Phase 5 — surface, then density.** Surface is *not* "kit structure": the
lowering has already erased the distinctions it needs. Authored `Panel` and
`Card` both become `l0_panel`, every generic `Row` becomes `l0_row`, `Grid`
synthesizes its own rows (so `.cards` would card every grid row), map lowering
strips docked panels and draws its own chrome, and swipe-reveal recognition
depends on the evaluated root staying a `Row` — carding it would disable the
gesture. Surface therefore requires semantic roles (`ListRow`, `ListSeparator`,
Panel-vs-Card lowering) *and* emitter work: `border`, `bordercolor` and
`elevation` exist on the node and are read by the evaluator but are never
emitted (`l0_widgets.rs:888` writes only background, gradient stop and radius).
Density then requires an audit of hardcoded geometry — `space_factor` reaches
only the derived spacing list, while chips, fields, thumbnails, map bands and
visualisations carry fixed numbers.

**Phase 6 — charts.** Unchanged: a real widget project. Route chart-heavy
designs to the webview card until it exists.

## Measurement

The 120 recipes and mockups exist, so a per-phase re-run costs no image
generation and no HTML — **but the harness has no card-only mode today**: it
skips any id already in the ledger and otherwise runs the full specimen
(`batch_styles.py:323`). Add `--cards-only` before quoting phase deltas.
Phase 1 should show as a lift in `material-light`, `pastel-soft`, `neon-night`
and `dense-feed` (today 2.86-2.88). Single judge, ±1 variance, known length
bias: every phase gate needs eyeball review of three renders, not only a mean.

## Risks

1. **Silent axis collision** — two deltas writing one token; mitigated by the
   ownership table and its test.
2. **Stale renders** — theme overrides outside the cache key; fixed in Phase 0.
3. **Profile creep** — `accent: .amber` is a mood word; a hex would not be. The
   closed token set is the enforcement.
4. **Judge-driven design** — optimising against one Opus judge; hence the
   eyeball gate.
5. **Backend divergence** — OH/Android do not consume this pipeline, so axes
   widen an existing gap rather than breaking parity. Track it deliberately.
