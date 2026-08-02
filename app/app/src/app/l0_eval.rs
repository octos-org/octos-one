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

/// Evaluate `src` and walk it into a `UiNode` tree.
///
/// Returns `None` when the script evaluates to nil — a parse or runtime error —
/// or when the root tag is not one the model knows. A card must never render as
/// a silent absence, so a caller reports `None` rather than drawing nothing.
pub fn build(src: &str, register: impl FnOnce(&mut ScriptVm)) -> Option<UiNode> {
    let mut std_slot = 0;
    let mut host = 0;
    let vm = &mut ScriptVm {
        host: &mut host,
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
        text: string_prop(vm, value, id!(text)),
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
        spacing: f32_prop(vm, value, id!(spacing)),
        margin: f32_prop(vm, value, id!(margin)),
        marginx: f32_prop(vm, value, id!(marginx)),
        marginy: f32_prop(vm, value, id!(marginy)),
        border: f32_prop(vm, value, id!(border)),
        bordercolor: u32_prop(vm, value, id!(bordercolor)),
        variant: string_prop(vm, value, id!(variant)),
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
        tilt: num_prop(vm, value, id!(tilt)),
        rotation: num_prop(vm, value, id!(rotation)),
        x: num_prop(vm, value, id!(x)),
        y: num_prop(vm, value, id!(y)),
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
        super::build(&src, |_vm| {}).expect("the lowered card evaluated to nil")
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
        for expected in ["HACKER NEWS", "Top Stories", "Rust 1.95"] {
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
        assert_eq!(out.len(), 1, "one declared tap, one target: {out:?}");
        assert!(out[0].starts_with("l0:"), "the L0 prefix marks it: {out:?}");
        assert!(out[0].contains("for#0[NVDA]"), "instance key lost: {out:?}");
        assert!(out[0].contains("open_quote"), "event name lost: {out:?}");
    }

    /// An unknown root tag is `None`, never an empty tree.
    ///
    /// A card must fail loudly. Returning a bare node would render a blank
    /// screen that looks like a layout bug rather than a rejected card.
    #[test]
    fn an_unknown_root_is_refused() {
        assert!(super::build(r#"let n = {t:"hologram"}
n
"#, |_vm| {}).is_none());
    }
}
