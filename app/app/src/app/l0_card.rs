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
//! **One session, deliberately.** A chat can hold many cards and the host keys
//! them by chat item id, but the skeleton seeds exactly one. Keying by id is a
//! real concern and not this one: what is unproven here is whether a tap
//! completes the circuit at all, and a map would add a lookup that could fail
//! for reasons having nothing to do with that question.

use std::sync::RwLock;

/// The theme kit, baked in. §1.1's middle layer: the card names roles and this
/// answers them, so no colour or size is decided in Rust.
const KIT: &str = include_str!("../../../../splash-makepad/components/l0/_kit.splash");

/// Realize, lower to role calls, evaluate, and render as widgets.
///
/// This is §1.1 end to end. What it replaces — `makepad::lower` — went straight
/// from a realized tree to this backend's widget dialect with ten hardcoded
/// colours and a font-size ramp, so it decided what a theme decides and reached
/// one backend of three.
fn render_through_kit(
    source: &str,
    data: &serde_json::Value,
    store: &splash_ui_l0::InstanceStore,
) -> Result<String, String> {
    let report = splash_ui_l0::realize_with_state(
        source,
        data,
        store,
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
    let src = format!("{KIT}\n{}", splash_ui_l0::kit::lower(&root));
    // With capabilities: the kit lowers a source this backend can answer into a
    // `sys.*` call, and on a bare VM that call is undefined — the concatenation
    // around it yields `$[Error:WrongValue]`, which then draws as the price.
    let tree = super::l0_eval::build_with_capabilities(&src)
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

static SESSION: RwLock<Option<L0Session>> = RwLock::new(None);

/// Realize and lower a card for display. Returns the Splash DSL, or the
/// diagnostics if the card does not realize.
///
/// Diagnostics are returned rather than logged because a card that fails to
/// realize on device is otherwise a blank screen with the reason in logcat,
/// which is precisely the failure mode the device harness exists to catch.
pub fn render(
    source: &str,
    data: &serde_json::Value,
    store: &splash_ui_l0::InstanceStore,
) -> Result<String, String> {
    render_through_kit(source, data, store)
}

/// Record the card that was just seeded, so its taps have somewhere to land.
pub fn begin(source: String, data: serde_json::Value, item: usize) {
    if let Ok(mut slot) = SESSION.write() {
        *slot = Some(L0Session {
            source,
            data,
            store: splash_ui_l0::InstanceStore::default(),
            item,
        });
    }
}

/// A tap arrived. Apply it and produce the card's new body.
///
/// `Ok(None)` means the event applied to nothing — an unknown event, or a
/// transition whose payload did not fit its declared shape. That is not an
/// error to report on screen, but it must be distinguishable from a successful
/// no-op, because "the tap did nothing" and "the tap was refused" look
/// identical to a user and completely different to whoever is debugging.
pub fn tap(key: &str, event: &str, value: &str) -> Result<Option<(usize, String)>, String> {
    let mut slot = SESSION.write().map_err(|_| "session lock poisoned".to_owned())?;
    let Some(session) = slot.as_mut() else {
        return Ok(None);
    };

    let payload = (!value.is_empty()).then(|| serde_json::Value::String(value.to_owned()));
    let applied = splash_ui_l0::dispatch_with_data(
        &session.source,
        &mut session.store,
        key,
        event,
        payload.as_ref(),
        &session.data,
    );
    if !applied {
        return Ok(None);
    }

    // Realize once, then drop the cells whose instances are gone. §5.7 says a
    // cell dies with its instance; without this the store grows every time a
    // branch swaps, and a row that comes back later finds its OLD expansion
    // state waiting — which looks like a rendering bug and is not.
    let report = splash_ui_l0::realize_with_state(
        &session.source,
        &session.data,
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
        let tree = super::l0_eval::build_with_capabilities(&src)
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
#[allow(unused_imports)]
pub use splash_node::{Attrs, NodeKind, UiNode};

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
