//! `UiNode` → this backend's widget DSL.
//!
//! The last leg of §1.1:
//!
//! ```text
//!   L0 card → kit::lower → _kit.splash → l0_eval → UiNode → HERE → widgets
//! ```
//!
//! Everything above this line is shared with the other backends. This module is
//! the part that is allowed to know about makepad, and it is the only part —
//! which is the property `makepad::lower` violates by deciding presentation
//! (ten hardcoded colours, a font-size ramp) three layers too early.
//!
//! **The theme is upstream now.** A node arrives carrying the colours, sizes and
//! spacing the kit chose. This translates them; it does not choose them. If a
//! colour appears in this file that did not come off a node, that is the defect
//! §1.1 exists to prevent.

use splash_node::{Attrs, NodeKind, UiNode};
use std::fmt::Write as _;

/// Render a tree as the DSL this repository's VM evaluates.
pub fn to_dsl(root: &UiNode) -> String {
    let mut out = String::from("// REALIZED from an L0 ledger — do not edit.\n");
    emit(root, &mut out, 0);
    out
}

/// ARGB integer → makepad hex.
///
/// The two disagree about byte order, and silently: the kit builds
/// `((a*256+r)*256+g)*256+b` while makepad reads `#RRGGBBAA`. Passing the
/// integer through unchanged renders the alpha as red, which on this palette
/// turns a barely-there panel fill into an opaque block — a plausible-looking
/// card with every surface wrong.
/// A numeric parameter, trimmed of a trailing `.0`.
///
/// Missing is `0`, not omitted: these widgets have required uniforms, and one
/// left unset reads as whatever was in the buffer.
fn num(v: Option<f32>) -> String {
    let v = v.unwrap_or(0.0);
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn hex(argb: u32) -> String {
    let (a, r, g, b) = (argb >> 24 & 255, argb >> 16 & 255, argb >> 8 & 255, argb & 255);
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

/// The widget a kind renders as.
///
/// Container kinds differ only in flow, so they share `View` and set it below.
fn widget(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Text => "Label",
        NodeKind::Image => "Image",
        // A card and a chip are both rounded surfaces; what separates them is
        // the radius and padding the kit supplies, not the widget.
        NodeKind::Card | NodeKind::Chip => "RoundedView",
        // A divider is a filled rule: it needs a background, and a bare `View`
        // does not draw one.
        NodeKind::Divider => "SolidView",
        NodeKind::WeatherIcon => "WeatherIcon",
        // The five data visualisations. This backend already ships all six as
        // native widgets — which is why §1.1 says to prove the pipeline here
        // first and let the vocabulary be whatever that requires.
        NodeKind::TempBar => "TempBar",
        NodeKind::SunArc => "SunArc",
        NodeKind::MoonPhase => "MoonPhase",
        NodeKind::AqiContour => "AqiContour",
        NodeKind::StockPlot => "StockPlot",
        NodeKind::Scroll => "ScrollYView",
        _ => "View",
    }
}

/// `flow:` for a container kind.
fn flow(kind: NodeKind) -> Option<&'static str> {
    match kind {
        NodeKind::Row => Some("Right"),
        NodeKind::Stack => Some("Overlay"),
        NodeKind::Column | NodeKind::Grid | NodeKind::Card | NodeKind::Chip | NodeKind::Scroll => {
            Some("Down")
        }
        _ => None,
    }
}

/// Sizing, from whichever of the four the node carries.
///
/// `w`/`h` are explicit pixels; `fillw`/`fillh` and `fitw`/`fith` are the
/// intent. A node that says nothing gets nothing — forcing `Fill` on a hero
/// wrapped "Top Movers" one character per line on device, and forcing it on a
/// chip made the first chip eat the row.
fn sizing_of(kind: NodeKind, a: &Attrs, out: &mut String) {
    sizing(a, out);
    // A CONTAINER with no stated height gets `Fit`, not nothing.
    //
    // The kit says `{t: "row", fillw: 1, …}` and leaves height to the backend,
    // because a row's height is whatever its contents need — that is what "fit"
    // means and it is not worth restating on every node. This emitter wrote no
    // height at all, and a makepad View with no height is zero: the stock list
    // rendered as a panel containing four hairlines, the dividers being the only
    // things with an intrinsic size.
    //
    // `makepad::lower` wrote `height: Fit` on every container, which is why it
    // never hit this. The default belongs here rather than in the kit: it is
    // this backend's convention, and the kit is shared.
    if flow(kind).is_some() && a.h.is_none() && a.fillh.is_none() && a.fith.is_none() {
        out.push_str(" height: Fit");
    }
}

fn sizing(a: &Attrs, out: &mut String) {
    match (a.w, a.fillw, a.fitw) {
        (Some(w), _, _) => {
            let _ = write!(out, " width: {w}");
        }
        (_, Some(1), _) => out.push_str(" width: Fill"),
        (_, _, Some(1)) => out.push_str(" width: Fit"),
        _ => {}
    }
    match (a.h, a.fillh, a.fith) {
        (Some(h), _, _) => {
            let _ = write!(out, " height: {h}");
        }
        (_, Some(1), _) => out.push_str(" height: Fill"),
        (_, _, Some(1)) => out.push_str(" height: Fit"),
        _ => {}
    }
}

/// Padding and spacing, in the shapes this DSL takes them.
fn box_model(a: &Attrs, out: &mut String) {
    // `padtop`/`padbottom` override `pady`, because a page's top padding clears
    // the status bar and its bottom clears the gesture bar — different numbers,
    // and a single symmetric `pady` sat the whole card 30px too high.
    let top = a.padtop.or(a.pady);
    let bottom = a.padbottom.or(a.pady);
    match (a.pad, a.padx, top, bottom) {
        (Some(p), None, None, None) => {
            let _ = write!(out, " padding: {p}");
        }
        (_, x, t, b) if x.is_some() || t.is_some() || b.is_some() => {
            let (x, t, b) = (x.unwrap_or(0.0), t.unwrap_or(0.0), b.unwrap_or(0.0));
            let _ = write!(out, " padding: Inset{{left: {x} right: {x} top: {t} bottom: {b}}}");
        }
        _ => {}
    }
    // Same asymmetry as padding: a panel separates itself from what is ABOVE it,
    // and repeating that below doubles the gap between two stacked panels.
    let mt = a.margintop.or(a.marginy);
    let mb = a.marginbottom.or(a.marginy);
    match (a.margin, a.marginx, mt, mb) {
        (Some(m), None, None, None) => {
            let _ = write!(out, " margin: {m}");
        }
        (_, x, t, b) if x.is_some() || t.is_some() || b.is_some() => {
            let (x, t, b) = (x.unwrap_or(0.0), t.unwrap_or(0.0), b.unwrap_or(0.0));
            let _ = write!(out, " margin: Inset{{left: {x} right: {x} top: {t} bottom: {b}}}");
        }
        _ => {}
    }
    if let Some(s) = a.spacing {
        let _ = write!(out, " spacing: {s}");
    }
}

/// A node, wrapped in a hit target when it declares one.
///
/// The tap MUST be a transparent `Button` over the content, not an attribute.
/// An earlier version of this wrote `l0_tapto:` onto the node and nothing read
/// it — the VM does not hit-test an arbitrary attribute, so the card rendered
/// perfectly and every row was dead. That is the identical mistake
/// `makepad::lower` made and had to be corrected for, and writing it a second
/// time is why it is spelled out here.
fn emit(node: &UiNode, out: &mut String, depth: usize) {
    let Some(target) = node.attrs.tapto.as_deref() else {
        return emit_widget(node, out, depth);
    };
    let pad = "  ".repeat(depth.min(32));
    // The wrapper sizes like what it wraps, or a hug-content target stops
    // hugging: a filling wrapper made the first chip claim the whole row.
    let width = match (node.attrs.w, node.attrs.fillw, node.attrs.fitw) {
        (Some(w), _, _) => format!("width: {w}"),
        (_, Some(1), _) => "width: Fill".into(),
        _ => "width: Fit".into(),
    };
    let _ = writeln!(out, "{pad}View{{ {width} height: Fit flow: Overlay");
    emit_widget(node, out, depth + 1);
    // `agent.notify`'s first argument must stay a LITERAL: the host's
    // `tag_notify_calls` rewrites only literals, and an untagged event is
    // discarded as unattributable — so the target travels in the payload.
    let _ = writeln!(
        out,
        "{pad}  Button{{ width: Fill height: Fill draw_bg.color: #00000000 \
         draw_bg.color_hover: #ffffff08 draw_bg.color_focus: #00000000 \
         draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 \
         draw_bg.border_radius: 10 text: \"\" \
         on_click: || agent.notify(\"l0kit\", {{target: {target:?}}}) }}"
    );
    let _ = writeln!(out, "{pad}}}");
}

fn emit_widget(node: &UiNode, out: &mut String, depth: usize) {
    let pad = "  ".repeat(depth.min(32));
    let a = &node.attrs;
    let w = widget(node.kind);

    let _ = write!(out, "{pad}{w}{{");
    if let Some(f) = flow(node.kind) {
        let _ = write!(out, " flow: {f}");
    }
    sizing_of(node.kind, a, out);
    box_model(a, out);

    if let Some(bg) = a.bg {
        let _ = write!(out, " draw_bg.color: {}", hex(bg));
    }
    if let Some(r) = a.radius {
        let _ = write!(out, " draw_bg.border_radius: {r}");
    }
    if node.kind == NodeKind::Text {
        if let Some(t) = a.text.as_deref() {
            // Always a literal. An earlier version of this passed a value
            // through unquoted when it looked like `"$" + sys.stock(…)`, which
            // misread where the pipeline evaluates: that expression is called
            // during `l0_eval::build`, so by the time a node reaches here the
            // text is the RESULT. What arrives is a string, including
            // `$[Error:WrongValue]` when the capability was not registered.
            let _ = write!(out, " text: {t:?}");
        }
        if let Some(c) = a.color {
            let _ = write!(out, " draw_text.color: {}", hex(c));
        }
        if let Some(s) = a.size {
            let _ = write!(out, " draw_text.text_style.font_size: {s}");
        }
    }
    if node.kind == NodeKind::Image {
        if let Some(src) = a.src.as_deref() {
            let _ = write!(out, " src: http_resource({src:?})");
        }
    }
    // A visualisation's parameters. The attribute names are this backend's, not
    // the model's: `lo` becomes `draw_bg.tlo` because the shader's uniform is
    // called that, and a mismatch draws a bar against zero — which looks like
    // data rather than like a bug.
    match node.kind {
        NodeKind::TempBar => {
            let _ = write!(
                out,
                " draw_bg.tlo: {} draw_bg.thi: {} draw_bg.wmin: {} draw_bg.wmax: {}",
                num(a.lo), num(a.hi), num(a.min), num(a.max)
            );
        }
        NodeKind::SunArc => {
            let _ = write!(
                out,
                " draw_bg.rise: {} draw_bg.set: {} draw_bg.now: {}",
                num(a.rise), num(a.set), num(a.now)
            );
        }
        NodeKind::MoonPhase => {
            let _ = write!(out, " draw_bg.phase: {}", num(a.phase));
        }
        NodeKind::AqiContour => {
            let _ = write!(
                out,
                " lat: {} lon: {} span: {}",
                num(a.lat.map(|v| v as f32)),
                num(a.lon.map(|v| v as f32)),
                num(a.span)
            );
        }
        NodeKind::StockPlot => {
            let _ = write!(
                out,
                " symbol: {:?} range: {:?}",
                a.symbol.as_deref().unwrap_or(""),
                a.range.as_deref().unwrap_or("")
            );
        }
        _ => {}
    }
    if node.kind == NodeKind::WeatherIcon {
        if let Some(v) = a.variant.as_deref() {
            let _ = write!(out, " cond: {v:?}");
        }
    }
    if node.children.is_empty() {
        out.push_str(" }\n");
        return;
    }
    out.push('\n');
    for child in &node.children {
        emit(child, out, depth + 1);
    }
    let _ = writeln!(out, "{pad}}}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ARGB in, makepad hex out — and the two disagree about byte order.
    ///
    /// The kit builds `((a*256+r)*256+g)*256+b`; makepad reads `#RRGGBBAA`.
    /// Passing the integer through unchanged renders the alpha as red, which on
    /// this palette turns a barely-there panel fill into an opaque block: a card
    /// that looks plausible with every surface wrong.
    #[test]
    fn a_colour_survives_the_byte_order_change() {
        assert_eq!(hex(0x12FF_FFFF), "#ffffff12"); // panel: white at alpha 0x12
        assert_eq!(hex(0xFF0A_0E14), "#0a0e14ff"); // page base, opaque
        assert_eq!(hex(0xFF32_D74B), "#32d74bff"); // a rise
    }

    /// A node that asks for nothing gets nothing.
    ///
    /// Forcing `Fill` wrapped a hero one character per line on device, and made
    /// the first chip eat the row. The absence of an attribute is a decision.
    /// A declared tap becomes a REAL hit target, not an attribute.
    ///
    /// The first version of this wrote `l0_tapto:` onto the node, and nothing
    /// read it — the VM does not hit-test an arbitrary attribute, so the card
    /// rendered perfectly with every row dead. `makepad::lower` made the
    /// identical mistake earlier and had to be corrected; this test exists
    /// because it was then made a second time here.
    #[test]
    fn a_tap_becomes_a_hit_target_not_an_attribute() {
        let node = UiNode {
            kind: NodeKind::Row,
            attrs: Attrs {
                tapto: Some("l0:{\"e\":\"open_quote\",\"k\":\"root/x\"}".into()),
                fillw: Some(1),
                ..Default::default()
            },
            children: vec![],
        };
        let dsl = to_dsl(&node);
        assert!(dsl.contains("flow: Overlay"), "needs an overlay wrapper:\n{dsl}");
        assert!(dsl.contains("Button{"), "needs a hit target:\n{dsl}");
        assert!(dsl.contains("agent.notify(\"l0kit\""), "must reach the host:\n{dsl}");
        assert!(dsl.contains("root/x"), "the instance key must survive:\n{dsl}");
        // And the discriminating half: no tap, no target.
        let plain = to_dsl(&UiNode {
            kind: NodeKind::Row,
            attrs: Attrs::default(),
            children: vec![],
        });
        assert!(!plain.contains("Button{"), "only declared taps:\n{plain}");
    }

    #[test]
    fn sizing_is_only_what_the_node_asked_for() {
        let mut out = String::new();
        sizing(&Attrs::default(), &mut out);
        assert!(out.is_empty(), "got {out:?}");

        let mut out = String::new();
        sizing(&Attrs { fillw: Some(1), h: Some(44.0), ..Default::default() }, &mut out);
        assert_eq!(out, " width: Fill height: 44");
    }
}
