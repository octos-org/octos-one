   7. Audit all hardcoded geometry, then density.
   8. Define a general icon role before claiming the icons axis addresses the measured category.

**Verdict:** phase 1 is not safe to start as written. It is safe to start only the Phase 0 parser/contract/cache/test work. The accent implementation should wait until existing output has immutable per-mood baselines, omitted-axis legacy behavior is explicit, and the Makepad-only versus three-backend scope is resolved.
tokens used
512,420
1. **MAJOR — The proposed VM composition is legal, but the axes are not automatically independent.** `kit_for` currently emits base → mood → derive → kit ([l0_card.rs:67](/Users/yuechen/home/octos-one/app/app/src/app/l0_card.rs:67)). A later `let` does not error: the actual VM creates a new scope whose prototype is the old scope ([object_heap.rs:616](/Users/yuechen/home/octos-one/aichat/platform/script/src/object_heap.rs:616)) and pushes that shadow scope ([thread.rs:267](/Users/yuechen/home/octos-one/aichat/platform/script/src/thread.rs:267)). Functions capture the current scope when declared ([opcodes_calls.rs:271](/Users/yuechen/home/octos-one/aichat/platform/script/src/opcodes_calls.rs:271)). Therefore:

   - Later delta `let`s shadow earlier ones successfully.
   - All knob-changing deltas must precede DERIVE.
   - DERIVE must precede KIT.
   - A delta after KIT would not affect already-declared role functions.
   - Assignment is not equivalent to rebinding; the kit documents that `fill = …` fails in this VM ([kit:408](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:408)).

   The false part is “independent.” Overlapping writes are last-wins, not composed. For example, glass currently changes `radius_factor` ([glass palette:26](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_palette_glass.splash:26)); it does not already dial the proposed `panel_radius_factor`, contrary to the plan ([plan:71](/Users/yuechen/home/octos-one/lab/style-factory/PLAN-theme-axes.md:71)). The design needs an enforced per-axis token-ownership table and a structural test over all 1,296 combinations.

2. **BLOCKER — The proposed theme line is not parseable today.** The parser literally consumes one identifier:

   ```rust
   "theme" => {
       self.at += 1;
       ...
       if let Some(name) = self.ident() {
           ...
           card.theme = Some((name, line));
       }
   }
   ```

   ([parser:1200](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:1200)). `accent`, `:`, and `.amber` remain in the token stream, after which `accent` is diagnosed as an unexpected top-level declaration ([parser:1240](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:1240)). The lexer recognizes dotted tokens and colons ([lexer:536](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:536), [lexer:648](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:648)), but it discards newline tokens ([lexer:481](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:481)). Parsing “axes only on the theme line” therefore needs explicit same-line logic using `Token.line`, not just more constants.

   The data model and host API also need real changes: `Card` holds only `Option<(String, line)>` ([lib.rs:795](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:795)), `card_theme()` returns only one string ([lib.rs:9026](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:9026)), and `kit_for` asks only for that string ([l0_card.rs:69](/Users/yuechen/home/octos-one/app/app/src/app/l0_card.rs:69)). This requires a normalized `ThemeSpec`, axis duplicate checks, token validation, diagnostics, host override resolution, and updated APIs.

3. **BLOCKER — “Identity defaults make existing cards byte-identical” is false as specified.** The existing roles do not share one latent accent:

   - Selected chips use `l0_active`; unselected chips use `l0_fill` ([kit:459](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:459)).
   - Eyebrows use neutral `l0_soft` ([kit:97](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:97)).
   - Primary actions use `l0_go` ([kit:447](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:447)).
   - Directional values use separate `l0_up`/`l0_down` ([kit:101](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:101)).
   - Dark/light/glass temp bars use `l0_bar == 0`, meaning the legacy multicolour renderer; photo supplies an indigo bar ([dark palette:56](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_palette_dark.splash:56), [photo palette:36](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_palette_photo.splash:36), [kit:621](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:621)).

   Three new accent tokens cannot unconditionally replace all those heterogeneous defaults without moving some existing rendering. Literal byte identity is impossible once base/KIT source changes; output identity is possible only with an explicit legacy/no-axis branch or by having accent deltas rebind the existing role-specific tokens.

4. **BLOCKER — The existing kit lacks enough semantics for `.cards` and `.framed`.** Fixed authored views do not by themselves prevent branching: a kit function can return a different wrapper. The problem is that the lowering has already erased the distinctions needed to branch correctly:

   - Authored `Panel` and `Card` both become `l0_panel` ([lib.rs:9943](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:9943)).
   - Every generic `Row` becomes `l0_row`, including layout/header rows.
   - `Grid` synthesizes its own `l0_row` calls ([lib.rs:10484](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:10484)); `.cards` would card every grid row.
   - Map lowering strips docked `Panel` wrappers and lets `l0_surface_map` draw separate hardcoded chrome ([lib.rs:10382](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:10382)); changing `l0_panel` will not affect those surfaces.
   - Swipe reveal is recognized only when the evaluated root remains `NodeKind::Row` ([l0_widgets.rs:650](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:650)); changing the row to a card can disable reveal behavior.
   - `Rule` has no distinction between a list separator and an intentional design rule.

   Surface needs semantic roles/context such as `ListRow` and `ListSeparator`, distinct Panel/Card lowering, map-surface handling, and a wrapper strategy that preserves the inner Row.

5. **BLOCKER (surface) — Border and elevation currently die at the Makepad emitter.** The shared node has `elevation`, `border`, and `bordercolor` ([node.rs:259](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-node/src/node.rs:259), [node.rs:291](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-node/src/node.rs:291)), and the evaluator reads them ([l0_eval.rs:154](/Users/yuechen/home/octos-one/app/app/src/app/l0_eval.rs:154)). But Makepad emission writes only background, gradient stop, and radius ([l0_widgets.rs:888](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:888)). Neither border nor elevation is emitted. Phase 3 is therefore emitter/widget work, not merely “kit structure” as claimed ([plan:121](/Users/yuechen/home/octos-one/lab/style-factory/PLAN-theme-axes.md:121)).

6. **BLOCKER (type) — There is no font-family transport from kit to backend.** `l0_txt` carries only size, weight, and colour ([kit:50](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:50)); shared `Attrs` similarly has size/weight but no family or semantic font token ([node.rs:245](/Users/yuechen/home/octos-one/splash-makepad/crates/splash-node/src/node.rs:245)). The emitter chooses Roboto solely from weight and hardcodes:

   `crate_resource("makepad_widgets:resources/{face}.ttf")`

   ([l0_widgets.rs:162](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:162)). This app’s `makepad-widgets` is the `../../aichat/widgets` path dependency ([Cargo.toml:23](/Users/yuechen/home/octos-one/app/app/Cargo.toml:23)); adding fonts to app resources or another Makepad checkout will not satisfy that URI. The current emitter also intentionally declines explicit Roboto for CJK, arrows, and live text to preserve fallback coverage ([l0_widgets.rs:127](/Users/yuechen/home/octos-one/app/app/src/app/l0_widgets.rs:127)).

   Phase 2 first needs a backend-neutral font-role/stack field, evaluator support, backend mappings, glyph-fallback tests, and resource placement in the actual dependency.

7. **BLOCKER (portability claim) — Splash-OH and Splash-Android do not consume this L0 pipeline today.** “Accent is safe” across three backends is unsupported ([plan:135](/Users/yuechen/home/octos-one/lab/style-factory/PLAN-theme-axes.md:135)).

   Splash-OH depends on `splash-core`, not `splash-ui-l0` or the shared `splash-node` ([Cargo.toml:14](/Users/yuechen/home/Splash-OH/crates/splash-oh-native/Cargo.toml:14)). Its entry points evaluate unrelated bundled Splash assets ([dsl.rs:40](/Users/yuechen/home/Splash-OH/crates/splash-oh-native/src/dsl.rs:40)), and its walker drops unknown tags ([dsl.rs:468](/Users/yuechen/home/Splash-OH/crates/splash-oh-native/src/dsl.rs:468)); it has no `card`, `chip`, `divider`, or L0 data-visualization path.

   Splash-Android likewise has its own node type and evaluator ([lib.rs:42](/Users/yuechen/home/Splash-Android/catalog/rust/src/lib.rs:42)). Its semantic-plan path explicitly says “No DSL is involved” ([plan.rs:1](/Users/yuechen/home/Splash-Android/catalog/rust/src/plan.rs:1)) and uses a separate hardcoded theme, including one fixed accent ([plan.rs:56](/Users/yuechen/home/Splash-Android/catalog/rust/src/plan.rs:56)). Either phase 1 must be explicitly called a Makepad-only pilot, or cross-backend adapters are required before claiming the invariant holds.

8. **MAJOR — Kit caching and host overrides are missing from the design.** Every render concatenates the full kit and lowered card ([l0_card.rs:197](/Users/yuechen/home/octos-one/app/app/src/app/l0_card.rs:197)), creates a fresh VM, clones the source, and reparses it ([l0_eval.rs:59](/Users/yuechen/home/octos-one/app/app/src/app/l0_eval.rs:59)). The surrounding render cache exists because this pipeline costs about 30 ms, but its key is only item, raw message, card state, and fetch epoch ([main.rs:5124](/Users/yuechen/home/octos-one/app/app/src/main.rs:5124)). A host-level type/density/accent override can change while those inputs remain equal, leaving a stale widget tree.

   Normalize to an effective `ThemeSpec`, include its revision in the render-cache key, and lazily cache assembled prefixes by that spec. Do not prebuild 1,296 strings blindly; benchmark parsing and memory first.

9. **MAJOR — L1/L2 and the normative profile are omitted.** The same parser admits explicitly declared L1 cards ([lib.rs:225](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:225)), so theme grammar changes automatically affect L1 and need L1 tests. L2 is explicitly refused because it uses another grammar ([lib.rs:232](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:232)); it will not gain axes through this work.

   The normative profile still specifies only `theme dark` and one catalogued mood ([ui-profile-l0.md:540](/Users/yuechen/home/octos-one/splash/docs/ui-profile-l0.md:540)). The TOML is explicitly only the constructor argument contract ([ui-l0-constructors.toml:1](/Users/yuechen/home/octos-one/splash/docs/ui-l0-constructors.toml:1)); it has no schema for theme axes. The plan needs a normative theme-axis catalog—either a new TOML section/file or another single source of truth—with code/spec agreement tests.

10. **MAJOR — Density, truncation, and icons are substantially under-scoped.** `space_factor` affects only the derived spacing list ([derive:12](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_derive.splash:12)), while map bands/controls, chips, fields, thumbnails, and visualizations contain many hardcoded sizes ([kit:248](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:248), [kit:429](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:429), [kit:500](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:500)). Density would currently produce a partly scaled, internally inconsistent UI.

   Truncation is not “catalog + kit”: `TextRow` does not accept `lines` ([catalog:3034](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:3034)); although `Attrs.lines` exists, the Makepad emitter never reads it. The Label widget already requires both `max_lines` and ellipsis overflow ([label.rs:236](/Users/yuechen/home/octos-one/aichat/widgets/src/label.rs:236)), so emitter work is mandatory.

   The proposed icons axis only reaches `WeatherIcon`; that is the sole icon constructor ([catalog:3082](/Users/yuechen/home/octos-one/splash/crates/splash-ui-l0/src/lib.rs:3082)), and `icon_mono` only recolours that widget ([kit:600](/Users/yuechen/home/octos-one/splash-makepad/components/l0/_kit.splash:600)). It does not address avatars or general line-art icons—the reported 52% category.

11. **MAJOR — The stated identity and measurement tests do not exist in the claimed form.** `l0_themes_are_all_answered` only checks table membership and string presence, not evaluated token values or role output ([l0_card.rs:979](/Users/yuechen/home/octos-one/app/app/src/app/l0_card.rs:979)). The checked-in four device goldens are weather, stock-list, stock-detail, and news—not four moods ([device-l0-test.py:63](/Users/yuechen/home/octos-one/docs/tools/device-l0-test.py:63)). The harness itself warns that regenerating a golden to clear a failure merely blesses current output ([device-l0-test.py:14](/Users/yuechen/home/octos-one/docs/tools/device-l0-test.py:14)).

   The style-factory runner also has no card-only rerun mode: it skips every ID already present in the ledger and otherwise executes `run_specimen` ([batch_styles.py:323](/Users/yuechen/home/octos-one/lab/style-factory/batch_styles.py:323)), which includes mockup and HTML work. This checkout has neither `out/` nor `recipes.jsonl`, despite the runner expecting both. Moreover, the reported means are n=115 versus n=91, not 120 paired cases ([FINDINGS.md:11](/Users/yuechen/home/octos-one/lab/style-factory/FINDINGS.md:11)), and 22 reading cards have an acknowledged empty-data confound ([FINDINGS.md:77](/Users/yuechen/home/octos-one/lab/style-factory/FINDINGS.md:77)). “Mentioned in 92%” is frequency of judge feedback, not an expected 92% lift.

12. **MAJOR — The phasing should be reordered.** A defensible sequence is:

   1. Phase 0: normative grammar, `ThemeSpec`, catalog/schema, parser/host APIs, override resolution, cache key, delta ownership rules, and immutable pre-change baselines.
   2. Phase 1a: Makepad-only accent pilot with explicit legacy fallback and clearly identified accent-consuming roles.
   3. Phase 1b: either port that contract to OH/Android or explicitly keep it experimental and Makepad-only.
   4. Truncation vertical slice; its backend plumbing is bounded and already partly present.
   5. Typography transport and fallback model, then assets and the type axis.
   6. New row/separator/panel semantics plus emitter support, then the surface axis.
   7. Audit all hardcoded geometry, then density.
   8. Define a general icon role before claiming the icons axis addresses the measured category.

**Verdict:** phase 1 is not safe to start as written. It is safe to start only the Phase 0 parser/contract/cache/test work. The accent implementation should wait until existing output has immutable per-mood baselines, omitted-axis legacy behavior is explicit, and the Makepad-only versus three-backend scope is resolved.

[exited with code 0]
