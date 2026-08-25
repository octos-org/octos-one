The harness contains no delta calculation, no `dsl_gap` grouping, and no pass/fail threshold. It deletes the old screenshots and writes a new ledger ([batch_styles.py:359-377](/Users/yuechen/home/octos-one/lab/style-factory/batch_styles.py:359)). Its “no HTML” claim is also false: every `run_specimen` still judges HTML and may rewrite/re-render it ([batch_styles.py:248-285](/Users/yuechen/home/octos-one/lab/style-factory/batch_styles.py:248)).

Finally, the claimed “four mood baselines” are not present. The checked-in device goldens are weather, stock-list, stock-detail, and news—not dark/light/glass/photo ([device-l0-test.py:63-70](/Users/yuechen/home/octos-one/docs/tools/device-l0-test.py:63)).

4. **MAJOR — Stroke transport exists through evaluation, but “the emitter only needs to write three attrs” is incomplete.**

The exact trace is:

- The kit VM can produce the exact keys `border`, `bordercolor`, and `elevation`; they are valid fixed `Attrs` fields ([node.rs:259-262](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-node/src/node.rs:259), [node.rs:291-292](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-node/src/node.rs:291)).
- The evaluator already reads all three ([l0_eval.rs:154-171](/Users/yuechen/home/octos-one/app/app/src/app/l0_eval.rs:154)).
- The current kit does **not** set them: `l0_panel` supplies only fill, radius, padding, and margins ([kit:388-400](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:388)).
- The app emitter emits only `bg`, `bg2`, and radius in this section and ignores border/elevation ([l0_widgets.rs:888-905](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:888)).

Both target widgets support borders:

- `RectView`: `draw_bg.border_size` and `draw_bg.border_color` ([view_ui.rs:100-144](/Users/yuechen/home/octos-one/aichat/widgets/src/view_ui.rs:100)).
- `RoundedView`: the same names, plus `border_radius` ([view_ui.rs:308-350](/Users/yuechen/home/octos-one/aichat/widgets/src/view_ui.rs:308)).
- `RoundedShadowView` additionally exposes `shadow_color`, `shadow_radius`, and `shadow_offset` ([view_ui.rs:227-246](/Users/yuechen/home/octos-one/aichat/widgets/src/view_ui.rs:227)).

`Card`/`Chip` already select `RoundedView`, so their border is primarily missing attribute-writing ([l0_widgets.rs:461-470](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:461)). Generic containers remain plain `View`; elevation always needs widget-class promotion. Square De Stijl-style surfaces also need `RectView`, because `RoundedView` clamps its shader radius to at least `1.0` ([view_ui.rs:340-345](/Users/yuechen/home/octos-one/aichat/widgets/src/view_ui.rs:340)).

5. **BLOCKER — The plan missed an existing generic-background renderer bug that would contaminate the stroke ablation.**

The app maps almost every container to plain `View` ([l0_widgets.rs:461-495](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:461)) and writes `draw_bg` properties without setting `show_bg` ([l0_widgets.rs:888-905](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:888)). Plain `View` defaults `show_bg` to false and only begins its background draw when true ([view.rs:68-80](/Users/yuechen/home/octos-one/aichat/widgets/src/view.rs:68), [view.rs:980-988](/Users/yuechen/home/octos-one/aichat/widgets/src/view.rs:980)); its base `DrawQuad` pixel is transparent ([draw_quad.rs:61-67](/Users/yuechen/home/octos-one/aichat/draw/src/shader/draw_quad.rs:61)).

Consequently, generic-column backgrounds such as `l0_surface`, the photo scrim, map bands, and map sheets are not reliably painted despite the emitted attributes ([kit:195-212](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:195), [kit:248-251](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:248)).

The sibling Makepad renderer already documents and solves this by promoting any generic container with background/radius/border to `RoundedView`, and elevation to `RoundedShadowView` ([splash-makepad lib.rs:413-429](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-makepad/src/lib.rs:413)). If stroke work ports that behavior, it simultaneously activates previously dormant fills, gradients, and radii across the corpus. Any score lift could not be attributed to stroke. This baseline renderer defect must be fixed and rebaselined separately.

6. **MAJOR — Theme-side truncation is feasible; item 3 is conceptually correct.**

No card-facing `lines:` argument is required. The trace can be:

`TextRow` → `l0_row_text` → kit-produced `{t:"text", lines:row_lines}` → evaluator → emitter → `Label`.

`TextRow` currently has no `lines` argument ([lib.rs:3026-3034](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:3026)), but lowering already routes it to `l0_row_text` ([lib.rs:9966-9971](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:9966)). That kit function may stamp `lines` directly; evaluator already reads it ([l0_eval.rs:179-180](/Users/yuechen/home/octos-one/app/app/src/app/l0_eval.rs:179)).

The remaining emitter mapping is:

- `lines > 0` → `max_lines: N`
- plus `text_overflow: Ellipsis`

`Label` exposes both fields and forwards them to `DrawText` ([label.rs:236-242](/Users/yuechen/home/octos-one/aichat/widgets/src/label.rs:236), [label.rs:309-310](/Users/yuechen/home/octos-one/aichat/widgets/src/label.rs:309)). Ellipsis requires a bounded width ([draw_text.rs:1250-1259](/Users/yuechen/home/octos-one/aichat/draw/src/shader/draw_text.rs:1250)); the emitter usually supplies `Fill` when text has no explicit width ([l0_widgets.rs:940-948](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:940)). Fit/hug and explicitly sized cases still need tests.

7. **MAJOR — The photo knobs are syntactically palette-only, but behaviorally coupled.**

Specific hazards:

- Dark is the base inherited by every mood; changing a dark token also changes light/glass unless they explicitly rebind it ([palette_dark.splash:8-15](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_palette_dark.splash:8)). Light overrides only colours, while glass additionally overrides radius ([palette_light.splash:12-34](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_palette_light.splash:12), [palette_glass.splash:10-28](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_palette_glass.splash:10)).
- `l0_bar` and `l0_bar_rail` are a pair: the bar switch tests only `l0_bar` but writes both values ([kit:621-627](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:621)). Setting only the bar gives a zero/transparent rail.
- `icon_mono` reaches only `WeatherIcon`, not general icons ([kit:597-610](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:597)).
- `panel_inset` reaches `l0_panel`, but docked map panels are deliberately unwrapped and map chrome is separately hardcoded ([lib.rs:10382-10403](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:10382), [kit:315-367](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:315)).
- A card declaring `light` over a `Photo` root is forcibly rendered with the photo palette, so light-palette edits do not reach it ([l0_card.rs:69-90](/Users/yuechen/home/octos-one/app/app/src/app/l0_card.rs:69)).
- `hero_factor` affects only the top hero step (`pt >= 43`), which lowering selects only for text of four characters or fewer ([kit:75-81](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:75), [lib.rs:7688-7699](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:7688)).
- `weight_hero` is ignored for CJK, arrows, and unresolved live text because the fallback emitter writes only font size when it declines Roboto ([l0_widgets.rs:139-159](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:139)).

8. **MAJOR — “Stroke” and “panel chrome” are bundles of incompatible styles, not one theme default.**

I independently confirmed the reported 53/100 `stroke` count. The numerical claim is sound. The implementation inference is not: recipes use `stroke` for Bauhaus outlines, De Stijl thick black rules, Art Deco double frames, and Memphis/neubrutalist hard-offset surfaces ([recipes.py:83-118](/Users/yuechen/home/octos-one/lab/style-factory/recipes.py:83)). One mood-owned panel border cannot express all of those.

The surface taxonomy also distinguishes soft elevation, zero-blur hard offset, and paired/inner relief shadows ([recipes.py:139-145](/Users/yuechen/home/octos-one/lab/style-factory/recipes.py:139)). `Attrs.elevation` is explicitly Material-style elevation ([node.rs:259-262](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-node/src/node.rs:259)); the existing sibling renderer maps it to one soft black drop shadow ([splash-makepad lib.rs:699-707](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-makepad/src/lib.rs:699)). That cannot represent hard-offset or inner-shadow recipes.

Additionally, authored `Panel` and `Card` both collapse to `l0_panel` ([lib.rs:9951-9953](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:9951)). There is no remaining semantic distinction for “outline this panel but not this ordinary card.”

9. **MAJOR — Typography can be zero-card-syntax, but not by the plan’s stated route.**

A new `font_role` field is unnecessary: existing `Attrs.variant` is explicitly documented to carry “the type role on a text node” ([node.rs:293-299](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-node/src/node.rs:293)). The kit already uses it for `eyebrow` ([kit:91-99](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:91)). Stamping `title`, `body`, `hero`, etc. into that existing field and switching in the emitter preserves the fence.

But new serif/display faces cannot be added by the fenced loop because the plan itself puts font-binary bundling outside it. Font mapping must also carry a complete fallback chain: the existing text-role widgets document that a Latin-only explicit family replaces CJK/emoji fallbacks and makes arrows tofu ([text_roles.rs:17-31](/Users/yuechen/home/octos-one/aichat/widgets/src/text_roles.rs:17)); their correct role definitions include Latin, symbols, CJK, and emoji members ([text_roles.rs:43-68](/Users/yuechen/home/octos-one/aichat/widgets/src/text_roles.rs:43)).

Therefore:

- Role-based typography using existing assets: zero card grammar, feasible.
- The queued serif/font-pair capability requiring new binaries: not fenced as written.

10. **BLOCKER — Major experimental confounds survive even after the harness is made runnable.**

- **Live data:** the host injects freshly fetched rows/scalars before realization ([l0_card.rs:133-177](/Users/yuechen/home/octos-one/app/app/src/app/l0_card.rs:133)); the harness seeds only the photo URL and weather city ([batch_styles.py:321-325](/Users/yuechen/home/octos-one/lab/style-factory/batch_styles.py:321)). Headlines, prices, quake places, row counts, and text lengths can change between A/B renders. A judge instruction to ignore content cannot undo layout and truncation changes.
- **Judge variance/model drift:** every score is one unseeded call to the moving `opus` alias ([batch_styles.py:101-125](/Users/yuechen/home/octos-one/lab/style-factory/batch_styles.py:101)). Only scores ≤3 are retried, and the maximum is retained, creating conditional upward bias ([batch_styles.py:331-344](/Users/yuechen/home/octos-one/lab/style-factory/batch_styles.py:331)).
- **Render nondeterminism:** `WeatherIcon` is continuously animated at roughly 60fps ([weather_icon.rs:3-6](/Users/yuechen/home/octos-one/aichat/widgets/src/weather_icon.rs:3)). Pixel settling may accept an arbitrary phase if the changed area remains below 0.2%, or time out and capture another arbitrary phase ([batch_styles.py:165-186](/Users/yuechen/home/octos-one/lab/style-factory/batch_styles.py:165)).
- **No contemporaneous control:** old screenshots are deleted before new rendering, so the old build cannot be rejudged blindly with the same model invocation.
- **Old-card effect:** fixed old sources are valid for measuring *retroactive lift*. They do not measure future generator fluency: the translator generated each card from the mockup, old examples, and constructor contract, with validation but no opportunity to adapt roles/mood/composition to the changed renderer ([batch_styles.py:287-315](/Users/yuechen/home/octos-one/lab/style-factory/batch_styles.py:287)). The result may understate useful capabilities or reward indiscriminate defaults that future cards would not choose.

11. **MAJOR — Autonomous overnight editing has several silent, codebase-specific failure modes.**

- A misspelled/unknown kit attribute is silently discarded because `Attrs` is fixed ([kit:28-31](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:28)).
- A missing palette token silently becomes zero/transparent ([palette_dark.splash:17-21](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_palette_dark.splash:17)).
- Reassignment can silently fail, and a token/function name collision can silently coerce a colour to zero—both have already happened in this kit ([kit:408-419](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:408), [palette_dark.splash:75-85](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_palette_dark.splash:75)).
- Emitting `border` instead of Makepad’s `draw_bg.border_size`, or failing to promote to `RoundedShadowView`, yields no visible result.
- Border units require calibration: the sibling renderer halves a model border before writing `border_size`, with a test locking `border:1` to `0.5` ([splash-makepad lib.rs:690-697](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-makepad/src/lib.rs:690), [splash-makepad lib.rs:1040-1050](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-makepad/src/lib.rs:1040)).
- A palette edit can unintentionally propagate through base inheritance, the light→photo substitution, the bar/rail pair, or bypassed map chrome.
- A font edit can drop CJK/arrows/live-text weight.
- The runner deletes prior screenshots and moves its only ledger before proving the rerun succeeded, catches per-specimen failures, and still prints completion ([batch_styles.py:359-378](/Users/yuechen/home/octos-one/lab/style-factory/batch_styles.py:359)).
- Existing app tests check palette/catalog membership and source-string presence, not rendered border/elevation/truncation behavior ([l0_card.rs:972-1005](/Users/yuechen/home/octos-one/app/app/src/app/l0_card.rs:972)).

## Verdict

The loop is **not safe to run fenced as described**.

Before any scored ablation, it needs:

1. A reproducible build/install step tied to the exact source revision.
2. Restored immutable round-two cards/mockups, the correct round-two ledger, and an assertion that exactly 100 paired specimens completed.
3. Frozen realized data and animation state.
4. Retained old screenshots plus repeated or blinded paired judging with a pinned model.
5. The generic `View` background/promotion defect fixed and independently rebaselined.
6. End-to-end tests for border, elevation, and truncation.
7. Capability-specific exposure definitions instead of relying blindly on `dsl_gap`.

After that, theme-side truncation and a narrowly scoped `Card`/`Chip` border are defensible zero-syntax pilots. Broad “stroke,” panel chrome, and new-font typography are not safely represented or measurable by the current plan.

[exited with code 0]
