//! An L0 card that handles its own taps, on device.
//!
//! Everything in the L0 profile has until now been verified against itself:
//! the card parses, realizes, lowers, and four goldens render on the 6T. None
//! of that proves a tap works, and a tap is what the whole design rests on —
//! `ui-profile-l0.md` §5.10.1 argues about instance identity crossing a layer
//! boundary, and `scoped-state.md` §10 names "one card, one toggle, on device"
//! as the thing that would prove the contract wrong.
//!
//! So this is that walking skeleton. The loop is:
//!
//! ```text
//!   tap  ->  agent.notify("l0", {key, event, value})   (lowered hit target)
//!        ->  SplashAction::Notify                       (host receives)
//!        ->  dispatch_with_data                         (transition applies)
//!        ->  realize + kit::lower + kit + eval + widgets   (new tree)
//!        ->  replace the card body, redraw
//! ```
//!
//! **Sessions are keyed by chat item.** This held exactly one while the open
//! question was whether a tap completes the circuit at all — a defensible
//! scope for a skeleton, and a bug the moment the circuit worked: a second card
//! replaced the first one's session, so scrolling back left an earlier card
//! rendering perfectly and completely inert. A conversation is a list of cards
//! and every one of them stays live.

use std::collections::BTreeMap;
use std::sync::RwLock;

/// The theme kit, baked in. §1.1's middle layer: the card names roles and this
/// answers them, so no colour or size is decided in Rust.
const KIT: &str = include_str!("../../../../../Splash-Makepad/components/l0/_kit.splash");

/// The kit source, for tests that need to reproduce the DEVICE's exact chain.
#[cfg(test)]
pub(super) const KIT_SRC: &str = KIT;

/// Realize, lower to role calls, evaluate, and render as widgets.
///
/// This is §1.1 end to end. What it replaces — `makepad::lower` — went straight
/// from a realized tree to this backend's widget dialect with ten hardcoded
/// colours and a font-size ramp, so it decided what a theme decides and reached
/// one backend of three.
fn render_through_kit(
    cx: &mut makepad_widgets::Cx,
    source: &str,
    data: &serde_json::Value,
    store: &splash_ui_l0::InstanceStore,
) -> Result<String, String> {
    // Durable collections join the data here, so a `for` over one iterates the
    // rows the store actually holds (see `with_durable`).
    let mut data = with_durable(source, data);
    // And so do FETCHED lists. A `for` iterates the data, so a list's length has to
    // be in it — the one thing a live call cannot supply, because length is
    // structural. This is the narrow half of §5.9 the results panel needs.
    let plan = splash_ui_l0::source_plan(source);
    for request in &plan.requests {
        // A declared PREFERENCE source is seeded from the store, with a host
        // default for anything unset — measured: an empty capture into an enum
        // state leaves junk the guards all fail against, so a card whose mode
        // reads `mode_pref.mode` would lose every `mode == .drive` branch on a
        // fresh install. The host owns the store, so the host owns its
        // defaults, exactly as it owns locale's "en"/"c".
        if request.helper == "sys.prefs" {
            let prefs = super::user_store::get().prefs;
            let mut obj = serde_json::Map::new();
            for field in splash_ui_l0::catalog::answers("sys.prefs").unwrap_or(&[]) {
                let stored = prefs.get(*field).cloned();
                let value = stored.unwrap_or_else(|| match *field {
                    "mode" => "drive".to_owned(),
                    "range" => "d1".to_owned(),
                    // Units follow the device until the user chooses.
                    "units" => String::new(),
                    _ => String::new(),
                });
                obj.insert((*field).to_owned(), serde_json::Value::String(value));
            }
            if let Some(map) = data.as_object_mut() {
                map.insert(request.name.clone(), serde_json::Value::Object(obj));
            }
            continue;
        }
        let answered = fetched_rows(cx, request, &data, store)
            .map(serde_json::Value::Array)
            .or_else(|| fetched_scalars(cx, request));
        let Some(value) = answered else { continue };
        if let Some(map) = data.as_object_mut() {
            map.insert(request.name.clone(), value);
        }
    }
    let data = &data;
    let report = splash_ui_l0::realize_with_state(
        source,
        data,
        store,
        splash_ui_l0::RealizeLimits::default(),
    );
    LAST_CAPTURED.with(|c| *c.borrow_mut() = Some(report.captured.clone()));
    let Some(root) = report.root else {
        return Err(report
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; "));
    };
    // What each source's fetch is doing, for the NEXT realize to read as `$state`.
    // After realize because a source's arguments are only resolved here.
    observe_source_states(cx, source, &root);
    let src = format!("{KIT}\n{}", splash_ui_l0::kit::lower(&root));
    // With capabilities: the kit lowers a source this backend can answer into a
    // `sys.*` call, and on a bare VM that call is undefined — the concatenation
    // around it yields `$[Error:WrongValue]`, which then draws as the price.
    let tree = super::l0_eval::build_with_capabilities(cx, &src)
        .ok_or_else(|| "the lowered card evaluated to nil".to_owned())?;
    Ok(super::l0_widgets::to_dsl(&tree))
}

/// The card, its data, and its live state.
///
/// The store is what makes a tap local: it holds the cells a transition writes,
/// keyed by instance, and outlives the tree that is rebuilt around it.
pub struct L0Session {
    /// The L0 ledger source — re-realized on every dispatch.
    pub source: String,
    /// Host-supplied data. Static for the skeleton; §5.9 invalidation and
    /// refetch are a separate step and deliberately not attempted here.
    pub data: serde_json::Value,
    /// Live cells. Survives the rebuild — which is the point.
    pub store: splash_ui_l0::InstanceStore,
    /// Which chat message holds the rendered card, so a redraw replaces the
    /// right one rather than appending a second copy per tap.
    pub item: usize,
}

/// Every live card, keyed by the message that holds it.
///
/// This was an `Option`, so the app held exactly ONE. Asking a second question
/// replaced the first card's session, and scrolling back to the earlier card
/// left it rendering fine and completely inert — its taps landed on the newer
/// card's ledger or on nothing at all. A conversation is a list of cards and
/// every one of them stays interactive, so the session is a map.
///
/// `card_id` in `agent.notify("<card_id>:l0kit", …)` is this key: `tag_notify_calls`
/// stamps each card's own message index into the channel, so a tap already
/// carries which card it came from.
static SESSIONS: RwLock<BTreeMap<usize, L0Session>> = RwLock::new(BTreeMap::new());

/// Realize and lower a card for display. Returns the Splash DSL, or the
/// diagnostics if the card does not realize.
///
/// Diagnostics are returned rather than logged because a card that fails to
/// realize on device is otherwise a blank screen with the reason in logcat,
/// which is precisely the failure mode the device harness exists to catch.
pub fn render(
    cx: &mut makepad_widgets::Cx,
    source: &str,
    data: &serde_json::Value,
    store: &splash_ui_l0::InstanceStore,
) -> Result<String, String> {
    render_through_kit(cx, source, data, store)
}

/// Seed the durable collections into a card's data.
///
/// A `for` over a source realizes as many rows as the data holds, and
/// `sys.watchlist` takes no `count:` — its length IS the store's. Without this
/// the loop iterates nothing and a saved list renders as an empty panel, which
/// looks exactly like a list nobody has added to.
///
/// Only the REFERENCES go in. Every other field the card asked for lowers to a
/// live call, so the row's values are fetched rather than read from here — the
/// blob supplies identity and shape, never a fact.
/// Evaluate one expression through the card VM and read the string back.
///
/// The `sys.*` capabilities live in the VM, so this is how the host asks one a
/// question outside of rendering. A text node is the smallest thing `build` will
/// return, and its `text` is the answer.
fn eval_text(cx: &mut makepad_widgets::Cx, expr: &str) -> Option<String> {
    let src = format!("{KIT}\nreturn {{t: \"text\", text: \"\" + {expr}}}");
    super::l0_eval::build_with_capabilities(cx, &src)?.attrs.text
}

/// How many rows a LIST source has, and what identifies each of them.
///
/// A `for` iterates the DATA: a list's length is structural, so unlike a scalar it
/// cannot come from a live call at render time. Without this the results panel of
/// any card that searches is permanently empty — the query is set, the fetch runs,
/// and the loop has nothing to walk.
///
/// Only the KEY goes in, exactly as `with_durable` does for a saved collection:
/// the realizer needs one field per row to give the `for` its keys, and every other
/// field the card asked for lowers to a live call. So this adds identity and never
/// a fact.
///
/// Capped by the card's declared `count:`, because that is the card saying how many
/// it is prepared to show.
fn fetched_rows(
    cx: &mut makepad_widgets::Cx,
    request: &splash_ui_l0::SourceRequest,
    data: &serde_json::Value,
    store: &splash_ui_l0::InstanceStore,
) -> Option<Vec<serde_json::Value>> {
    // One capability for now, and named rather than inferred: `sys.search` is the
    // only list the backend answers by index today. A table here beats a guess.
    let (key_field, count_helper) = match request.helper.as_str() {
        // Keyed on the LABEL, because the name is not unique: a search for
        // "Stanford" answers five rows all named that, and identity has to be the
        // line that tells them apart.
        "sys.search" => ("label", "sys.searchnum"),
        _ => return None,
    };
    let query = match request.args.iter().find(|(n, _)| n == "query")?.1.clone() {
        splash_ui_l0::SourceArg::Text(t) => t,
        // A path is card state — the search box's own value.
        splash_ui_l0::SourceArg::Path(p) => {
            let name = p.strip_prefix("state.").unwrap_or(&p);
            // Through the CONSTANT. I wrote `"root"` here from memory and it is
            // `"@card"`, so every lookup missed the store, fell through to the blob's
            // empty query, and the list stayed empty with everything else working.
            store
                .get(splash_ui_l0::CARD_STATE_KEY, name)
                .or_else(|| data.get(name))
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        }
        _ => return None,
    };
    // An empty query is not a search. Asking anyway costs a request and answers
    // nothing, and an empty list is what the card should show for it.
    if query.trim().is_empty() {
        return Some(Vec::new());
    }
    let cap = request
        .args
        .iter()
        .find(|(n, _)| n == "count")
        .and_then(|(_, a)| match a {
            splash_ui_l0::SourceArg::Number(n) => Some(*n as usize),
            _ => None,
        })
        .unwrap_or(0);
    let found: usize = eval_text(cx, &format!("{count_helper}({query:?}, 0, \"count\")"))?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|n| *n >= 0.0)? as usize;
    let mut rows = Vec::new();
    for i in 0..found.min(cap) {
        let Some(key) = eval_text(cx, &format!("sys.search({query:?}, {i}, {key_field:?})")) else {
            break;
        };
        // A blank key past the last hit, or one still loading: stop rather than
        // pad the list with rows that draw as nothing.
        if key.trim().is_empty() {
            break;
        }
        rows.push(serde_json::json!({ key_field: key }));
    }
    Some(rows)
}

/// A scalar source's fields, answered into the card's data.
///
/// The other half of §5.9's write-back. `fetched_rows` above gives a `for` its rows;
/// this gives a `state` something to initialise FROM and a transition something to
/// capture. Without it a card can render a live value and can never take one: the
/// data holds whatever it was seeded with, so `initial: here.lat` reads nothing and
/// `set(here.lat)` writes nothing.
///
/// Scoped to `sys.gps`, and deliberately. It is the device's own state — local, free
/// to ask, and answered synchronously — so writing it on every resolve costs
/// nothing. Doing the same for a NETWORK source would fire a request per field per
/// render, which is the sort of thing that looks fine on a desk and melts a phone.
/// Widening this needs a fetch policy, not another arm in this match.
fn fetched_scalars(
    cx: &mut makepad_widgets::Cx,
    request: &splash_ui_l0::SourceRequest,
) -> Option<serde_json::Value> {
    if request.helper != "sys.gps" {
        return None;
    }
    // A fix we do not have is not a fact. `sys.gps` answers -9999 for a coordinate it
    // lacks, and writing that into a card's data would let a state CAPTURE it — a
    // frozen origin in the Gulf of Guinea that no later fix could dislodge. Absent is
    // the honest answer, and a card reading `here.$state` can say so.
    if eval_text(cx, "sys.gps(\"ok\")")?.trim() != "1" {
        return None;
    }
    let mut out = serde_json::Map::new();
    for field in splash_ui_l0::catalog::answers(&request.helper)? {
        let binding = splash_ui_l0::SourceBinding {
            helper: request.helper.clone(),
            args: Vec::new(),
            field: (*field).to_owned(),
        };
        // Through the BACKEND's own translation, so the host cannot drift from the
        // call the lowering would have emitted for the same field.
        let Some(call) = splash_ui_l0::makepad::vm_call(&binding) else {
            continue;
        };
        let Some(text) = eval_text(cx, &call) else {
            continue;
        };
        let value = match text.trim().parse::<f64>() {
            Ok(n) => serde_json::json!(n),
            Err(_) => serde_json::Value::String(text),
        };
        out.insert((*field).to_owned(), value);
    }
    (!out.is_empty()).then(|| serde_json::Value::Object(out))
}

fn with_durable(source: &str, data: &serde_json::Value) -> serde_json::Value {
    let plan = splash_ui_l0::source_plan(source);
    let mut out = data.clone();
    for request in &plan.requests {
        let Some(collection) = request.helper.strip_prefix("sys.") else {
            continue;
        };
        // Any capability the profile says is writable is backed by the store,
        // so this follows the catalog rather than naming collections here — a
        // list hardcoded in the host is one that forgets the next capability.
        if splash_ui_l0::catalog::mutable(&request.helper).is_none() {
            continue;
        }
        // Only the REFERENCE goes in, under the field that identifies a row.
        // The realizer needs one field per row to give the `for` its keys; every
        // other field the card asked for lowers to a live call.
        let key_field = match collection {
            "cities" => "name",
            _ => "ticker",
        };
        let rows: Vec<serde_json::Value> = super::user_store::collection(collection)
            .into_iter()
            .map(|reference| serde_json::json!({ key_field: reference }))
            .collect();
        if !out.is_object() {
            out = serde_json::Value::Object(Default::default());
        }
        if let Some(map) = out.as_object_mut() {
            map.insert(request.name.clone(), serde_json::Value::Array(rows));
        }
    }
    out
}

/// Hand every stored collection to the VM, so a durable source can resolve a row.
///
/// The app owns the file and the widgets crate owns the fetch, so a list of
/// tickers is the only thing that crosses between them — the same shape
/// `sys.gps` uses for the platform's location fix. Called at startup and after
/// every write; anything that changes the store and forgets this leaves the
/// screen showing the list as it was.
pub fn publish_collections() {
    makepad_widgets::splash::set_collections(super::user_store::all_collections());
    // §5.12's single values travel the same way. Read-only from a card's side:
    // `sys.prefs` answers them and no transition writes them, which is what keeps
    // a preference the user's rather than a cell a card can move.
    makepad_widgets::splash::set_prefs(super::user_store::get().prefs);
    publish_locale();
}

/// What the device is set to, as `sys.locale` answers it.
///
/// Read from the environment rather than taken from a plan: the app's other
/// `locale` is whatever the model wrote in a plan JSON, which is a property of one
/// request and not of the device, and an L0 card is not a plan. `LANG`/`LC_ALL` is
/// what this platform actually exposes here.
///
/// **The limit, stated rather than hidden.** Android does not always set either,
/// and this does not reach the platform's own locale API — so a phone with neither
/// set reads as `en`, which is the default `sys.locale` already falls back to. That
/// is a wrong answer for some users and it is a *stated* one; what it replaces was
/// an em dash for every user, on six of the seven exemplars.
///
/// Fahrenheit is a REGION property, not a language one — `en_GB` is Celsius — so it
/// is derived from the region, and only for the three countries that use it.
fn publish_locale() {
    let raw = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    let tag = raw.split('.').next().unwrap_or("").replace('-', "_");
    let mut parts = tag.split('_');
    let lang = match parts.next().unwrap_or("") {
        "" | "C" | "POSIX" => "en",
        l => l,
    };
    let region = parts.next().unwrap_or("");
    let unit = if matches!(region, "US" | "LR" | "MM") {
        "f"
    } else {
        "c"
    };
    makepad_widgets::splash::set_locale(lang, unit);
}

/// Record the card that was just seeded, so its taps have somewhere to land.
pub fn begin(source: String, data: serde_json::Value, item: usize) {
    if let Ok(mut map) = SESSIONS.write() {
        map.insert(
            item,
            L0Session {
                source,
                data,
                store: splash_ui_l0::InstanceStore::default(),
                item,
            },
        );
    }
}

/// A tap arrived. Apply it and produce the card's new body.
///
/// `Ok(None)` means the event applied to nothing — an unknown event, or a
/// transition whose payload did not fit its declared shape. That is not an
/// error to report on screen, but it must be distinguishable from a successful
/// no-op, because "the tap did nothing" and "the tap was refused" look
/// identical to a user and completely different to whoever is debugging.
pub fn tap(
    cx: &mut makepad_widgets::Cx,
    item: usize,
    key: &str,
    event: &str,
    value: &str,
) -> Result<Option<(usize, String)>, String> {
    let mut map = SESSIONS.write().map_err(|_| "session lock poisoned".to_owned())?;
    // The card the tap came FROM, not whichever was rendered last.
    let Some(session) = map.get_mut(&item) else {
        return Ok(None);
    };

    let payload = (!value.is_empty()).then(|| serde_json::Value::String(value.to_owned()));
    // `dispatch_reporting`, not the bool form: a §5.12 transition writes nothing
    // in the card and instead REPORTS a write the host owes its store. The bool
    // form cannot express that, so a tap on a watchlist row would have applied
    // and done nothing.
    let outcome = splash_ui_l0::dispatch_reporting(
        &session.source,
        &mut session.store,
        key,
        event,
        payload.as_ref(),
        &session.data,
    );
    for write in &outcome.writes {
        // Keyed by CAPABILITY, not by the card's binding name. A card may call
        // it `watch` and another `saved`, and they must reach the same list —
        // sharing between cards is the reason the store exists outside them.
        //
        // A PREFERENCE is keyed one level deeper: the capability names the store
        // and the write's `field` — the source's single declared field, which
        // the checker guarantees — names the entry. `sys.prefs(fields: [home])`
        // written with set("Saratoga High School") lands under `home`.
        let collection = if write.helper == "sys.prefs" {
            write.field.as_str()
        } else {
            write.helper.strip_prefix("sys.").unwrap_or(&write.helper)
        };
        super::user_store::apply(collection, &write.op, &write.value);
    }
    if !outcome.writes.is_empty() {
        publish_collections();
    }
    // WHICH cells moved. "Applied" and "applied the thing you meant" are different
    // claims, and only the second one is what a screen shows: a `cycle` that
    // advanced from the wrong current value applies, reports success, and lands
    // back where it started.
    makepad_widgets::log!(
        "[l0] {event} applied={} changed={:?} stale={:?}",
        outcome.applied,
        outcome.changed,
        outcome.stale
    );
    if !outcome.applied {
        return Ok(None);
    }

    // Realize once, then drop the cells whose instances are gone. §5.7 says a
    // cell dies with its instance; without this the store grows every time a
    // branch swaps, and a row that comes back later finds its OLD expansion
    // state waiting — which looks like a rendering bug and is not.
    // The store may have just grown by this very tap, so the durable rows are
    // merged AFTER the write rather than from the session's original blob.
    let tapped_data = with_durable(&session.source, &session.data);
    let report = splash_ui_l0::realize_with_state(
        &session.source,
        &tapped_data,
        &session.store,
        splash_ui_l0::RealizeLimits::default(),
    );
    let Some(root) = report.root else {
        return Err(report
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; "));
    };
    let body = {
        let src = format!("{KIT}\n{}", splash_ui_l0::kit::lower(&root));
        let tree = super::l0_eval::build_with_capabilities(cx, &src)
            .ok_or_else(|| "the lowered card evaluated to nil".to_owned())?;
        super::l0_widgets::to_dsl(&tree)
    };
    session.store.prune(&report.live_keys);
    Ok(Some((session.item, body)))
}

/// §1.1's branch point, adopted here.
///
/// `splash-node` carries the `UiNode` model the profile names as the point where
/// one card reaches three backends. Taking `splash-render` instead fails on
/// `makepad-error-log v1.0.0` existing at two paths — the same collision that
/// forced `splash-ui-l0` out of `splash-core` — so the model had to be split
/// from its evaluator before this app could hold it at all.
///
/// Nothing renders through it yet. What it establishes is that it CAN: the
/// evaluator and the widget mapping are the remaining work, and neither is
/// blocked on a lockfile any more.
pub use splash_node::{Attrs, NodeKind, UiNode};

#[cfg(test)]
mod exemplar_drift {
    //! The exemplar the MODEL is shown must be the card the TESTS check.
    //!
    //! `nav.card` lives in `splash-ui-l0`'s fixtures, where every profile test
    //! exercises it, and `exemplar.card` is compiled into this app as what the model
    //! is given to write from. They are the same card in two repositories, and they
    //! silently diverged: a day of work — a route badge, search-as-you-type, a 2D/3D
    //! toggle, map controls, a reworked sheet — went into the fixture while the app
    //! kept generating from a copy that predated all of it. Every one of those
    //! features passed its tests and none of them reached the phone.
    //!
    //! This is the same failure as `catalog.md` drifting from the normative TOML, and
    //! it is invisible from either side: the fixture's tests pass, the exemplar still
    //! checks as valid L0, and only a human tapping a button that does not exist
    //! finds out.
    const EXEMPLAR: &str = include_str!("../../../../a2app-l0/apps/nav/exemplar.card");
    const FIXTURE: &str =
        include_str!("../../../../../Splash/crates/splash-ui-l0/tests/fixtures/nav.card");

    #[test]
    fn the_nav_exemplar_is_the_card_the_profile_tests_check() {
        assert_eq!(
            EXEMPLAR.trim(),
            FIXTURE.trim(),
            "the exemplar the model writes from has drifted from the fixture the \
             tests check — copy Splash's tests/fixtures/nav.card over \
             a2app-l0/apps/nav/exemplar.card"
        );
    }
}

#[cfg(test)]
mod tests {
    /// The branch point is linked, not merely resolved.
    ///
    /// A path dependency that resolves can still fail to compile or link, and
    /// the whole question here was whether this binary can hold the model at
    /// all. Naming a variant proves the type crossed the boundary.
    #[test]
    fn the_branch_point_is_usable_from_this_binary() {
        assert_eq!(
            super::NodeKind::from_tag("column"),
            Some(super::NodeKind::Column)
        );
        // A tag outside the table is None rather than a default — an unknown
        // kind must be reported, never silently rendered as something else.
        assert_eq!(super::NodeKind::from_tag("hologram"), None);
    }
}

/// Turn every ```` ```runl0 ```` block in a message into a rendered card.
///
/// The model emits an L0 LEDGER; everything downstream — the markdown widget,
/// `tag_notify_calls`, the render cache — speaks the backend's widget DSL. So
/// the ledger is realized, lowered through the theme kit and evaluated here, and
/// what continues down the pipe is a ```` ```runsplash ```` block. Nothing after
/// this point needs to know L0 exists.
///
/// A card that does not check or does not realize is replaced by its
/// DIAGNOSTICS, as a card. That is the whole reason to do this at render time
/// rather than at generation time: a rejected card should say why on screen,
/// where the person who asked for it can see it, and the reasons are what a
/// second attempt would need.
pub fn resolve_l0_blocks(cx: &mut makepad_widgets::Cx, text: &str, item: usize) -> String {
    let mut out = String::with_capacity(text.len());
    for piece in split_l0_blocks(text) {
        match piece {
            Piece::Prose(t) => out.push_str(t),
            Piece::Ledger(src) => {
                out.push_str("```runsplash\n");
                out.push_str(&render_ledger(cx, src, item));
                out.push_str("\n```");
            }
        }
    }
    out
}

/// A message, split into prose and ledgers.
#[derive(Debug, PartialEq)]
pub enum Piece<'a> {
    Prose(&'a str),
    Ledger(&'a str),
}

/// Find the ledgers, without rendering any.
///
/// Split out because rendering needs the app's `Cx` — the capabilities at the
/// end of the pipeline downcast the VM host to one — and a unit test has no
/// `Cx` to give. This half is this module's own logic and is testable; the other
/// half is covered by `splash-ui-l0`'s lowering tests and by the device harness.
pub fn split_l0_blocks(text: &str) -> Vec<Piece<'_>> {
    const FENCE: &str = "```runl0";
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find(FENCE) {
        out.push(Piece::Prose(&rest[..open]));
        let after = &rest[open + FENCE.len()..];
        let body_start = after.find('\n').map(|n| n + 1).unwrap_or(after.len());
        let body_and_rest = &after[body_start..];
        match body_and_rest.find("```") {
            Some(close) => {
                out.push(Piece::Ledger(&body_and_rest[..close]));
                rest = &body_and_rest[close + 3..];
            }
            // An unclosed block is still streaming. Leave it whole rather than
            // rendering half a ledger, which looks like a card the model got
            // wrong rather than one that has not finished arriving.
            None => {
                out.push(Piece::Prose(&rest[open..]));
                return out;
            }
        }
    }
    out.push(Piece::Prose(rest));
    out
}

/// What each source's fetch is actually doing, keyed by the ledger it belongs to.
///
/// Observed, not asserted. See `live_source_status`.
static SOURCE_STATES: RwLock<BTreeMap<u64, BTreeMap<String, String>>> =
    RwLock::new(BTreeMap::new());

fn ledger_key(ledger: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ledger.hash(&mut h);
    h.finish()
}

/// The `$status` block a card realizes against — §5.9's lifecycle, as far as this
/// host can observe it.
///
/// **This reported `ready` for every declared source, always.** It scanned the
/// ledger for `source ` lines and asserted the token, before any fetch had been
/// issued and regardless of how one turned out. §5.9 exists precisely so that
/// "still loading", "the network is down" and "this field genuinely has no value"
/// do not all render the same em dash — and under a blanket `ready` they did again,
/// with the card additionally asserting a lifecycle nothing had looked at. That is
/// §4's no-facts rule broken by the host, on the one value §5.9 says the card must
/// never compute.
///
/// It cost a real diagnosis: launched without its proxy, the nav card drew an empty
/// instruction banner and a bare " min" — the shape of a lowering bug — when what
/// had happened was that every fetch failed and no source could say so.
///
/// **What is observed.** The capabilities already answer `"—"` while a fetch is in
/// flight and `"n/a"` once its retry budget is spent (`script_data_placeholder`),
/// which is exactly the distinction §5.9 needs and was being thrown away. After each
/// realize, every source's own call is evaluated once and classified from its answer.
///
/// **Three limits, stated rather than hidden.**
///
/// - It LAGS BY ONE RENDER. `$state` is read at realize time, and the calls whose
///   state it describes are only known after realize resolves their arguments — so
///   the blob carries what the previous pass saw. Everything starts `.pending`,
///   which is true, and a landing fetch bumps the data-fetch epoch and redraws.
/// - Two sources naming the SAME capability share a state, taking the worse of the
///   two. The realized bindings carry a helper and resolved arguments but not the
///   card's name for them, so `sys.weather` for two cities cannot be told apart
///   here. Conservative in the safe direction: a card says "loading" while either
///   is loading, and never says "ready" while something is not.
/// - `.stale` is never reported. The fetch layer has no freshness budget to compare
///   against, and §5.9 is explicit that staleness is not derivable from data.
fn live_source_status(ledger: &str) -> serde_json::Value {
    let observed = SOURCE_STATES
        .read()
        .ok()
        .and_then(|m| m.get(&ledger_key(ledger)).cloned())
        .unwrap_or_default();
    let mut status = serde_json::Map::new();
    for line in ledger.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("source ") else {
            continue;
        };
        let Some(name) = rest.split_whitespace().next() else {
            continue;
        };
        // PENDING until something is seen. The realizer's own fallback would read an
        // absent key as pending too, but only when the blob is the whole truth — and
        // here it is not, because the values arrive as live calls the blob never
        // holds. Saying it explicitly is what stops the two rules disagreeing.
        let state = observed
            .get(name)
            .map(String::as_str)
            .unwrap_or("pending");
        status.insert(name.to_string(), serde_json::Value::String(state.into()));
    }
    if status.is_empty() {
        return serde_json::Value::Object(Default::default());
    }
    let mut blob = serde_json::Map::new();
    blob.insert("$status".to_string(), serde_json::Value::Object(status));
    serde_json::Value::Object(blob)
}

/// A call that exercises this binding's fetch, whatever field it happens to name.
fn probe_call(binding: &splash_ui_l0::SourceBinding) -> Option<String> {
    if !binding.field.is_empty() {
        return splash_ui_l0::makepad::vm_call(binding);
    }
    for field in splash_ui_l0::catalog::answers(&binding.helper)? {
        let probe = splash_ui_l0::SourceBinding {
            field: (*field).to_owned(),
            ..binding.clone()
        };
        if let Some(call) = splash_ui_l0::makepad::vm_call(&probe) {
            return Some(call);
        }
    }
    None
}

/// Which lifecycle an answer reports. See `script_data_placeholder`.
fn state_of_answer(text: &str) -> &'static str {
    match text.trim() {
        "\u{2014}" | "" => "pending",
        "n/a" => "failed",
        _ => "ready",
    }
}

/// Evaluate every source's own call once and record what its fetch is doing.
///
/// Runs after realize, because that is when a source's arguments are resolved — a
/// `sys.weather` for a city the user typed has no URL until the search that found it
/// has answered.
fn observe_source_states(
    cx: &mut makepad_widgets::Cx,
    ledger: &str,
    root: &splash_ui_l0::UiNode,
) {
    // Worst state per HELPER, then attributed to the sources that name it.
    let mut by_helper: BTreeMap<String, &'static str> = BTreeMap::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        for (_, binding) in &node.bindings {
            if by_helper.get(&binding.helper) == Some(&"pending") {
                continue;
            }
            // A binding that names the SOURCE rather than a field of it — `Map(from:
            // origin_place)` — has an empty `field`, and no call answers "the source
            // itself". Every binding on a planning screen is one of those, which is
            // why the first version of this observed nothing at all: three bindings
            // found, zero calls built, every source reported `.pending` forever.
            //
            // Any field the catalog says the helper answers will do, because the
            // lifecycle is a property of the FETCH and every field of one source
            // rides the same request.
            let Some(call) = probe_call(binding) else {
                continue;
            };
            let Some(text) = eval_text(cx, &call) else {
                continue;
            };
            let seen = state_of_answer(&text);
            let rank = |s: &str| match s {
                "pending" => 0,
                "failed" => 1,
                _ => 2,
            };
            match by_helper.get(&binding.helper) {
                Some(prev) if rank(prev) <= rank(seen) => {}
                _ => {
                    by_helper.insert(binding.helper.clone(), seen);
                }
            }
        }
        stack.extend(node.children.iter());
    }

    let mut states: BTreeMap<String, String> = BTreeMap::new();
    for request in splash_ui_l0::source_plan(ledger).requests {
        if let Some(state) = by_helper.get(&request.helper) {
            states.insert(request.name, (*state).to_owned());
        }
    }
    makepad_widgets::log!(
        "L0 $state: {}",
        states
            .iter()
            .map(|(n, s)| format!("{n}={s}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    if let Ok(mut map) = SOURCE_STATES.write() {
        map.insert(ledger_key(ledger), states);
    }
}

thread_local! {
    /// What the last realization CAPTURED from a source, handed out to whoever owns
    /// the store. A thread local because the render path is several calls deep and
    /// only the outermost one can write.
    static LAST_CAPTURED: std::cell::RefCell<Option<Vec<(String, serde_json::Value)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Render, and FREEZE anything the card captured from a source.
///
/// A state declared `initial: here.lat` reads its source once — but only once the
/// value is written down. Left in the data it re-resolves on every realization, so
/// the state FOLLOWS the source: an origin declared as "where I am" chases the
/// device, and a route from it is re-fetched before it can answer. Writing the cell
/// is what turns an initial value into a captured one.
///
/// The realizer decides WHAT was captured, because it owns the precedence. The host
/// decides when it becomes durable, because it owns the store. Written once: a cell
/// that exists is already captured, and a tap that changes it wins from then on.
fn render_capturing(
    cx: &mut makepad_widgets::Cx,
    source: &str,
    data: &serde_json::Value,
    store: &splash_ui_l0::InstanceStore,
    item: usize,
) -> Result<String, String> {
    let dsl = render_through_kit(cx, source, data, store)?;
    let captured = LAST_CAPTURED
        .with(|c| c.borrow_mut().take())
        .unwrap_or_default();
    if captured.is_empty() {
        return Ok(dsl);
    }
    if let Ok(mut map) = SESSIONS.write() {
        if let Some(session) = map.get_mut(&item) {
            for (path, value) in captured {
                if session
                    .store
                    .get(splash_ui_l0::CARD_STATE_KEY, &path)
                    .is_none()
                {
                    session
                        .store
                        .set_cell(splash_ui_l0::CARD_STATE_KEY, &path, value);
                }
            }
        }
    }
    Ok(dsl)
}

fn render_ledger(cx: &mut makepad_widgets::Cx, source: &str, item: usize) -> String {
    // Check first. `realize` is tolerant where the profile is not, so a card
    // that realizes is not necessarily a card that was admissible, and shipping
    // an inadmissible one would make the checker decorative.
    let report = splash_ui_l0::check_ui_l0_named("card", source);
    if !report.valid {
        // A REFUSAL IS NOT A SCREEN.
        //
        // This drew "This card was refused" with the diagnostics under it, which
        // shows a person the compiler's opinion of a card they did not write and
        // cannot fix. The reasons are for whoever is building the generator, and
        // they go to the log; the repair path already re-prompts the model with
        // them, so the user's outcome is a corrected card or nothing.
        //
        // Nothing is the right fallback. A blank card is a card still arriving,
        // which is what it is — and it is the presentation §5.9 already gives to
        // data in flight, rather than a new failure vocabulary aimed at the wrong
        // audience.
        makepad_widgets::log!(
            "L0 card refused, not rendered ({} diagnostic(s)): {}",
            report.diagnostics.len(),
            report
                .diagnostics
                .iter()
                .map(|d| format!("line {}: {}", d.line, d.message))
                .collect::<Vec<_>>()
                .join(" | ")
        );
        return quiet_card();
    }
    // What the model actually declared. Only the `state` lines: enough to tell a
    // card that seeded the request's places from one that opened on an empty
    // search box, without putting a whole generated card in the log.
    //
    // Refusals were logged and acceptances were not, so a card that passed the
    // checker and then rendered nothing but em dashes gave nothing to read.
    {
        let states: Vec<&str> = source
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("state ") || l.starts_with("source "))
            .collect();
        // Which ROLES it used, too. A card can be accepted, seed its places
        // correctly, and simply not contain the role the spec calls mandatory —
        // and a screen with no map looks identical to a map that was guarded out.
        let mut roles: Vec<&str> = Vec::new();
        for r in ["Map", "Field", "TextHero", "TempBar", "StockPlot", "WeatherIcon"] {
            if source.contains(&format!("{r}(")) {
                roles.push(r);
            }
        }
        // The §7 closure digest, logged rather than stored.
        //
        // §7 asks a host to keep the digest beside the approved level so a card
        // cannot inherit a level that was derived from a component definition since
        // replaced. That hazard needs two things this host does not have: a level
        // it CACHES, and components defined outside the card. Neither exists —
        // `check_ui_l0_named` runs on every render and on every acceptance, and the
        // checker refuses any constructor the card does not declare itself
        // ("`X` is not an L0 constructor or a declared component"), so a card's
        // closure is always its own text. `a_card_cannot_reference_a_component_it_
        // does_not_declare` holds the second half, which is what makes the first
        // half safe rather than assumed: the day a shared component library lands,
        // that test fails and this has to become a stored pin.
        //
        // Logged because a digest nobody can see is also a digest nobody can use to
        // tell two cards apart in a bug report.
        let digest: Vec<String> = report
            .closure
            .iter()
            .map(|(n, h)| format!("{n}:{h:x}"))
            .collect();
        makepad_widgets::log!(
            "L0 card accepted at {:?}; declarations: {} ;; roles: {} ;; closure: [{}]",
            report.level,
            states.join(" | "),
            roles.join(","),
            digest.join(" ")
        );
    }

    // A resolve is not a RESET.
    //
    // This made a fresh empty blob and a fresh store on every draw and then
    // called `begin`, which replaces the session wholesale. Two things died with
    // it each time: the data a seeded card was handed, and every cell a tap had
    // written — so a card that had been tapped returned to its initial state on
    // the next redraw, and the redraw could come from anything.
    //
    // Sources this backend can answer become live calls and need no blob at all;
    // the blob is what a fixture supplies and what an unanswerable source falls
    // back to. Either way it belongs to the session, not to the draw.
    // Compare TRIMMED. The session holds the source as it was handed in; this
    // one came back out of a ```runl0 fence, which adds a newline at each end.
    // Comparing them raw never matched, so every draw looked like a new card:
    // the seeded data was replaced with an empty blob and the whole weather
    // screen — backdrop, forecast, every reading — became em dashes.
    let existing = SESSIONS.read().ok().and_then(|map| {
        map.get(&item)
            .filter(|s| s.source.trim() == source.trim())
            .map(|s| (s.data.clone(), s.store.clone()))
    });
    let is_new = existing.is_none();
    let (mut data, store) = existing.unwrap_or_else(|| {
        (
            live_source_status(source),
            splash_ui_l0::InstanceStore::default(),
        )
    });
    // REFRESHED, not carried. A session's data blob is created once and reused, so
    // whatever `$status` it was born with is what every later render would read —
    // and since a first render has observed nothing, every source would be
    // `.pending` for the life of the card and no `when x.$state == .ready` branch
    // would ever appear. The blob is the right home for the lifecycle (a guard is
    // evaluated at realize time) and the wrong home for a value that changes, so
    // only this key is replaced and everything else the session holds survives.
    if let (Some(map), Some(fresh)) = (
        data.as_object_mut(),
        live_source_status(source).get("$status").cloned(),
    ) {
        map.insert("$status".to_owned(), fresh);
    }
    match render_capturing(cx, source, &data, &store, item) {
        Ok(dsl) => {
            if is_new {
                begin(source.to_owned(), data.clone(), item);
            }
            // And the session keeps the refreshed one, so a tap re-realizes against
            // the lifecycle the screen was showing rather than the one it was born
            // with.
            if let Ok(mut map) = SESSIONS.write() {
                if let Some(session) = map.get_mut(&item) {
                    session.data = data;
                }
            }
            dsl
        }
        // Same rule: a realization failure is not a screen either.
        Err(why) => {
            makepad_widgets::log!("L0 card did not realize, not rendered: {why}");
            quiet_card()
        }
    }
}

/// The reasons, as a card. A blank screen reads as a layout bug; the reasons
/// read as a rejected card, and they are what a retry would need.
/// An empty surface: what a refused or still-arriving card looks like.
///
/// Deliberately indistinguishable from a card whose sources have not landed. A
/// person cannot act on a checker diagnostic about generated source, so the only
/// honest states to show them are "here it is" and "not yet".
pub(crate) fn quiet_card() -> String {
    "SolidView{ width: Fill height: Fit flow: Down new_batch: true \
     draw_bg.color: #0a0e14 padding: Inset{left: 20 top: 54 right: 20 bottom: 24} }"
        .to_string()
}

pub(crate) fn diagnostics_card(headline: &str, reasons: &[String]) -> String {
    let mut out = String::from(
        "SolidView{ width: Fill height: Fit flow: Down new_batch: true \
         draw_bg.color: #0a0e14 padding: Inset{left: 20 top: 54 right: 20 bottom: 24}\n",
    );
    out.push_str(&format!(
        "  TextTitle{{ text: {headline:?} draw_text.color: #ffffff }}\n"
    ));
    for r in reasons.iter().take(8) {
        let line = r.replace('\\', " ").replace('"', "'");
        out.push_str(&format!(
            "  TextBody{{ width: Fill text: {line:?} draw_text.color: #ffffff99 }}\n"
        ));
    }
    out.push('}');
    out
}

#[cfg(test)]
mod resolve_tests {
    use super::{split_l0_blocks, Piece};

    /// The fence is found, and the ledger comes out separated from the prose.
    ///
    /// WHAT THIS CANNOT CHECK. Rendering needs the app's `Cx`, because the
    /// capabilities at the end of the pipeline downcast the VM host to one — a
    /// bare host panics, which is exactly how the stock card took the app down.
    /// So the scan is tested here, the lowering in `splash-ui-l0`, and the
    /// rendering on a phone.
    #[test]
    fn a_ledger_is_separated_from_its_prose() {
        let msg = "here you go\n\n```runl0\nview root Rule()\n```\ntrailing";
        assert_eq!(
            split_l0_blocks(msg),
            vec![
                Piece::Prose("here you go\n\n"),
                Piece::Ledger("view root Rule()\n"),
                Piece::Prose("\ntrailing"),
            ]
        );
    }

    /// An unclosed block is still streaming and yields NO ledger.
    ///
    /// Asserted as "nothing was recognised", not as an exact piece list: the
    /// scan may emit an empty prose run before a fence and that is immaterial.
    /// A test pinned to the exact shape fails on a change that means nothing.
    #[test]
    fn an_unclosed_block_yields_no_ledger() {
        let partial = "```runl0\nsource now sys.weather(lat: 1.0";
        let pieces = split_l0_blocks(partial);
        assert!(
            !pieces.iter().any(|p| matches!(p, Piece::Ledger(_))),
            "a half-arrived ledger must not be rendered: {pieces:?}"
        );
        assert_eq!(rejoin(&pieces), partial, "and nothing may be lost");
    }

    /// A message with no ledger survives unchanged.
    #[test]
    fn a_message_without_a_ledger_is_unchanged() {
        let msg = "just prose, and a ```runsplash\nView{}\n``` card";
        let pieces = split_l0_blocks(msg);
        assert!(!pieces.iter().any(|p| matches!(p, Piece::Ledger(_))));
        assert_eq!(rejoin(&pieces), msg);
    }

    /// Prose reassembles to exactly the input — nothing dropped, nothing added.
    fn rejoin(pieces: &[Piece<'_>]) -> String {
        pieces
            .iter()
            .map(|p| match p {
                Piece::Prose(t) => *t,
                Piece::Ledger(t) => *t,
            })
            .collect()
    }

    /// Two ledgers in one message are both found.
    #[test]
    fn two_ledgers_are_both_found() {
        let msg = "```runl0\na\n```mid```runl0\nb\n```";
        let n = split_l0_blocks(msg)
            .iter()
            .filter(|p| matches!(p, Piece::Ledger(_)))
            .count();
        assert_eq!(n, 2, "both ledgers must be found");
    }
}


/// Every call the lowering can emit is a helper this app actually registers.
///
/// The two halves of `sys.*` live in different repositories: Splash decides which
/// call answers a card's declared source, and `aichat/widgets/src/splash.rs`
/// implements it. Nothing checked that the names met. Four whole capabilities were
/// documented in the catalog, accepted by the checker, and answered by no helper —
/// `sys.locale` among them, which six of the seven exemplars reach for — and each
/// one rendered an em dash on the phone with no diagnostic anywhere.
///
/// Both sides are read out of their own SOURCE, so this cannot become a third place
/// to keep the names in sync, and it covers the whole lowering rather than whatever
/// the shipped exemplars happen to exercise.
///
/// This is the SECOND of the two directions, and it is worth being exact about
/// which. Splash's `every_offered_field_has_a_translation` holds "the catalog
/// offers nothing the lowering cannot emit" — that is the one the four missing
/// capabilities failed, and they failed it into an allowlist. This one holds "the
/// lowering emits nothing this app cannot answer", which is the direction that
/// breaks when a helper is renamed or a new arm is written against a call that was
/// never implemented. Neither implies the other and the gap needed both.
#[cfg(test)]
mod capability_bridge {
    const BACKEND: &str = include_str!("../../../../aichat/widgets/src/splash.rs");
    const LOWERING: &str =
        include_str!("../../../../../Splash/crates/splash-ui-l0/src/lib.rs");

    fn between(hay: &str, open: &str, close: char) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        for (i, _) in hay.match_indices(open) {
            let rest = &hay[i + open.len()..];
            if let Some(end) = rest.find(close) {
                let name = rest[..end].trim();
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    out.insert(name.to_owned());
                }
            }
        }
        out
    }

    #[test]
    fn every_call_the_lowering_emits_has_a_helper() {
        let registered = between(BACKEND, "id_lut!(", ')');
        assert!(
            registered.len() > 20,
            "the registration list did not parse: {registered:?}"
        );
        // Every `"sys.<name>(` in the lowering is a call it can emit. Reading the
        // literal is the point: this is the string that reaches the VM.
        let mut emitted = between(LOWERING, "\"sys.", '(');
        // `sys.num` wraps another call for numeric coercion and is emitted the same
        // way, so it is in this set legitimately.
        emitted.retain(|n| !n.is_empty());
        assert!(
            emitted.len() > 15,
            "the emitted-call set did not parse; this test would pass vacuously: {emitted:?}"
        );
        let missing: Vec<&String> = emitted.difference(&registered).collect();
        assert!(
            missing.is_empty(),
            "the lowering emits calls this app does not register: {missing:#?}\n\
             registered: {registered:?}"
        );
    }
}


/// §7's pinning hazard cannot arise here, and this is what keeps that true.
///
/// "Component versions must be pinned, or an L0 card can be moved to L2 by a
/// definition it references being replaced." That needs components defined outside
/// the card. The checker refuses any constructor a card does not declare itself, so
/// every closure is the card's own text and there is no external definition to move
/// — which is why this host logs the digest instead of storing it.
///
/// Written as a test rather than as a comment because it is the load-bearing half.
/// A shared component library would make the digest mandatory, and the failure mode
/// is silent: cards keep rendering, at a level derived from a definition that has
/// changed underneath them.
#[cfg(test)]
mod component_closure {
    #[test]
    fn a_card_cannot_reference_a_component_it_does_not_declare() {
        let report = splash_ui_l0::check_ui_l0_named(
            "probe",
            "source w sys.weather(lat: 1, lon: 2, fields: [temp])\n\
             view root Surface { NotDeclared(x: w.temp) }\n",
        );
        assert!(
            !report.valid,
            "an undeclared component must be refused, or the closure digest has to \
             be stored and compared per §7"
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("declared component")),
            "and refused for THAT reason: {:#?}",
            report.diagnostics
        );
    }

    /// Every shipped exemplar's closure is its own declarations, and nothing else.
    #[test]
    fn every_exemplar_closes_over_only_itself() {
        for (domain, _, exemplar) in crate::L0_APPS {
            let report = splash_ui_l0::check_ui_l0_named(domain, exemplar);
            for (name, _) in &report.closure {
                assert!(
                    exemplar.contains(&format!("component {name}")),
                    "{domain}'s closure names `{name}`, which it does not declare"
                );
            }
        }
    }
}


/// §5.9's lifecycle is observed, not asserted.
#[cfg(test)]
mod source_lifecycle {
    use super::{ledger_key, live_source_status, state_of_answer, SOURCE_STATES};

    // One ledger per test: `SOURCE_STATES` is keyed by the ledger's text and the
    // tests run in parallel, so a shared key makes them race each other.
    fn ledger(tag: &str) -> String {
        format!(
            "# {tag}\n\
             source now sys.weather(lat: 1, lon: 2, fields: [temp])\n\
             source air sys.airquality(lat: 1, lon: 2, fields: [aqi])\n\
             view root Surface {{ TextHero(value: now.temp) }}\n"
        )
    }

    fn status_of(ledger: &str, name: &str) -> String {
        live_source_status(ledger)["$status"][name]
            .as_str()
            .unwrap_or("<absent>")
            .to_owned()
    }

    /// Nothing observed means PENDING, not ready.
    ///
    /// This is the whole of the bug: the host reported `ready` for every declared
    /// source by scanning the ledger's text, so a card could never say "loading" and
    /// — the half that mattered — could never say "the network is down". Launched
    /// without its proxy, nav drew an empty banner and a bare " min", which reads as
    /// a lowering fault and was every fetch failing silently.
    #[test]
    fn an_unobserved_source_is_pending() {
        let l = ledger("unobserved");
        assert_eq!(status_of(&l, "now"), "pending");
        assert_eq!(status_of(&l, "air"), "pending");
    }

    /// And what was observed is what the card reads.
    #[test]
    fn an_observed_source_reports_what_it_did() {
        let l = ledger("observed");
        if let Ok(mut m) = SOURCE_STATES.write() {
            let mut states = std::collections::BTreeMap::new();
            states.insert("now".to_owned(), "ready".to_owned());
            states.insert("air".to_owned(), "failed".to_owned());
            m.insert(ledger_key(&l), states);
        }
        assert_eq!(status_of(&l, "now"), "ready");
        // The half that was impossible to report before.
        assert_eq!(status_of(&l, "air"), "failed");
    }

    /// The three answers a capability can give, as the three tokens §5.9 defines.
    ///
    /// `script_data_placeholder` already drew this distinction — "—" while a fetch
    /// is in flight or retrying, "n/a" once the retry budget is spent — and the
    /// status blob threw it away.
    #[test]
    fn an_answer_classifies_the_way_the_placeholder_does() {
        assert_eq!(state_of_answer("\u{2014}"), "pending");
        assert_eq!(state_of_answer(""), "pending");
        assert_eq!(state_of_answer("n/a"), "failed");
        assert_eq!(state_of_answer("18.4"), "ready");
        // A value that merely CONTAINS the placeholder is a value.
        assert_eq!(state_of_answer("n/august"), "ready");
    }
}
