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
const KIT: &str = include_str!("../../../../../Splash-Makepad/components/l0/_kit.splash");

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
pub fn resolve_l0_blocks(text: &str, item: usize) -> String {
    const FENCE: &str = "```runl0";
    if !text.contains(FENCE) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find(FENCE) {
        out.push_str(&rest[..open]);
        let after = &rest[open + FENCE.len()..];
        let body_start = after.find('\n').map(|n| n + 1).unwrap_or(after.len());
        let body_and_rest = &after[body_start..];
        let (body, tail) = match body_and_rest.find("```") {
            Some(close) => (&body_and_rest[..close], &body_and_rest[close + 3..]),
            // An unclosed block is still streaming. Leave it alone rather than
            // rendering half a ledger as a card.
            None => {
                out.push_str(&rest[open..]);
                return out;
            }
        };
        out.push_str("```runsplash\n");
        out.push_str(&render_ledger(body, item));
        out.push_str("\n```");
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// One ledger, as widget DSL — or as a card saying why it could not be.
fn render_ledger(source: &str, item: usize) -> String {
    // Check first. `realize` is tolerant where the profile is not, so a card
    // that realizes is not necessarily a card that was admissible, and shipping
    // an inadmissible one would make the checker decorative.
    let report = splash_ui_l0::check_ui_l0_named("card", source);
    if !report.valid {
        let why: Vec<String> = report
            .diagnostics
            .iter()
            .map(|d| format!("line {}: {}", d.line, d.message))
            .collect();
        return diagnostics_card("This card was refused", &why);
    }

    // No data blob. Sources the backend can answer become live calls; the rest
    // resolve to nothing and render an em dash, which is honest — the card asked
    // for something this backend cannot supply.
    let data = serde_json::Value::Object(Default::default());
    let store = splash_ui_l0::InstanceStore::default();
    match render(source, &data, &store) {
        Ok(dsl) => {
            begin(source.to_owned(), data, item);
            dsl
        }
        Err(why) => diagnostics_card("This card did not realize", &[why]),
    }
}

/// The reasons, as a card. A blank screen reads as a layout bug; the reasons
/// read as a rejected card, and they are what a retry would need.
fn diagnostics_card(headline: &str, reasons: &[String]) -> String {
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
    /// The fence is consumed and the ledger does not survive into the output.
    ///
    /// WHAT THIS CANNOT CHECK. Evaluating the lowered card needs the VM's
    /// capabilities, and those need makepad's `Cx` — which a unit test does not
    /// have, so `sys.movers` panics rather than returning. The full path is
    /// covered where it can run: `splash-ui-l0`'s
    /// `a_live_source_survives_a_loop` asserts the lowering emits per-index
    /// calls, and the device harness renders the result on a phone.
    ///
    /// So this asserts the FENCE handling, which is this module's own logic, and
    /// leaves evaluation to the tests that can actually evaluate.
    #[test]
    fn a_ledger_fence_is_consumed() {
        // A ledger that checks but cannot evaluate here still reports through
        // the diagnostics path, so assert on a deliberately refused one — its
        // route through this function is identical up to the fence rewrite.
        let msg = "here you go\n\n```runl0\nview root Hologram()\n```\ntrailing";
        let out = super::resolve_l0_blocks(msg, 0);
        assert!(!out.contains("```runl0"), "the ledger must be consumed:\n{out}");
        assert!(out.contains("```runsplash"), "and replaced by a card:\n{out}");
        assert!(out.contains("here you go"), "prose before is preserved");
        assert!(out.contains("trailing"), "and prose after");
        assert!(
            !out.contains("view root"),
            "the ledger source must not survive into the card:\n{out}"
        );
    }

    /// A card that does not check shows its REASONS, not a blank screen.
    ///
    /// This is why resolution happens at render time. A rejected card should say
    /// why where the person who asked can see it, and those reasons are exactly
    /// what a second attempt needs.
    #[test]
    fn a_refused_card_renders_its_diagnostics() {
        let bad = "```runl0\nview root Hologram()\n```";
        let out = super::resolve_l0_blocks(bad, 0);
        assert!(out.contains("```runsplash"), "still a card:\n{out}");
        assert!(out.contains("refused"), "it must say it was refused:\n{out}");
        assert!(
            out.contains("Hologram"),
            "and name what it choked on:\n{out}"
        );
    }

    /// An unclosed block is still streaming and is left alone.
    ///
    /// Rendering half a ledger produces a card missing whatever had not arrived,
    /// which looks like a card the model got wrong.
    #[test]
    fn an_unclosed_block_is_left_alone() {
        let partial = "```runl0\nsource now sys.weather(lat: 1.0, lon: 2.0";
        assert_eq!(super::resolve_l0_blocks(partial, 0), partial);
    }

    /// A message with no ledger is untouched, cheaply.
    #[test]
    fn a_message_without_a_ledger_is_unchanged() {
        let msg = "just prose, and a ```runsplash\nView{}\n``` card";
        assert_eq!(super::resolve_l0_blocks(msg, 0), msg);
    }
}
