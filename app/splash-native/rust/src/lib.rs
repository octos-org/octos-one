//! octos-one's Splash cards, rendered as native Android views.
//!
//! The experiment: take the card octos-one's LLM produced and render it with **no
//! makepad renderer anywhere in the path**.
//!
//! ```text
//! octos-one LLM  ─►  plan  ─►  Splash DSL card      (octos-one, already shipping)
//!                                    │
//!                                    ▼
//!                         splash-core VM            ymote/Splash — vendored
//!                         (language only)           makepad-script, no UI code
//!                                    │
//!                                    ▼
//!                            node tree              {kind, attrs, children}
//!                                    │
//!                                    ▼  one JNI crossing, one flat buffer
//!                     android.widget.* / Material   ymote/Splash-Android's design
//! ```
//!
//! ## What this deliberately does NOT depend on
//!
//! `makepad-widgets`, `makepad-draw`, `makepad-platform` — the whole aichat render
//! stack. `splash-core` re-exports the pinned VM and nothing else, so the DSL survives
//! and the renderer does not come with it. If any of those appeared in this crate's
//! dependency tree the experiment would be answering a different question.
//!
//! ## Ownership
//!
//! Java owns every `View`; Rust owns the buffer. No `jobject` is held across a call, so
//! ART's 512-local-reference abort and the `FindClass` classloader trap — the two traps
//! that make naive JNI view-building fail — are structurally unreachable rather than
//! merely avoided.
//!
//! ## The cards it reads
//!
//! Whatever octos-one saved. The app pulls
//! `/data/data/dev.makepad.octos_app/files/a2app_cards/*.splash` when it can, else a
//! bundled copy — so what renders here is the SAME text the LLM produced, not a
//! hand-written approximation.

use splash_core::vm as ms;

use ms::apply::*;
use ms::array::ScriptArrayStorage;
use ms::makepad_live_id::*;
use ms::traits::*;
use ms::*;

use jni::objects::{JClass, JObject, JString};
use jni::JNIEnv;
use std::collections::BTreeMap;
use std::sync::Mutex;

mod fetch;
mod node;

use node::{encode, Node, Val};

static BUF: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static DIAG: Mutex<Option<String>> = Mutex::new(None);

fn diag(msg: impl Into<String>) {
    *DIAG.lock().unwrap() = Some(msg.into());
}

// ------------------------------------------------------------------ vocab ----

/// Attributes carried as STRINGS. Explicit rather than discovered, because `LiveId`
/// keys are one-way hashes — there is no way to enumerate what a card set, so the
/// vocabulary has to be declared. An attribute missing here is silently dropped, which
/// is why adding a node kind means adding its attributes in the same change.
const ATTRS_S: &[(&str, LiveId)] = &[
    ("text", live_id!(text)),
    ("variant", live_id!(variant)),
    ("label", live_id!(label)),
    ("src", live_id!(src)),
    ("icon", live_id!(icon)),
];

/// Attributes carried as NUMBERS.
const ATTRS_N: &[(&str, LiveId)] = &[
    ("w", live_id!(w)),
    ("h", live_id!(h)),
    ("pad", live_id!(pad)),
    ("spacing", live_id!(spacing)),
    ("gap", live_id!(gap)),
    ("radius", live_id!(radius)),
    ("bg", live_id!(bg)),
    ("color", live_id!(color)),
    ("size", live_id!(size)),
    ("weight", live_id!(weight)),
    ("cond", live_id!(cond)),
    ("value", live_id!(value)),
    ("max", live_id!(max)),
    ("grow", live_id!(grow)),
    // `align` is a NUMBER (0 = start, 1 = centre). It was in the string list, so the
    // builder's `n.f("align")` never saw it and the hero stayed hard left however many
    // times the lowering asked for centring. An attribute in the wrong list is not a
    // parse error — it is silently the wrong type, which is why it took a render to spot.
    ("align", live_id!(align)),
];

fn sprop(vm: &mut ScriptVm, v: ScriptValue, id: LiveId) -> Option<String> {
    let p = vm.bx.heap.value_for_apply(v, id.into(), &Apply::Eval)?;
    if p.is_nil() {
        return None;
    }
    let mut s = String::new();
    vm.bx.heap.cast_to_string(p, &mut s);
    Some(s)
}

fn nprop(vm: &mut ScriptVm, v: ScriptValue, id: LiveId) -> Option<f64> {
    let p = vm.bx.heap.value_for_apply(v, id.into(), &Apply::Eval)?;
    p.as_number()
}

/// Positional children — a node's `c: [...]` array.
fn children_of(vm: &mut ScriptVm, v: ScriptValue) -> Vec<ScriptValue> {
    let Some(c) = vm.bx.heap.value_for_apply(v, live_id!(c).into(), &Apply::Eval) else {
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

/// A VM value → a node. Depth-capped: a card is generated text, and a cyclic or absurdly
/// deep tree must fail as a bounded miss rather than a stack overflow in the renderer.
fn walk(vm: &mut ScriptVm, value: ScriptValue, depth: usize) -> Option<Node> {
    if depth > 48 {
        return None;
    }
    let kind = sprop(vm, value, live_id!(t))?;
    let mut attrs = Vec::new();
    for (name, id) in ATTRS_S {
        if let Some(s) = sprop(vm, value, *id) {
            attrs.push((name.to_string(), Val::S(s)));
        }
    }
    for (name, id) in ATTRS_N {
        if let Some(n) = nprop(vm, value, *id) {
            attrs.push((name.to_string(), Val::F(n)));
        }
    }
    let children = children_of(vm, value)
        .into_iter()
        .filter_map(|k| walk(vm, k, depth + 1))
        .collect();
    Some(Node {
        kind,
        attrs,
        children,
    })
}

// ------------------------------------------------------------------ eval ----

fn text_node(kind_hint: &str, msg: &str) -> Node {
    Node::new("col").n("pad", 20.0).n("spacing", 8.0).kids(vec![
        Node::new("text")
            .s("text", kind_hint)
            .s("variant", "headlineSmall"),
        Node::new("text").s("text", msg).s("variant", "bodyMedium"),
    ])
}

/// Evaluate Splash source and walk the result into a node tree.
///
/// The host functions the card may call are registered by [`fetch`] — they mirror
/// octos-one's `sys.*` helpers, because the CARD IS OCTOS-ONE'S and calls them by name.
/// That is the whole point of reading the real card rather than a transcription: if a
/// helper is missing here, the card says so instead of quietly rendering an em dash.
fn eval_to_nodes(src: &str) -> Node {
    // The VM is built by hand rather than via a constructor: `ScriptVm` borrows its
    // host and std slots, so they must outlive it on the caller's stack.
    let mut std_slot = 0;
    let mut host = 0;
    let vm = &mut ScriptVm {
        host: &mut host,
        std: &mut std_slot,
        bx: Box::new(ScriptVmBase::new()),
    };
    fetch::register(vm);

    let value = vm.eval(ScriptMod {
        cargo_manifest_path: String::new(),
        module_path: String::from("octos"),
        file: String::from("card.splash"),
        line: 0,
        column: 0,
        code: src.to_string(),
        values: Vec::new(),
    });
    if value.is_err() || value.is_nil() {
        diag("the card evaluated to nil or an error");
        return text_node(
            "Card did not evaluate",
            "the Splash source did not produce a value — see logcat for the VM error",
        );
    }
    match walk(vm, value, 0) {
        Some(n) => n,
        None => {
            diag("card evaluated but produced no node tree (missing `t:` on the root?)");
            text_node(
                "No node tree",
                "the card evaluated but its root has no `t:` type tag",
            )
        }
    }
}

// ------------------------------------------------------------------- JNI ----

fn jstr(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(Into::into).unwrap_or_default()
}

/// Evaluate a Splash card and return its node tree as a direct buffer.
///
/// Never returns null for a bad card: it renders a visible explanation instead, because
/// a blank screen is indistinguishable from a crash.
#[no_mangle]
pub extern "system" fn Java_dev_octos_splashnative_Native_renderCard<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    src: JString<'l>,
) -> JObject<'l> {
    *DIAG.lock().unwrap() = None;
    let source = jstr(&mut env, &src);
    let root = eval_to_nodes(&source);
    let buf = encode(&root);
    let mut g = BUF.lock().unwrap();
    *g = Some(buf);
    let b = g.as_ref().unwrap();
    match unsafe { env.new_direct_byte_buffer(b.as_ptr() as *mut u8, b.len()) } {
        Ok(v) => v.into(),
        Err(_) => JObject::null(),
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_octos_splashnative_Native_diag<'l>(
    env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> JObject<'l> {
    let d = DIAG.lock().unwrap().clone().unwrap_or_default();
    env.new_string(d)
        .map(Into::into)
        .unwrap_or_else(|_| JObject::null())
}

/// Which `sys.*` helpers this backend implements, for the on-screen capability report.
///
/// octos-one exposes about thirty; this has the subset the weather and news cards
/// actually call. Naming the gap beats discovering it as a card full of em dashes.
#[no_mangle]
pub extern "system" fn Java_dev_octos_splashnative_Native_capabilities<'l>(
    env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> JObject<'l> {
    env.new_string(fetch::CAPABILITIES.join(", "))
        .map(Into::into)
        .unwrap_or_else(|_| JObject::null())
}

/// Cache stats, so a reviewer can see that N field reads cost one request rather than N.
#[no_mangle]
pub extern "system" fn Java_dev_octos_splashnative_Native_fetchStats<'l>(
    env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> JObject<'l> {
    env.new_string(fetch::stats())
        .map(Into::into)
        .unwrap_or_else(|_| JObject::null())
}

// Kept so the map type is named once; `fetch` owns the cache itself.
type _Cache = BTreeMap<String, String>;
