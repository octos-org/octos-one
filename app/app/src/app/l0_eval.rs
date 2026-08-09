//! Evaluate the L0 theme kit in THIS repository's VM, into a shared `UiNode`.
//!
//! `ui-profile-l0.md` §1.1 names `UiNode` as the point where one card reaches
//! three backends. `splash-node` carries that model and depends on nothing, so
//! this app can hold it — but the evaluator that *produces* one needs a VM, and
//! `splash-render`'s is a different lineage. Taking it fails the lockfile:
//!
//! ```text
//! makepad-error-log v1.0.0 (makepad-splash/libs/error_log)
//! makepad-error-log v1.0.0 (octos-one/aichat/libs/error_log)
//! ```
//!
//! So the contract is shared and the evaluator is not. This is the second
//! implementation of it, against this repo's own `makepad-script`, producing the
//! same tree. `splash-render`'s `eval.rs` is the reference — a working one, which
//! is the difference between porting and guessing.
//!
//! **This is not a fork.** The tree it produces is the shared type, so a
//! divergence shows up as a test failing here rather than as two node models
//! drifting. The only thing duplicated is the walk.

use makepad_widgets::makepad_draw::makepad_platform::makepad_script::apply::*;
use makepad_widgets::makepad_draw::makepad_platform::makepad_script::array::ScriptArrayStorage;
use makepad_widgets::makepad_draw::makepad_platform::makepad_script::makepad_live_id::*;
use makepad_widgets::makepad_draw::makepad_platform::makepad_script::traits::*;
use makepad_widgets::makepad_draw::makepad_platform::makepad_script::*;
use splash_node::{Attrs, NodeKind, UiNode};

/// Evaluate `src` with this app's capabilities registered.
///
/// The kit lowers a source the backend can answer into a live call, so a card
/// evaluated on a bare VM does not merely miss data — `sys.stock(…)` is
/// undefined, and the concatenation around it yields `$[Error:WrongValue]`,
/// which then renders as the price. Loud rather than silent, and still wrong.
///
/// `register_agent_module` installs the whole `sys` module, the same one the
/// existing Splash widget evaluates cards against.
pub fn build_with_capabilities(cx: &mut makepad_widgets::Cx, src: &str) -> Option<UiNode> {
    build(cx, src, makepad_widgets::splash::register_agent_module)
}

/// Evaluate `src` and walk it into a `UiNode` tree.
///
/// Returns `None` when the script evaluates to nil — a parse or runtime error —
/// or when the root tag is not one the model knows. A card must never render as
/// a silent absence, so a caller reports `None` rather than drawing nothing.
pub fn build(
    cx: &mut makepad_widgets::Cx,
    src: &str,
    register: impl FnOnce(&mut ScriptVm),
) -> Option<UiNode> {
    // The HOST is the app's `Cx`, not a placeholder.
    //
    // `splash-render`'s evaluator passes a dummy because nothing it evaluates
    // touches one. This backend's capabilities do: `cx_mut()` downcasts the host
    // to `&mut Cx` and unwraps, so a `sys.*` call on a VM built with anything
    // else panics the app. Weather and news lower to literals and survived it;
    // the stock card has live calls and took the process down.
    let mut std_slot = 0;
    let vm = &mut ScriptVm {
        host: cx,
        std: &mut std_slot,
        bx: Box::new(ScriptVmBase::new()),
    };

    register(vm);

    let value = vm.eval(ScriptMod {
        cargo_manifest_path: String::new(),
        module_path: String::from("splash"),
        file: String::from("l0.splash"),
        line: 0,
        column: 0,
        code: src.to_string(),
        values: Vec::new(),
    });

    if value.is_nil() {
        return None;
    }
    walk(vm, value, 0)
}

/// Evaluate on a host that is NOT a `Cx`. Test-only, and unsafe for any card
/// with a live call.
///
/// A capability downcasts the host and unwraps, so `sys.*` on this VM panics.
/// That is fine for a card whose sources all resolve from data — which is what
/// the walk tests exercise — and catastrophic for one that does not, which is
/// why it is `cfg(test)` and named for what it lacks.
#[cfg(test)]
fn build_without_capabilities(src: &str) -> Option<UiNode> {
    let mut host = 0;
    let mut std_slot = 0;
    let vm = &mut ScriptVm {
        host: &mut host,
        std: &mut std_slot,
        bx: Box::new(ScriptVmBase::new()),
    };
    let value = vm.eval(ScriptMod {
        cargo_manifest_path: String::new(),
        module_path: String::from("splash"),
        file: String::from("l0.splash"),
        line: 0,
        column: 0,
        code: src.to_string(),
        values: Vec::new(),
    });
    if value.is_nil() {
        return None;
    }
    walk(vm, value, 0)
}

/// One DSL object → one `UiNode`, recursing into `c`.
fn walk(vm: &mut ScriptVm, value: ScriptValue, depth: usize) -> Option<UiNode> {
    // A malformed script must not be able to blow the native stack. The card is
    // generated, so this is a bound on untrusted input rather than a nicety.
    if depth > 32 {
        return None;
    }
    let tag = string_prop(vm, value, id!(t)).unwrap_or_default();
    let kind = NodeKind::from_tag(&tag)?;

    let attrs = Attrs {
        // `text_prop`, not `string_prop`: a card may bind a declared NUMBER here.
        //
        // `string_prop` returns None for a number, so a node whose text was one
        // arrived with no text at all and rendered as an empty row — present,
        // laid out, and blank. Measured with a card reading `sys.gps`, which
        // answers numbers: four labelled rows, four empty values, and nothing to
        // say whether the device had a fix or the call had failed.
        //
        // The same fix was already made for `variant` after the kit's `variant: 2`
        // arrived as nothing and every weather icon fell back to a default sun.
        // This is the same defect one slot over.
        text: text_prop(vm, value, id!(text)),
        label: string_prop(vm, value, id!(label)),
        placeholder: string_prop(vm, value, id!(placeholder)),
        id: string_prop(vm, value, id!(id)),
        tapto: string_prop(vm, value, id!(tapto)),
        src: string_prop(vm, value, id!(src)),
        fit: int_prop(vm, value, id!(fit)),
        w: f32_prop(vm, value, id!(w)),
        h: f32_prop(vm, value, id!(h)),
        fitw: int_prop(vm, value, id!(fitw)),
        fith: int_prop(vm, value, id!(fith)),
        fillw: int_prop(vm, value, id!(fillw)),
        fillh: int_prop(vm, value, id!(fillh)),
        size: f32_prop(vm, value, id!(size)),
        weight: int_prop(vm, value, id!(weight)),
        icon: int_prop(vm, value, id!(icon)),
        icon_name: string_prop(vm, value, id!(icon)),
        color: u32_prop(vm, value, id!(color)),
        bg: u32_prop(vm, value, id!(bg)),
        bg2: u32_prop(vm, value, id!(bg2)),
        radius: f32_prop(vm, value, id!(radius)),
        elevation: f32_prop(vm, value, id!(elevation)),
        pad: f32_prop(vm, value, id!(pad)),
        padx: f32_prop(vm, value, id!(padx)),
        pady: f32_prop(vm, value, id!(pady)),
        padtop: f32_prop(vm, value, id!(padtop)),
        padbottom: f32_prop(vm, value, id!(padbottom)),
        spacing: f32_prop(vm, value, id!(spacing)),
        margin: f32_prop(vm, value, id!(margin)),
        marginx: f32_prop(vm, value, id!(marginx)),
        marginy: f32_prop(vm, value, id!(marginy)),
        margintop: f32_prop(vm, value, id!(margintop)),
        marginbottom: f32_prop(vm, value, id!(marginbottom)),
        border: f32_prop(vm, value, id!(border)),
        bordercolor: u32_prop(vm, value, id!(bordercolor)),
        variant: text_prop(vm, value, id!(variant)),
        enabled: int_prop(vm, value, id!(enabled)),
        key: string_prop(vm, value, id!(key)),
        action: string_prop(vm, value, id!(action)),
        items: string_prop(vm, value, id!(items)),
        selected: int_prop(vm, value, id!(selected)),
        hint: string_prop(vm, value, id!(hint)),
        title: string_prop(vm, value, id!(title)),
        lines: int_prop(vm, value, id!(lines)),
        badge: string_prop(vm, value, id!(badge)),
        count: int_prop(vm, value, id!(count)),
        supporting: string_prop(vm, value, id!(supporting)),
        helper: string_prop(vm, value, id!(helper)),
        group: string_prop(vm, value, id!(group)),
        indeterminate: int_prop(vm, value, id!(indeterminate)),
        min: f32_prop(vm, value, id!(min)),
        max: f32_prop(vm, value, id!(max)),
        step: f32_prop(vm, value, id!(step)),
        value2: f32_prop(vm, value, id!(value2)),
        error: string_prop(vm, value, id!(error)),
        accent: u32_prop(vm, value, id!(accent)),
        markcolor: u32_prop(vm, value, id!(markcolor)),
        value: f32_prop(vm, value, id!(value)),
        total: f32_prop(vm, value, id!(total)),
        align: int_prop(vm, value, id!(align)),
        alignx: f32_prop(vm, value, id!(alignx)),
        aligny: f32_prop(vm, value, id!(aligny)),
        on: int_prop(vm, value, id!(on)),
        tap: int_prop(vm, value, id!(tap)),
        lat: num_prop(vm, value, id!(lat)),
        lon: num_prop(vm, value, id!(lon)),
        zoom: num_prop(vm, value, id!(zoom)),
        changeto: string_prop(vm, value, id!(changeto)),
        polyline: string_prop(vm, value, id!(polyline)),
        markers: string_prop(vm, value, id!(markers)),
        route_badge: string_prop(vm, value, id!(route_badge)),
        tilt: num_prop(vm, value, id!(tilt)),
        rotation: num_prop(vm, value, id!(rotation)),
        x: num_prop(vm, value, id!(x)),
        y: num_prop(vm, value, id!(y)),
        // The data-visualisation parameters. The compiler enforces this mirror:
        // `Attrs` is exhaustive here, so a field added to the shared model
        // cannot be silently ignored by this evaluator.
        lo: f32_prop(vm, value, id!(lo)),
        hi: f32_prop(vm, value, id!(hi)),
        rise: f32_prop(vm, value, id!(rise)),
        set: f32_prop(vm, value, id!(set)),
        now: f32_prop(vm, value, id!(now)),
        phase: f32_prop(vm, value, id!(phase)),
        illum: f32_prop(vm, value, id!(illum)),
        span: f32_prop(vm, value, id!(span)),
        symbol: string_prop(vm, value, id!(symbol)),
        range: string_prop(vm, value, id!(range)),
        countries: string_prop(vm, value, id!(countries)),
        indicator: string_prop(vm, value, id!(indicator)),
        years: f32_prop(vm, value, id!(years)),
    };

    let mut children = Vec::new();
    for kid in children_of(vm, value) {
        if let Some(child) = walk(vm, kid, depth + 1) {
            children.push(child);
        }
    }

    Some(UiNode {
        kind,
        attrs,
        children,
    })
}

fn prop(vm: &mut ScriptVm, obj: ScriptValue, key: LiveId) -> Option<ScriptValue> {
    vm.bx.heap.value_for_apply(obj, key.into(), &Apply::Eval)
}

fn string_prop(vm: &mut ScriptVm, obj: ScriptValue, key: LiveId) -> Option<String> {
    let v = prop(vm, obj, key)?;
    vm.string_with(v, |_vm, s| s.to_string())
}

/// A property that may be text OR a number, read as text.
///
/// `variant` is the one slot on the shared model that carries a CODE rather than
/// a word — a WeatherIcon's condition is the forecast's WMO number — and
/// `string_prop` returns `None` for a number. So the kit set `variant: 2`, the
/// node received nothing, and every weather icon on the card fell back to its
/// default: seven identical suns over a week that was cloudy, rainy and clear.
///
/// Integral values print without a fraction, because this becomes a shader
/// uniform and `2` reads as a code where `2.0` reads as a measurement.
fn text_prop(vm: &mut ScriptVm, obj: ScriptValue, key: LiveId) -> Option<String> {
    if let Some(s) = string_prop(vm, obj, key) {
        return Some(s);
    }
    let n = num_prop(vm, obj, key)?;
    Some(if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    })
}

/// A numeric property, via the VM's own coercion rather than by reading the
/// NaN-boxed representation, so ints, floats and colour literals all work.
fn num_prop(vm: &mut ScriptVm, obj: ScriptValue, key: LiveId) -> Option<f64> {
    let v = prop(vm, obj, key)?;
    if v.is_nil() {
        return None;
    }
    let mut out: f64 = 0.0;
    <f64 as ScriptApply>::script_apply(&mut out, vm, &Apply::Eval, &mut Scope::default(), v);
    Some(out)
}

fn f32_prop(vm: &mut ScriptVm, obj: ScriptValue, key: LiveId) -> Option<f32> {
    num_prop(vm, obj, key).map(|v| v as f32)
}
fn int_prop(vm: &mut ScriptVm, obj: ScriptValue, key: LiveId) -> Option<i32> {
    num_prop(vm, obj, key).map(|v| v as i32)
}
fn u32_prop(vm: &mut ScriptVm, obj: ScriptValue, key: LiveId) -> Option<u32> {
    num_prop(vm, obj, key).map(|v| v as u32)
}

/// The `c` array's members, copied out so the walk can re-borrow the vm.
///
/// `c` is a ScriptArray, NOT an object holding a vec — arrays are their own heap
/// type in this VM, and treating one as an object drops the entire subtree
/// silently.
fn children_of(vm: &mut ScriptVm, value: ScriptValue) -> Vec<ScriptValue> {
    let Some(c) = prop(vm, value, id!(c)) else {
        return Vec::new();
    };
    let Some(arr) = c.as_array() else {
        return Vec::new();
    };
    match vm.bx.heap.array_storage(arr) {
        ScriptArrayStorage::ScriptValue(v) => v.iter().copied().collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use splash_ui_l0::{kit, realize, RealizeLimits};

    /// The kit and the cards, read from the repositories that own them.
    const KIT: &str = include_str!("../../../../../Splash-Makepad/components/l0/_kit.splash");
    const NEWS: &str = include_str!("../../../../../Splash/crates/splash-ui-l0/tests/fixtures/news.card");
    const STOCK: &str = include_str!("../../../../../Splash/crates/splash-ui-l0/tests/fixtures/stock.card");
    const WEATHER: &str = include_str!("../../../../../Splash/crates/splash-ui-l0/tests/fixtures/weather.card");

    fn build_card(card: &str, data: serde_json::Value) -> splash_node::UiNode {
        let report = realize(card, &data, RealizeLimits::default());
        assert!(
            report.diagnostics.is_empty(),
            "card did not realize: {:#?}",
            report.diagnostics
        );
        let root = report.root.expect("a realized tree");
        let src = format!("{KIT}\n{}", kit::lower(&root));
        super::build_without_capabilities(&src).expect("the lowered card evaluated to nil")
    }

    fn news_data() -> serde_json::Value {
        serde_json::json!({
            "lead": [{"id":"1","title":"Rust 1.95","author":"a","points":412.0,"comments":137.0,"url":"u"}],
            "feed": [{"id":"2","title":"Another","author":"b","points":90.0,"comments":12.0,"url":"u"}],
            "article": {}, "selected": "", "env": {"locale": {"lang":"en"}}
        })
    }

    fn stock_data() -> serde_json::Value {
        serde_json::json!({
            "movers": [{"ticker":"NVDA","name":"Nvidia","last":184.2,"change":3.1,"pct":1.7}],
            "quote": {"name":"Nvidia","last":184.2,"change":3.1,"pct":1.7,"open":181.0,
                      "high":185.6,"low":180.2,"volume":41200000.0,"mktcap":4.52e12,"pe":58.3},
            "selected": "", "range": "m1", "env": {"locale":{}},
            "copy": {"movers":"Top Movers","open":"Open","high":"High","low":"Low",
                     "volume":"Volume","mktcap":"Mkt Cap","pe":"P/E"}
        })
    }

    /// A saved row reveals ITS OWN remove on swipe — widget visibility
    /// scoped to the row, not card state. The dsl must carry: a hidden view
    /// named for this row, a swipe overlay that shows/hides exactly that
    /// name, and the row's own tap as the overlay's click — the swipe
    /// fires INSTEAD of the click, so a drag cannot also open the quote.
    #[test]
    fn a_watch_row_reveals_its_own_remove() {
        let mut data = stock_data();
        data["watch"] = serde_json::json!([
            {"ticker":"NVDA","last":184.2,"pct":1.7},
            {"ticker":"TEAM","last":149.0,"pct":2.0}
        ]);
        // Through the DEVICE chain: realize, kit, eval, then the dsl.
        let dsl = super::super::l0_widgets::to_dsl(&build_card(STOCK, data));
        for name in ["l0rr0", "l0rr1"] {
            assert!(
                dsl.contains(&format!("{name} := View{{ visible: false")),
                "{name} hidden view:\n{dsl}"
            );
            assert!(
                dsl.contains(&format!("on_swipe_left: || ui.{name}.set_visible(true)")),
                "{name} swipe shows it:\n{dsl}"
            );
            assert!(
                dsl.contains(&format!("on_swipe_right: || ui.{name}.set_visible(false)")),
                "{name} swipe hides it:\n{dsl}"
            );
        }
        // One swipe overlay per REVEALING row: the two saved rows and
        // nothing else — movers carry no per-row menu.
        assert!(
            dsl.matches("swipe: true").count() == 2,
            "one swipe overlay per saved row:\n{dsl}"
        );
        assert!(
            dsl.contains("for#0[NVDA]") && dsl.contains("for#0[TEAM]"),
            "each overlay clicks its own row:\n{dsl}"
        );
    }

    fn weather_data() -> serde_json::Value {
        serde_json::json!({
            "place": {"name":"Kyoto","lat":35.0,"lon":135.8},
            "now": {"temp":21.0,"cond":"clear","feels":20.0,"humidity":54.0,"wind":3.2,
                    "pressure":1013.0,"uv":4.0,"visibility":10.0},
            "week": {"days":[{"dayname":"Mon","hi":24.0,"lo":15.0,"cond":"clear"}],
                     "min_lo":15.0,"max_hi":24.0},
            "sun": {"rise":5.1,"set":18.9}, "moon": {"phase":0.5,"illum":50.0},
            "scene": "https://x/y.jpg", "city":"", "units":"c", "days":7.0,
            "env": {"locale":{"lang":"en","temp_unit":"c"}}
        })
    }

    /// The counts both evaluators must agree on, read from the file that owns
    /// them rather than copied into a literal.
    const CONFORMANCE: &str =
        include_str!("../../../../../Splash-Makepad/components/l0/conformance.txt");

    fn expected(card: &str) -> usize {
        CONFORMANCE
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .find_map(|l| {
                let (name, n) = l.split_once(char::is_whitespace)?;
                (name == card).then(|| n.trim().parse().ok())?
            })
            .unwrap_or_else(|| panic!("no conformance entry for {card:?}"))
    }

    /// **The point of this test.** Two independent evaluators, two VM lineages,
    /// one tree.
    ///
    /// `splash-render` walks the DSL with the makepad-splash VM; this walks it
    /// with the app's own, because taking `splash-render` here fails the
    /// lockfile. The model is shared and only the walk is duplicated — and a
    /// duplicated walk drifts unless something checks it. Neither repository can
    /// run the other's evaluator, so the check is a number both read.
    ///
    /// The first version of this test carried those numbers as literals copied
    /// from a note, and they were stale the first time they ran: they predated
    /// the tap wrappers, which add a node per declared tap. It reported a
    /// divergence between the two evaluators that did not exist. Reading the
    /// shared file cannot fail that way.
    #[test]
    fn this_vm_produces_the_same_tree_as_the_reference_evaluator() {
        for (name, tree) in [
            ("news", build_card(NEWS, news_data())),
            ("stock", build_card(STOCK, stock_data())),
            ("weather", build_card(WEATHER, weather_data())),
        ] {
            assert_eq!(
                tree.count(),
                expected(name),
                "{name}: this VM produced {} nodes, splash-render produces {}",
                tree.count(),
                expected(name)
            );
        }
    }

    /// The card's text survives the second evaluator too.
    ///
    /// A tree of the right SIZE carrying no words would pass the count check
    /// exactly, and render as a blank card.
    #[test]
    fn the_second_evaluator_carries_the_cards_text() {
        fn words(n: &splash_node::UiNode, out: &mut Vec<String>) {
            if let Some(t) = n.attrs.text.as_deref() {
                out.push(t.to_owned());
            }
            for c in &n.children {
                words(c, out);
            }
        }
        let mut out = Vec::new();
        words(&build_card(NEWS, news_data()), &mut out);
        let text = out.join(" | ");
        // The card's OWN words — `copy` declarations, authored in the ledger.
        //
        // A seeded headline used to be here too. It is not any more: `sys.news`
        // is answered live now, so a story title lowers to the call rather than
        // to the blob this test hands in, and a bare VM has no `sys` to run it.
        // What this test guards is that words survive the second evaluator at
        // all — a right-sized tree of empty nodes would pass the count check
        // beside it — and the card's own copy proves that without depending on
        // a value the backend now fetches.
        for expected in ["HACKER NEWS", "Top Stories"] {
            assert!(text.contains(expected), "{expected:?} missing from: {text}");
        }
    }

    /// A declared tap survives evaluation carrying its instance key.
    ///
    /// The key is what lets a tap say WHICH row was hit — §5.1 identity is the
    /// basis of dispatch, and an evaluator that dropped `tapto` would produce a
    /// tree of exactly the right shape that no one can interact with.
    #[test]
    fn a_tap_target_survives_evaluation() {
        fn taps(n: &splash_node::UiNode, out: &mut Vec<String>) {
            if let Some(t) = n.attrs.tapto.as_deref() {
                out.push(t.to_owned());
            }
            for c in &n.children {
                taps(c, out);
            }
        }
        let mut out = Vec::new();
        taps(&build_card(STOCK, stock_data()), &mut out);
        // TWO: the header's + chip and the mover row that opens the quote.
        // Movers carry no per-row menu — Add lives on the quote page,
        // which the fixture leaves closed (selected is empty).
        assert_eq!(out.len(), 2, "+ chip and a row tap: {out:?}");
        for t in &out {
            assert!(t.starts_with("l0:"), "the L0 prefix marks it: {out:?}");
        }
        assert_eq!(
            out.iter().filter(|t| t.contains("for#0[NVDA]")).count(),
            1,
            "the row alone carries the row's key: {out:?}"
        );
        assert!(
            out.iter().any(|t| t.contains("open_quote"))
                && out.iter().any(|t| t.contains("\"e\":\"add_sym\"")),
            "both events present: {out:?}"
        );
    }

    /// A live source needs its capability registered, or it renders an ERROR.
    ///
    /// The kit lowers a source this backend can answer into a `sys.*` call, and
    /// on a bare VM that call is undefined — the concatenation around it yields
    /// `$[Error:WrongValue]`, which then draws where the price should be. Loud
    /// rather than silent, and still wrong, so the capability registration is
    /// part of the contract rather than an optimisation.
    #[test]
    fn a_live_source_without_its_capability_is_visibly_wrong() {
        let mut store = splash_ui_l0::InstanceStore::default();
        splash_ui_l0::dispatch_with(
            STOCK, &mut store, "root", "open_quote",
            Some(&serde_json::Value::String("NVDA".into())));
        let r = splash_ui_l0::realize_with_state(STOCK, &stock_data(), &store, RealizeLimits::default());
        let src = format!("{KIT}\n{}", kit::lower(&r.root.expect("root")));

        let bare = super::build_without_capabilities(&src).expect("still evaluates");
        let mut out = Vec::new();
        fn words(n: &splash_node::UiNode, out: &mut Vec<String>) {
            if let Some(t) = n.attrs.text.as_deref() { out.push(t.to_owned()); }
            for c in &n.children { words(c, out); }
        }
        words(&bare, &mut out);
        assert!(
            out.iter().any(|w| w.contains("Error")),
            "an unregistered capability must be VISIBLE, not silently empty: {out:?}"
        );
    }

    /// An unknown root tag is `None`, never an empty tree.
    ///
    /// A card must fail loudly. Returning a bare node would render a blank
    /// screen that looks like a layout bug rather than a rejected card.
    #[test]
    fn an_unknown_root_is_refused() {
        assert!(super::build_without_capabilities("let n = {t:\"hologram\"}\nn\n").is_none());
    }
}

#[cfg(test)]
mod cond_tests {
    /// A WeatherIcon's condition is a NUMBER, and it must survive both hops.
    ///
    /// The forecast returns a WMO code; the kit stores it as `variant`, which is
    /// a `String` on the shared node model. Whether the VM coerces a number into
    /// that slot is not something to assume — the card that got this wrong drew
    /// seven identical icons over a week that was cloudy, rainy and clear, and
    /// nothing distinguished that from a week of identical weather.
    #[test]
    fn a_numeric_condition_reaches_the_node() {
        let tree = super::build_without_capabilities(
            "let node = {t: \"weathericon\", variant: 2, w: 34, h: 34}\nnode\n",
        )
        .expect("evaluates");
        assert_eq!(
            tree.attrs.variant.as_deref(),
            Some("2"),
            "a numeric `variant` must reach the node as its digits"
        );
    }
}

#[cfg(test)]
mod nav_dsl {
    //! Emit the nav card's FINAL DSL through the DEVICE's own path.
    //!
    //! `kit::lower` -> `_kit.splash` -> this VM -> `l0_widgets::to_dsl` is what a
    //! generated card becomes on a phone. `makepad::lower` is a different backend,
    //! so verifying a layout fix there proves nothing about the device — which is
    //! the mistake this exists to stop repeating: a map-card layout fix was
    //! screenshot-verified through `makepad::lower` while the device path still
    //! stacked the map in a column. Writing the DSL to a file lets it be pushed to
    //! a running app with no rebuild.
    use splash_ui_l0::{kit, realize, RealizeLimits};

    const NAV: &str = include_str!(
        "../../../../../Splash/crates/splash-ui-l0/tests/fixtures/nav.card"
    );
    const KIT: &str = super::super::l0_card::KIT_SRC;

    fn data(screen: &str) -> serde_json::Value {
        serde_json::json!({
            "origin": "Saratoga High School", "dest": "Stanford University",
            "query": "", "screen": screen, "found": [],
            "here": { "lat": -9999, "lon": -9999, "accuracy": 0, "ok": 0 },
            "origin_place": [{ "id": "o", "name": "S", "lat": -9999, "lon": -9999 }],
            "dest_place": [{ "id": "d", "name": "S", "lat": -9999, "lon": -9999 }],
            "trip": { "duration": "SEEDED", "distance": "SEEDED" },
            "step": { "instruction": "SEEDED", "remaining": "SEEDED" },
            "env": { "locale": { "lang": "en" } },
            "copy": { "where": "Where to?", "from": "FROM", "to": "TO",
                      "here_now": "Starting from…", "away": "away",
                      "seeking": "Finding a route…", "start": "Go",
                      "stop": "End", "left": "left" }
        })
    }

    #[test]
    #[ignore = "writes files for the device harness; run explicitly"]
    fn write_nav_dsl() {
        for (name, screen) in [("nav-plan", "plan"), ("nav-drive", "drive")] {
            let report = realize(NAV, &data(screen), RealizeLimits::default());
            assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
            let root = report.root.expect("a realized tree");
            let src = format!("{KIT}\n{}", kit::lower(&root));
            let tree = super::build_without_capabilities(&src)
                .expect("the lowered card evaluated to nil");
            let dsl = super::super::l0_widgets::to_dsl(&tree);
            let out = std::path::Path::new("/tmp").join(format!("kit-{name}.splash"));
            std::fs::write(&out, format!("// name: l0-card\n{dsl}")).expect("write");
            println!("--- {name}: {} bytes -> {}", dsl.len(), out.display());
            for line in dsl.lines().take(4) {
                println!("---   {}", line.chars().take(150).collect::<String>());
            }
        }
    }
}

#[cfg(test)]
mod kit_palette {
    //! A role the kit fills must reach the tree with a fill.
    //!
    //! `l0_panel` was BOTH a palette colour and a role function in `_kit.splash`.
    //! From `fn l0_panel`'s declaration onward the name resolved to the function,
    //! and a function coerced to a colour is 0 — so `l0_chip` and `l0_field` both
    //! asked for that fill and both got fully transparent. Uses inside
    //! `l0_panel`'s own body still resolved to the colour, so panels looked right
    //! and chips did not, which is why it read as a chip bug.
    //!
    //! On screen: a nav card's Go and End, and the stock card's unselected range
    //! chips, rendered as bare text with no button under them. Nothing was missing
    //! and nothing was misplaced — they just did not look tappable. Found by
    //! live-testing on a phone, and invisible to every test here, because zero is
    //! a perfectly valid colour.
    //!
    //! So this asserts the tree, where the collision is observable, and compares a
    //! chip against a panel rather than against a constant: the two are meant to
    //! carry the same fill, and a rename that misses one of them fails here.
    use splash_ui_l0::{kit, realize, RealizeLimits};

    fn find<'a>(n: &'a splash_node::UiNode, k: splash_node::NodeKind) -> Option<&'a splash_node::UiNode> {
        if n.kind == k {
            return Some(n);
        }
        n.children.iter().find_map(|c| find(c, k))
    }

    #[test]
    fn a_filled_role_reaches_the_tree_with_its_fill() {
        const CARD: &str = concat!(
            "copy g { class: vocabulary, en: \"Go\" }\n",
            "state s { shape: bool, initial: false }\n",
            "event e { s: toggle }\n",
            "view root Surface { Chip(text: copy.g, on_tap: e) Panel { Rule() } }\n"
        );
        let data = serde_json::json!({
            "copy": { "g": "Go" }, "s": false, "env": { "locale": {} }
        });
        let root = realize(CARD, &data, RealizeLimits::default())
            .root
            .expect("realizes");
        let src = format!(
            "{}\n{}",
            super::super::l0_card::KIT_SRC,
            kit::lower(&root)
        );
        let tree = super::build_without_capabilities(&src).expect("evaluated to nil");

        let chip = find(&tree, splash_node::NodeKind::Chip).expect("a chip");
        let panel = find(&tree, splash_node::NodeKind::Card).expect("a panel");
        assert_eq!(
            chip.attrs.bg, panel.attrs.bg,
            "a chip and a panel carry the same fill; a chip of {:?} against a panel \
             of {:?} means the palette name was shadowed",
            chip.attrs.bg, panel.attrs.bg
        );
        assert!(
            chip.attrs.bg.is_some_and(|c| c != 0),
            "and it must not be transparent — a chip with no fill is a label"
        );
    }
}

#[cfg(test)]
mod numeric_text {
    //! A card that binds a declared NUMBER to text renders the number.
    //!
    //! `string_prop` returns None for a number, so a text node whose value was one
    //! arrived with no text: present, laid out, and blank. Found with a card
    //! reading `sys.gps`, which answers numbers — four labelled rows, four empty
    //! values, and no way to tell a missing fix from a broken call.
    //!
    //! The identical bug had already been fixed one slot over, for `variant`, after
    //! the kit's `variant: 2` arrived as nothing and every weather icon fell back
    //! to a default sun. Two slots on the same model take the same shape of value
    //! and only one of them tolerated it.
    //!
    //! Built from a REAL card through `kit::lower`. The first version of this test
    //! handed the VM a synthetic `l0_col([l0_value(42)])` tail, which evaluates to
    //! nil there, so it took its own skip path and passed with the fix reverted.
    use splash_ui_l0::{kit, realize, RealizeLimits};

    #[test]
    fn a_number_bound_to_text_reaches_the_tree_as_text() {
        const CARD: &str = concat!(
            "state n { shape: number, initial: 42 }\n",
            "view root Surface { TextValue(value: n) }\n"
        );
        let data = serde_json::json!({ "n": 42.0, "env": { "locale": {} }, "copy": {} });
        let root = realize(CARD, &data, RealizeLimits::default())
            .root
            .expect("realizes");
        // The number has to be UNQUOTED in the kit source, and realization cannot
        // produce that: it stringifies a declared value, so `TextValue(value: n)`
        // lowers to `l0_value("42")` and reads identically through either property
        // reader. The numeric case only arises from a live `sys.*` call, which a
        // bare VM has no `sys` to run.
        //
        // So the quotes are removed from real lowered output. That is exactly the
        // shape `sys.gps("lat")` hands the kit on a device, and it is the only way
        // to reach it from here — the first two versions of this test passed with
        // the fix reverted, one through a skip path and one because it was
        // asserting on a string all along.
        let lowered = kit::lower(&root).replace("l0_value(\"42\")", "l0_value(42)");
        assert!(
            lowered.contains("l0_value(42)"),
            "the probe must actually carry a bare number:\n{lowered}"
        );
        let src = format!("{}\n{}", super::super::l0_card::KIT_SRC, lowered);
        let tree = super::build_without_capabilities(&src).expect("evaluated to nil");

        fn words(n: &splash_node::UiNode, out: &mut Vec<String>) {
            if let Some(t) = n.attrs.text.as_deref() {
                out.push(t.to_owned());
            }
            for c in &n.children {
                words(c, out);
            }
        }
        let mut out = Vec::new();
        words(&tree, &mut out);
        assert!(
            out.iter().any(|w| w == "42"),
            "a declared number must reach the tree as text, and print without a \
             fraction; got {out:?}"
        );
    }
}
