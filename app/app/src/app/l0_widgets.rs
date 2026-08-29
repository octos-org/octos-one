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
    let mut body = String::new();
    // Per document, not per process: the names must line up with THIS tree's maps.
    MAPS.with(|m| *m.borrow_mut() = (0, String::new()));
    ROW_REVEALS.with(|r| *r.borrow_mut() = 0);
    let live = LIVE.with(|l| {
        *l.borrow_mut() = Some(Vec::new());
        emit(root, &mut body, 0);
        l.borrow_mut().take().unwrap_or_default()
    });

    let mut out = String::from("// REALIZED from an L0 ledger — do not edit.\n");
    if live.is_empty() {
        out.push_str(&body);
        return out;
    }

    // HOIST the constant sub-calls, then TICK the rest — which is exactly the shape
    // of the card this replaces, and the half of it I got wrong the first time.
    //
    // A live text node's expression is `sys.navstep(searchnum×4, sys.navprog(searchnum×4,
    // gps×2), …)`. Emitted into `fn tick()` as-is it runs every frame: measured on a
    // OnePlus 6 at 644% CPU and 27 hitches, worse than the rebuild it replaced.
    //
    // The place lookups do not change while driving. So they become top-level `let`s,
    // evaluated once at build, and the tick references those — leaving only `sys.gps`
    // live per frame. `a2app/apps/nav` does the same thing with `olat`/`olon`/`dlat`/
    // `dlon`, and its R9.5 note is about precisely this: a top-level `let` freezes at
    // build, which is what makes it cheap and why anything that DOES change must stay
    // inside the tick.
    let (lets, ticks) = hoist_constants(&live);
    for (name, expr) in &lets {
        let _ = writeln!(out, "let {name} = {expr}");
    }
    out.push_str(&body);
    out.push_str("\nfn tick() {\n");
    for (name, expr) in &ticks {
        let _ = writeln!(out, "    ui.{name}.set_text({expr})");
    }
    out.push_str("}\n");
    out
}

thread_local! {
    /// The live text nodes emitted by the current `to_dsl`, in emission order.
    ///
    /// A thread local rather than a parameter because `emit` recurses through eight
    /// call sites and only two of them care.
    /// The map most recently emitted, and how many have been emitted.
    ///
    /// A map's controls are drawn by `l0_surface_map` as siblings that FOLLOW it in
    /// the same subtree, so "the map this control drives" is "the last one emitted".
    /// Both halves were previously the constant `l0map`: every `MapView` claimed the
    /// same instance name and every control called it, so a card with two maps —
    /// two routes side by side, a plan over a preview — emitted a duplicate id and
    /// pointed both control columns at whichever one won. The nav card is safe only
    /// because its guards leave one map realized at a time, which is a property of
    /// that card and not of this emitter.
    static MAPS: std::cell::RefCell<(usize, String)> =
        const { std::cell::RefCell::new((0, String::new())) };

    static LIVE: std::cell::RefCell<Option<Vec<(String, String)>>> =
        const { std::cell::RefCell::new(None) };

    /// Whether the container currently being emitted hugs its width. Text
    /// children consult the PARENT's entry: a Label's default `width: Fill`
    /// inside a hug (`fitw`) container resolves to ZERO width in makepad —
    /// the same trap `l0_tap_fit` hit, found a third time when a fit row's
    /// condition word and both feels captions rendered as nothing.
    static HUGS: std::cell::RefCell<Vec<bool>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// RAII entry in `HUGS`: pushed for the node being emitted, popped on every
/// return path of `emit_widget` — which has several, so a manual pop is a bug
/// waiting for the next early return.
struct HugGuard;
impl HugGuard {
    fn push(hugs: bool) -> Self {
        HUGS.with(|h| h.borrow_mut().push(hugs));
        HugGuard
    }
}
impl Drop for HugGuard {
    fn drop(&mut self) {
        HUGS.with(|h| {
            h.borrow_mut().pop();
        });
    }
}

fn parent_hugs() -> bool {
    HUGS.with(|h| h.borrow().last().copied().unwrap_or(false))
}

/// A Roboto text style at this size and weight.
///
/// The L0 path emitted `font_size` alone, which leaves makepad's default face. Every
/// other card in this app names Roboto explicitly — the weather card's hero is
/// `Roboto-Thin`, its labels `Roboto-Medium` — so an L0 card was the only surface in
/// the product not using the product's typeface, at a glance slightly wrong and hard
/// to name.
///
/// The weight comes from the kit, which already says whether a run is body text or a
/// heading; this maps that onto the three faces the app bundles.
/// Does this run stay inside the bundled Roboto's coverage?
///
/// Roboto carries Latin-1 and little beyond it. Forcing it on EVERY run put the
/// product's typeface on an L0 card — and drew a tofu box for anything outside that
/// range. Measured on device three times in one day: `◎` for a recenter control, `→`
/// for an onward action, and `↑`/`↓` in a generated weather card's high/low, which
/// rendered as `□32° □25°`. The first two were mine and I could pick other glyphs;
/// the third was the MODEL's, and it will keep reaching for arrows.
///
/// So the family is only stated when the text is known to fit it. Anything else keeps
/// makepad's default face, which covers the arrows and the CJK the app's own chrome
/// already renders. A different face is a smaller cost than a missing glyph.
fn fits_roboto(text: Option<&str>) -> bool {
    match text {
        // Unknown at lowering time — a live value's text is not in the DSL. Do not
        // gamble the font on it.
        None => false,
        Some(t) => t.chars().all(|c| (c as u32) < 0x0250 || c == '°'),
    }
}

/// The style for a run, naming the family only when the text is known to fit it.
fn text_style_for(size: f32, weight: Option<i32>, text: Option<&str>) -> String {
    if fits_roboto(text) {
        return text_style(size, weight);
    }
    // The DOTTED form, not a whole `TextStyle{}`. Replacing the style replaces its
    // font STACK, and a style with no family resolves to no font at all — the arrows
    // and the CJK stopped drawing entirely, which is worse than the tofu it was
    // meant to fix. Setting only the size leaves makepad's default face and the
    // fallbacks that cover them, which is what this backend emitted before it began
    // naming Roboto.
    format!(" draw_text.text_style.font_size: {size}")
}

fn text_style(size: f32, weight: Option<i32>) -> String {
    let face = match weight.unwrap_or(400) {
        // The hairline face the photo mood's hero asks for (weight_hero = 100).
        // Already bundled: makepad_widgets ships Roboto-Thin in every APK.
        w if w <= 250 => "Roboto-Thin",
        w if w >= 600 => "Roboto-Bold",
        w if w >= 500 => "Roboto-Medium",
        _ => "Roboto-Regular",
    };
    format!(
        " draw_text.text_style: TextStyle{{ font_family: FontFamily{{ \
         latin := FontMember{{ res: crate_resource(\"makepad_widgets:resources/{face}.ttf\") \
         asc: 0.0 desc: 0.0 }} }} font_size: {size} }}"
    )
}

/// Is this the container a swipe reveals?
fn is_reveal(node: &UiNode) -> bool {
    node.attrs.action.as_deref() == Some("reveal")
}

/// This subtree with every `Reveal` removed, and the reveals themselves.
///
/// The reveal has to leave the swipe overlay or the transparent swipe button sits on
/// top of it and swallows the tap it exists to receive — which is exactly what
/// happened: `End` appeared on swipe-up and did nothing. The shipping nav card puts
/// its `endrow` outside the swipe target for the same reason.
///
/// It is not a direct child of the sheet: the card says `Panel(dock: .bottom) { …
/// Reveal { … } }`, so the panel sits between them. Hence a walk rather than a filter.
fn split_reveals(node: &UiNode) -> (UiNode, Vec<UiNode>) {
    let mut revealed = Vec::new();
    let mut kept = Vec::new();
    for child in &node.children {
        if is_reveal(child) {
            revealed.push(child.clone());
            continue;
        }
        let (c, mut r) = split_reveals(child);
        revealed.append(&mut r);
        kept.push(c);
    }
    let mut compact = node.clone();
    compact.children = kept;
    (compact, revealed)
}

/// The call a tagged map control makes, if it is one.
///
/// One table, so the theme's tag and the widget's method cannot drift apart. `0.7`
/// and `-0.7` are the L2 card's own steps — a zoom control that moves by a different
/// amount than the app it replaces is a different control.
fn map_control_call(action: &str) -> Option<String> {
    // The map this control belongs to, not "the map". See `MAPS`.
    let map = MAPS.with(|m| m.borrow().1.clone());
    if map.is_empty() {
        // A control with no map above it is a theme bug, not a card one, and a
        // call on a name nothing declares is a parse error in the whole document —
        // one stray control would blank the card. Drawing the button dead is the
        // smaller failure and it is visible in a screenshot.
        return None;
    }
    Some(match action {
        "zoomin" => format!("ui.{map}.nav_zoom_by(\"0.7\")"),
        "zoomout" => format!("ui.{map}.nav_zoom_by(\"-0.7\")"),
        "recenter" => format!("ui.{map}.set_nav_recenter(\"1\")"),
        _ => return None,
    })
}

/// Does anything in this subtree take a tap?
///
/// A swipe target is a transparent `Button` laid OVER the sheet's summary, and a
/// button over a control is a control that cannot be used. The plan screen puts
/// every input it has — both places, the stop, the three travel modes, `Go` — in
/// the bottom panel, so covering that panel made the whole screen inert while it
/// went on rendering perfectly.
fn takes_a_tap(node: &UiNode) -> bool {
    node.attrs.tapto.is_some() || node.children.iter().any(takes_a_tap)
}

/// Register a live text node and return the name to emit it under.
fn live_name(call: &str) -> Option<String> {
    LIVE.with(|l| {
        let mut slot = l.borrow_mut();
        let found = slot.as_mut()?;
        let name = format!("l0v{}", found.len());
        found.push((name.clone(), call.to_owned()));
        Some(name)
    })
}

/// A list of `(name, expression)` bindings — the shape both halves of the hoist
/// return, named so the signature reads as one thing rather than four nested types.
type Bindings = Vec<(String, String)>;

/// Pull every constant sub-call out into a named binding.
///
/// "Constant" means: a `sys.*` call that does not read the device's position. Those
/// are the expensive ones — a place lookup parses its cached response every time it is
/// evaluated — and they answer the same thing for the life of the card. What is left
/// in the tick reads `sys.gps` and must run per frame.
fn hoist_constants(live: &[(String, String)]) -> (Bindings, Bindings) {
    let mut lets: Vec<(String, String)> = Vec::new();
    let mut ticks = Vec::new();
    for (name, expr) in live {
        let mut rewritten = expr.clone();
        // Innermost-first: a hoisted call must not still contain an unhoisted one.
        while let Some((start, end)) = innermost_constant_call(&rewritten) {
            let call = rewritten[start..end].to_owned();
            let bound = match lets.iter().find(|(_, e)| *e == call) {
                Some((n, _)) => n.clone(),
                None => {
                    let n = format!("l0c{}", lets.len());
                    lets.push((n.clone(), call));
                    n
                }
            };
            rewritten.replace_range(start..end, &bound);
        }
        ticks.push((name.clone(), rewritten));
    }
    (lets, ticks)
}

/// Which byte offsets of an expression are inside a STRING LITERAL.
///
/// The hoister must not reach into one. Card state arrives as a quoted argument, so
/// a place name is free to contain anything — and a name like
/// `cafe sys.navsecs(1) bar` was hoisted straight out of its quotes into
/// `let l0c0 = sys.navsecs(1)`: a host call the card never declared, with arguments
/// chosen by whoever typed the name. Found in review, reproduced, fixed here.
///
/// That is an authority escalation, not a cosmetic bug. The whole confinement
/// argument is that card text can only ever be data; a hoister that treats it as
/// code breaks exactly that.
fn literal_mask(expr: &str) -> Vec<bool> {
    let mut mask = vec![false; expr.len()];
    let mut in_str = false;
    let mut escape = false;
    for (i, c) in expr.char_indices() {
        if in_str {
            for m in mask.iter_mut().skip(i).take(c.len_utf8()) {
                *m = true;
            }
        }
        if escape {
            escape = false;
            continue;
        }
        match c {
            '\\' if in_str => escape = true,
            '"' => {
                // The quote itself belongs to the literal either way.
                for m in mask.iter_mut().skip(i).take(c.len_utf8()) {
                    *m = true;
                }
                in_str = !in_str;
            }
            _ => {}
        }
    }
    mask
}

/// The span of an innermost `sys.*` call that reads no device position.
fn innermost_constant_call(expr: &str) -> Option<(usize, usize)> {
    let bytes = expr.as_bytes();
    let mask = literal_mask(expr);
    let mut best: Option<(usize, usize)> = None;
    let mut at = 0;
    while let Some(rel) = expr[at..].find("sys.") {
        let start = at + rel;
        // Inside a quoted argument this is TEXT, not a call.
        if mask.get(start).copied().unwrap_or(false) {
            at = start + 4;
            continue;
        }
        // The call's own span, by balanced parens.
        let open = {
            let mut probe = start;
            loop {
                let Some(rel) = expr[probe..].find('(') else {
                    return best;
                };
                let hit = probe + rel;
                if !mask.get(hit).copied().unwrap_or(false) {
                    break hit;
                }
                probe = hit + 1;
            }
        };
        let mut depth = 0usize;
        let mut end = None;
        for (i, b) in bytes[open..].iter().enumerate() {
            // A paren inside a quoted argument is TEXT. Counting it cut the call's
            // span short at the first `)` in a place name — which then hoisted half
            // a call and left the rest of the name looking like code, so a name
            // containing `sys.navsecs(1)` still became a binding even with the
            // occurrences masked.
            if mask.get(open + i).copied().unwrap_or(false) {
                continue;
            }
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end?;
        // Innermost: no nested CALL inside, and nothing position-dependent. Both
        // questions ignore anything inside a literal — a place name containing
        // "sys." is text, and counting it made the enclosing call look nested, so
        // nothing was hoisted at all and the per-frame cost came straight back.
        let real_call_at = |from: usize| {
            let mut probe = from;
            while let Some(rel) = expr[probe..end].find("sys.") {
                let hit = probe + rel;
                if !mask.get(hit).copied().unwrap_or(false) {
                    return true;
                }
                probe = hit + 4;
            }
            false
        };
        let nested = real_call_at(start + 4);
        let reads_position = {
            let mut probe = start;
            let mut found = false;
            while let Some(rel) = expr[probe..end].find("sys.gps") {
                let hit = probe + rel;
                if !mask.get(hit).copied().unwrap_or(false) {
                    found = true;
                    break;
                }
                probe = hit + 7;
            }
            found
        };
        if !nested && !reads_position {
            // The shortest such span, so repeated hoisting terminates.
            if best.is_none_or(|(bs, be)| end - start < be - bs) {
                best = Some((start, end));
            }
        }
        at = start + 4;
    }
    best
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

/// A card's range token, as the chart widget's.
///
/// The card declares `enum[d1, w1, m1, m6, y1]` — an L0 enum member cannot start
/// with a digit — and `yahoo_range_params` reads `1d`, `1w`, `1m`, `6m`, `1y`.
/// The two were passed straight through, so every token except the fallback was
/// unknown and every chip drew the SAME intraday chart. The chips highlighted
/// correctly and the plot refetched, which is what made it look like it worked.
///
/// The translation belongs here: the enum is the card's vocabulary and the
/// parameter is this backend's, and §1.1 puts that seam in this file.
fn plot_range(token: &str) -> &str {
    match token {
        "d1" => "1d",
        "w1" => "1w",
        "m1" => "1m",
        "m6" => "6m",
        "y1" => "1y",
        // Already in the widget's spelling, or empty — the widget defaults.
        other => other,
    }
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
        //
        // `RoundedShadowView`, not `RoundedView`: it is a strict SUPERSET —
        // identical fill, gradient, radius and border uniforms, plus
        // `shadow_color` / `shadow_radius` / `shadow_offset`. That is what
        // implements `Attrs.elevation`, which every layer above declared and no
        // renderer has ever drawn. A card that sets no elevation writes
        // `shadow_color: #0000`, so the pixels are unchanged and the four device
        // goldens still hold; the shadow costs fill rate only when asked for.
        NodeKind::Card | NodeKind::Chip => "RoundedShadowView",
        // A divider is a filled rule: it needs a background, and a bare `View`
        // does not draw one.
        NodeKind::Divider => "SolidView",
        NodeKind::WeatherIcon => "WeatherIcon",
        // The map is a real widget here, so the role reaches it as one. `Map`
        // was lowered by neither backend before, which put an error box in the
        // middle of the nav card ON DEVICE — the device renders through the kit.
        // NAMED, because a control has to call it. `nav_zoom_by` and
        // `set_nav_recenter` are methods, so the button the theme asked for needs a
        // receiver — and a widget with no id cannot be one. A card with two maps
        // would collide here; the nav card has one per screen and only one renders.
        // Named per instance by `emit_widget`, which owns the counter.
        NodeKind::Map | NodeKind::NavMap => "MapView",
        // The five data visualisations. This backend already ships all six as
        // native widgets — which is why §1.1 says to prove the pipeline here
        // first and let the vocabulary be whatever that requires.
        NodeKind::TempBar => "TempBar",
        NodeKind::SunArc => "SunArc",
        NodeKind::MoonPhase => "MoonPhase",
        NodeKind::AqiContour => "AqiContour",
        NodeKind::StockPlot => "StockPlot",
        NodeKind::IndicatorPlot => "IndicatorPlot",
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
    // A row poster — a non-filling image that has a source — is portrait 2:3.
    // The realized node carries a wide (≈16:9) box that squashed the poster;
    // emit the poster-shaped tile instead. A full-width backdrop (fillw) and a
    // sourceless placeholder fall through to normal sizing untouched.
    if kind == NodeKind::Image
        && a.fillw != Some(1)
        && a.src.as_deref().is_some_and(|s| !s.is_empty())
    {
        out.push_str(" width: 118 height: 177");
        return;
    }
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
    // Alignment, which this emitter dropped entirely.
    //
    // The kit sets it — `aligny` on every row, `alignx` on a column the card
    // asked to centre — and none of it reached the DSL, so the weather card's
    // place name, icon and hero temperature sat against the left margin while
    // the reference rendering centred all three. A `Align{}` with neither axis
    // is not written: makepad reads the two independently and stating a default
    // would override the widget's own.
    if a.alignx.is_some() || a.aligny.is_some() {
        let x = a.alignx.map(|v| format!("x: {v}"));
        let y = a.aligny.map(|v| format!("y: {v}"));
        let parts: Vec<String> = [x, y].into_iter().flatten().collect();
        let _ = write!(out, " align: Align{{{}}}", parts.join(" "));
    }
}

/// The `agent.notify` channel an L0 tap arrives on.
///
/// Shared rather than written at both ends. The emitter said `"l0kit"` and the
/// handler tested for `"l0"`, so every tap on every L0 card was dropped — the
/// cards rendered perfectly and nothing on them worked. Two string literals in
/// two files cannot be checked against each other; one constant can.
pub const TAP_CHANNEL: &str = "l0kit";

/// The instance key, event name and payload carried by a tap.
///
/// `kit::tap_target` writes `l0:{"e":…,"k":…,"v":…}` as ONE string, because
/// `tag_notify_calls` rewrites only a literal channel and everything else has to
/// travel in the payload. The `l0:` prefix separates it from the renderer's own
/// `set:` verbs. This is the only reader; the shape is not restated anywhere.
pub fn parse_tap(target: &str) -> Option<(String, String, String)> {
    let json = target.strip_prefix("l0:")?;
    let t: serde_json::Value = serde_json::from_str(json).ok()?;
    let field = |k: &str| t.get(k).and_then(|v| v.as_str()).unwrap_or("").to_owned();
    Some((field("k"), field("e"), field("v")))
}

// A node is wrapped in a hit target when it declares one, and the tap MUST be a
// transparent `Button` over the content, not an attribute. An earlier version of
// this wrote `l0_tapto:` onto the node and nothing read it — the VM does not
// hit-test an arbitrary attribute, so the card rendered perfectly and every row
// was dead. That is the identical mistake `makepad::lower` made and had to be
// corrected for, and writing it a second time is why it is spelled out here.
//
// Plain `//`: this is rationale for the tap-wrapping approach as a whole, not
// documentation of the `thread_local!` below, which is about row reveals.
std::thread_local! {
    /// Per-document counter for row-reveal names (`l0rr0`, `l0rr1`, …) — one
    /// per swipe-revealed row, so each row's swipe drives ITS OWN chip.
    static ROW_REVEALS: std::cell::RefCell<usize> = const { std::cell::RefCell::new(0) };
}

/// A row that reveals its action on swipe: `Row { <tapped content> Reveal { … } }`.
///
/// The row the finger is ON is the one that opens — the reveal is widget
/// visibility scoped to this row, not card state, so no other row moves and
/// nothing re-realizes. The swipe overlay carries the inner content's tap as
/// its own click (the Button fires swipe INSTEAD of click, so a drag cannot
/// also tap), and the revealed chip stays OUTSIDE the overlay so the button
/// cannot swallow the tap it exists to receive.
fn row_reveal(node: &UiNode, out: &mut String, depth: usize) -> bool {
    if node.kind != NodeKind::Row {
        return false;
    }
    if !node.children.iter().any(is_reveal) {
        return false;
    }
    let (compact, revealed) = split_reveals(node);
    let name = ROW_REVEALS.with(|r| {
        let mut r = r.borrow_mut();
        let n = format!("l0rr{}", *r);
        *r += 1;
        n
    });
    // The row's tap lives on its filling inner content row; the swipe overlay
    // takes it over as its click so one surface answers both gestures.
    let inner_tap = compact
        .children
        .iter()
        .find_map(|c| c.attrs.tapto.as_deref())
        .unwrap_or("");
    let pad = "  ".repeat(depth.min(32));
    // A row is a `View{ flow: Right }` in this dsl — there is no `Row` widget,
    // and an unknown ident does not error, it QUIETLY DROPS the node: the
    // first cut wrote `Row{` and every saved row simply vanished on device
    // while the store held them and the movers rendered fine.
    let _ = write!(out, "{pad}View{{ flow: Right");
    sizing_of(node.kind, &node.attrs, out);
    box_model(&node.attrs, out);
    // A row a thumb swipes needs a thumb's worth of height: the compact
    // one-line row was a thin target and strokes kept missing it (asked for
    // by hand on device). 12dp above and below lands the row near the 48dp
    // touch minimum without the card saying a number.
    let _ = write!(out, " padding: Inset{{top: 12 bottom: 12}}");
    out.push('\n');
    let h = "  ".repeat((depth + 1).min(32));
    let _ = writeln!(out, "{h}View{{ width: Fill height: Fit flow: Overlay");
    for child in &compact.children {
        // The inner content renders WITHOUT its own tap wrapper — the swipe
        // overlay above it is the one surface for both gestures.
        emit_widget(child, out, depth + 2);
    }
    let click = if inner_tap.is_empty() {
        String::new()
    } else {
        format!(" on_click: || agent.notify({TAP_CHANNEL:?}, {{target: {inner_tap:?}}})")
    };
    let _ = writeln!(
        out,
        "{h}  Button{{ width: Fill height: Fill draw_bg.color: #00000000 \
         draw_bg.color_down: #ffffff12 draw_bg.border_size: 0.0 \
         draw_bg.border_radius: 10 text: \"\" swipe: true \
         on_swipe_left: || ui.{name}.set_visible(true) \
         on_swipe_right: || ui.{name}.set_visible(false){click} }}"
    );
    let _ = writeln!(out, "{h}}}");
    for r in &revealed {
        // Named and hidden: the swipe on this row's overlay is what shows it.
        let _ = writeln!(out, "{h}{name} := View{{ visible: false width: Fit height: Fit");
        for child in &r.children {
            emit(child, out, depth + 2);
        }
        let _ = writeln!(out, "{h}}}");
    }
    let _ = writeln!(out, "{pad}}}");
    true
}

fn emit(node: &UiNode, out: &mut String, depth: usize) {
    if row_reveal(node, out, depth) {
        return;
    }
    // A text input binds its RETURN key rather than being covered by a hit
    // target. A transparent button over a field would swallow the focus, and
    // there would be nothing to type into.
    //
    // `on_return` hands back what was typed, and `$$` in the target is where it
    // goes — the payload is the text, which does not exist until commit, so the
    // target is assembled at that moment instead of baked at lowering time.
    if node.kind == NodeKind::Input {
        let pad = "  ".repeat(depth.min(32));
        let a = &node.attrs;
        let target = a.tapto.as_deref().unwrap_or("");
        let (head, tail) = target.split_once("$$").unwrap_or((target, ""));
        let _ = write!(out, "{pad}TextInput{{");
        sizing_of(node.kind, a, out);
        box_model(a, out);
        if let Some(bg) = a.bg {
            let _ = write!(out, " draw_bg.color: {}", hex(bg));
        }
        if let Some(r) = a.radius {
            let _ = write!(out, " draw_bg.border_radius: {r}");
        }
        if let Some(c) = a.color {
            let _ = write!(out, " draw_text.color: {}", hex(c));
        }
        if let Some(s) = a.size {
            out.push_str(&text_style_for(s, a.weight, a.text.as_deref()));
        }
        // CENTRED in its own box, via `label_align` — the property `TextInput` actually
        // reads for its text and its placeholder (`text_input.rs`, used at the
        // `draw_walk` for both). Setting the container's `align` did nothing, because
        // that positions the widget, not the run inside it.
        //
        // A field is 48 high and its text sat at the top of that, so every place name
        // floated above the pill it was supposed to be inside — in every screenshot of
        // the trip planner, and easy to misread as "the box is too tall".
        out.push_str(" label_align: Align{y: 0.5}");
        let _ = write!(out, " text: {:?}", a.text.as_deref().unwrap_or(""));
        let _ = write!(
            out,
            " empty_text: {:?}",
            a.placeholder.as_deref().unwrap_or("")
        );
        if !target.is_empty() {
            let _ = write!(
                out,
                " on_return: |t| agent.notify({TAP_CHANNEL:?}, {{target: {head:?} + t + {tail:?}}})"
            );
        }
        // And the same, per KEYSTROKE. `TextInput` calls `on_change` with the text so
        // far, exactly as it calls `on_return` with the committed text, so a search
        // box can list results while you type and still commit a destination on
        // return. Two moments, two declared events, one widget.
        if let Some(changing) = a.changeto.as_deref().filter(|c| !c.is_empty()) {
            let (h, t2) = changing.split_once("$$").unwrap_or((changing, ""));
            // `"c":1` marks this dispatch as a KEYSTROKE, so the app can apply the
            // state change immediately but COALESCE the expensive re-render — a
            // full re-resolve per character re-laid the sheet out under the
            // user's finger, which read as the field jittering while they typed.
            let h = h.replacen("\"v\":\"", "\"c\":1,\"v\":\"", 1);
            let _ = write!(
                out,
                " on_change: |t| agent.notify({TAP_CHANNEL:?}, {{target: {h:?} + t + {t2:?}}})"
            );
        }
        let _ = writeln!(out, " }}");
        return;
    }
    // A MAP CONTROL. The card said `controls: .zoom`; the theme drew a square with a
    // glyph and tagged it, and the imperative call belongs here — this is the layer
    // §1.1 allows to know about widgets and methods. The card cannot say
    // `ui.themap.nav_zoom_by("0.7")`, which is exactly what the L2 card says.
    if let Some(call) = node.attrs.action.as_deref().and_then(map_control_call) {
        let pad = "  ".repeat(depth.min(32));
        let w = node.attrs.w.map(|w| format!("width: {w}")).unwrap_or("width: Fit".into());
        let h = node.attrs.h.map(|h| format!("height: {h}")).unwrap_or("height: Fit".into());
        let _ = writeln!(out, "{pad}View{{ {w} {h} flow: Overlay");
        emit_widget(node, out, depth + 1);
        let _ = writeln!(
            out,
            "{pad}  Button{{ width: Fill height: Fill draw_bg.color: #00000000 \
             draw_bg.color_down: #ffffff1a draw_bg.border_size: 0.0 \
             draw_bg.border_radius: 12 text: \"\" on_click: || {call} }}"
        );
        let _ = writeln!(out, "{pad}}}");
        return;
    }
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
         on_click: || agent.notify({TAP_CHANNEL:?}, {{target: {target:?}}}) }}"
    );
    let _ = writeln!(out, "{pad}}}");
}

fn emit_widget(node: &UiNode, out: &mut String, depth: usize) {
    let pad = "  ".repeat(depth.min(32));
    let a = &node.attrs;
    let in_hug = parent_hugs();
    let _hug = HugGuard::push(a.fitw == Some(1));
    let w = widget(node.kind);

    // A live text node is NAMED, so `fn tick()` can set it without a rebuild. The
    // kit stamps the call onto `action` (see `l0_live`); the name is assigned here in
    // emission order so it matches the tick written after the tree.
    let named = if node.kind == NodeKind::Text {
        a.action.as_deref().and_then(live_name)
    } else {
        None
    };
    // A REVEAL starts hidden and is named, so a swipe on its sheet can show it
    // without touching card state. Card state would re-resolve the card, re-parse the
    // document and rebuild the `MapView` — up to 327 ms of frozen map. The shipping
    // nav card toggles `ui.endrow.set_visible` for precisely this reason.
    let reveal = a.action.as_deref() == Some("reveal");
    // A map claims the next instance name and becomes the one its controls drive.
    let map_name = matches!(node.kind, NodeKind::Map | NodeKind::NavMap).then(|| {
        MAPS.with(|m| {
            let mut m = m.borrow_mut();
            let name = format!("l0map{}", m.0);
            m.0 += 1;
            m.1 = name.clone();
            name
        })
    });
    if let Some(name) = &map_name {
        let _ = write!(out, "{pad}{name} := {w}{{");
    }
    match &named {
        _ if map_name.is_some() => {}
        Some(name) => {
            let _ = write!(out, "{pad}{name} := {w}{{");
        }
        None if reveal => {
            let _ = write!(out, "{pad}l0reveal := {w}{{ visible: false");
        }
        None => {
            let _ = write!(out, "{pad}{w}{{");
        }
    }
    if let Some(f) = flow(node.kind) {
        let _ = write!(out, " flow: {f}");
    }
    sizing_of(node.kind, a, out);
    box_model(a, out);

    if let Some(bg) = a.bg {
        // No `show_bg: true` here, and that is not an oversight. `View` declares
        // `#[live(false)] show_bg`, so this looks like a fill that never paints —
        // it is not. Assigning `draw_bg.*` through the script apply path enables
        // the background, and a device A/B on 2026-08-25 measured the light
        // mood's page at #f2f2f7 both with and without an explicit `show_bg`.
        // Verify against an INTERIOR pixel if you re-open this: the card carries
        // a thin dark edge margin, and sampling x=14 reads the margin, not the page.
        let _ = write!(out, " draw_bg.color: {}", hex(bg));
    }
    // A second stop makes the fill a vertical gradient: the view shader mixes
    // color -> color_2 down `pos.y` (dithered) whenever color_2 is set, so a
    // scrim can be light where the photograph is and dark under the content.
    if let Some(bg2) = a.bg2 {
        let _ = write!(out, " draw_bg.color_2: {}", hex(bg2));
    }
    if let Some(r) = a.radius {
        let _ = write!(out, " draw_bg.border_radius: {r}");
    }
    // NOT REACHING THE SCREEN as of 2026-08-25, and left in place deliberately.
    // The emission is correct — the panel node carries
    // `draw_bg.border_size: 1 draw_bg.border_color: #00000026`, RoundedView's
    // shader strokes when border_size > 0, and no apply error is logged — yet a
    // deliberately garish 4px opaque red border produced zero red pixels on
    // device. `border_radius`, a uniform on the same prototype, works. So the
    // failure is below this layer and stroke is NOT the cheap win it looked
    // like; it needs widget-layer investigation before it can be measured.
    //
    // A stroke on a surface. NOT halved, unlike the sibling renderer: that one
    // targets a shader which draws to both sides of the edge, while this
    // RoundedView insets its box by `border_size` and strokes the inset path,
    // so the visible width is the value itself. Halving here emitted a 0.5px
    // stroke at 15% alpha, which measured as nothing on device. Only Card/Chip
    // declare the uniforms; a plain View would silently discard them.
    if matches!(node.kind, NodeKind::Card | NodeKind::Chip) {
        if let Some(b) = a.border.filter(|b| *b > 0.0) {
            let _ = write!(out, " draw_bg.border_size: {b}");
            // ALWAYS write the ink rather than leaving it unset: `border_color`
            // is `instance(#0000)` on the prototype, so an uncoloured border is
            // a correctly-sized stroke that paints nothing.
            //
            // BISECTED ON DEVICE 2026-08-30, and the answer is NOT what the
            // earlier note here guessed. `border_size` DOES reach the shader:
            // rendering the light baseline at `panel_border: 30` insets the
            // white fill by 82 device px — exactly 30 logical px at this
            // phone's 2.75 scale — and the card's own rows visibly hang outside
            // the shrunken fill. So the uniform applies and `sdf.box` insets by
            // it. What never appears is the STROKE: that 82px band renders as
            // plain page background at every ink tried, including opaque red
            // and a low-alpha red chosen to rule out the u32 > i32::MAX
            // hypothesis (argb(255,255,0,0) is 4294901760). `hex()` emits
            // #RRGGBBAA correctly, and the SAME instance mechanism works for
            // `draw_bg.color` (the fill) and `draw_bg.shadow_color` (below).
            //
            // So the failure is isolated to `sdf.stroke` / `border_color` in
            // the RoundedShadowView prototype, below anything octos-one
            // controls. Fixing it is makepad widget work, not emitter work.
            let ink = a.bordercolor.unwrap_or(0xff000000);
            let _ = write!(out, " draw_bg.border_color: {}", hex(ink));
        }
        // `Attrs.elevation` — declared by the node, read by the evaluator
        // (`l0_eval.rs:158`), and until now written by nothing at all. Material
        // semantics, which is what the field's own doc comment promises: the
        // shadow both softens and drops as the surface rises.
        //
        // The ink is derived here rather than themed because carrying a
        // `shadowcolor` would mean editing `Attrs` in the splash-node
        // submodule, and this change stays inside octos-one. The consequence is
        // that only SOFT shadows are expressible today — a hard offset block
        // (neubrutalist, memphis) needs a themed ink and near-zero blur, and
        // that is the measured +0.64 capability. `shadow_offset` below is the
        // uniform that will carry it, so the follow-up is one contract field,
        // not new shader work.
        if let Some(e) = a.elevation.filter(|e| *e > 0.0) {
            let alpha = ((0.10 + 0.02 * e).min(0.38) * 255.0) as u32;
            let _ = write!(out, " draw_bg.shadow_color: {}", hex(alpha << 24));
            let _ = write!(out, " draw_bg.shadow_radius: {:.1}", e * 1.5);
            let _ = write!(out, " draw_bg.shadow_offset: vec2(0.0, {:.1})", e * 0.5);
        } else {
            // Explicitly transparent: the prototype defaults `shadow_color` to
            // `#0007`, so every card would otherwise gain a shadow it never
            // asked for and all four goldens would move.
            let _ = write!(out, " draw_bg.shadow_color: #00000000");
        }
    }
    if node.kind == NodeKind::Text {
        // The eyebrow role: tracked caps, faked in the STRING because the
        // text stack has no tracking axis the DSL reaches — thin spaces
        // between uppercased characters of a LITERAL. A live value arrives
        // after emission and stays untransformed, which degrades gracefully.
        let eyebrow = a.variant.as_deref() == Some("eyebrow");
        if let Some(t) = a.text.as_deref() {
            if eyebrow && !t.contains("sys.") {
                let spaced: String = t
                    .to_uppercase()
                    .chars()
                    .flat_map(|c| [c, '\u{2009}'])
                    .collect();
                let spaced = spaced.trim_end_matches('\u{2009}');
                let _ = write!(out, " text: {spaced:?}");
            }
        }
        if let Some(t) = a.text.as_deref().filter(|_| {
            !eyebrow || a.text.as_deref().unwrap_or("").contains("sys.")
        }) {
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
            out.push_str(&text_style_for(s, a.weight, a.text.as_deref()));
        }
        // A Label already wraps (its layout is `Flow::right_wrap`) — it just
        // needs a BOUNDED width to wrap against, or it sizes to content and
        // CLIPS a long title/synopsis at the card edge. Fill the parent when
        // the node stated no width of its own (`sizing_of` already wrote one
        // otherwise, so this never double-writes `width:`).
        if a.w.is_none() && a.fillw.is_none() && a.fitw.is_none() {
            // Inside a hug container Fill is 0 — hug with it instead.
            let _ = write!(out, " width: {}", if in_hug { "Fit" } else { "Fill" });
        }
        // How many lines this run may occupy, and an ellipsis when it overruns.
        // The KIT decides — whether a row title truncates is presentation, not
        // content, so no card names it. Ellipsis needs a bounded width, which
        // the branch above supplies unless the node hugs.
        if let Some(n) = a.lines.filter(|n| *n > 0) {
            let _ = write!(out, " max_lines: {n} text_overflow: TextOverflow.Ellipsis");
        }
    }
    if node.kind == NodeKind::Image {
        // An EMPTY src is not a src. This emitted `http_resource("")`, so a
        // photo whose subject had not resolved yet, or a row thumbnail with no
        // image, still went out as a request — for nothing, once per redraw.
        // Omitted, the widget simply draws no image and the page keeps its base
        // colour and scrim underneath, which is what a card that is still
        // loading should look like.
        if let Some(src) = a.src.as_deref().filter(|s| !s.is_empty()) {
            // A row thumbnail (NOT a full-width backdrop) is almost always a
            // movie/show POSTER — portrait 2:3, not 16:9. Emitted with no size
            // and no fit, the widget defaulted to a wide box + `Stretch`, which
            // squashed the poster. Give a poster-shaped tile (only when the node
            // set no size of its own, so we never double-write `width:`) and
            // `CropToFill` so neither a poster (sized 2:3 in `sizing_of`) nor a
            // backdrop is distorted.
            let _ = write!(out, " fit: ImageFit.CropToFill src: http_resource({src:?})");
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
            // Mood-owned bar treatment (kit: `l0_bar`): rail on `bg`, single
            // hue on `bg2`. Absent, the shader keeps its legacy spectrum.
            if let Some(bar) = a.bg2 {
                let _ = write!(out, " draw_bg.flat_ink: {}", hex(bar));
            }
            if let Some(rail) = a.bg {
                let _ = write!(out, " draw_bg.rail_ink: {}", hex(rail));
            }
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
                plot_range(a.range.as_deref().unwrap_or(""))
            );
        }
        NodeKind::IndicatorPlot => {
            // Strings, not shader uniforms: the widget resolves them into a
            // URL and fetches the series itself.
            let _ = write!(
                out,
                " countries: {:?} indicator: {:?} years: {}",
                a.countries.as_deref().unwrap_or(""),
                a.indicator.as_deref().unwrap_or(""),
                num(a.years)
            );
        }
        _ => {}
    }
    // The condition code, as a NUMBER on the shader's own uniform.
    //
    // This wrote `cond: "2"` — the wrong property AND a quoted string — so the
    // widget kept its default and every day drew the same icon. The reference
    // emitter writes `draw_bg.cond: 2`, and that is the name the shader reads.
    if node.kind == NodeKind::WeatherIcon {
        if let Some(v) = a.variant.as_deref() {
            let _ = write!(out, " draw_bg.cond: {}", v.parse::<f32>().unwrap_or(0.0));
        }
        // Mood-owned mono ink (kit: `icon_mono`): the finished glyph is
        // recoloured to one silhouette ink. Absent, legacy colours.
        if let Some(ink) = a.color {
            let _ = write!(out, " draw_bg.mono_ink: {}", hex(ink));
        }
    }
    // The trip. `variant` carries which member of the map family this is, in the
    // widget's own vocabulary; the route arrives already resolved as a polyline5,
    // because the tree carries values and not requests.
    //
    // A polyline is omitted rather than written empty: `ensure_nav_route` returns
    // early on a blank one, and an empty string would look like a deliberate
    // "no route" instead of a route still in flight.
    // The trip. Every value here comes from the SHIPPING nav card rather than
    // from a guess — `a2app/apps/nav/app.md` is a working four-map reference and
    // its "MANDATORY rules" section says why each one matters.
    //
    // The rule that bites hardest: a `MapView` needs a FIXED PIXEL height.
    // `Fill`/`Fit` "resolve to 0 and hide the map", and the kit's first attempt
    // asked for 240 — which drew a map, correctly, in a letterbox. The shipping
    // card uses 812 for a full-bleed screen and 452/384 for a panel; a card that
    // stacks content above its map gets the panel size.
    //
    // `min_zoom`/`max_zoom` are widened because the widget defaults to 11..17 and
    // clamps into it, so a card asking to see a whole city silently got a street.
    if matches!(node.kind, NodeKind::Map | NodeKind::NavMap) {
        // Network tiles, always: the widget defaults to a local `.mbtiles` file
        // for offline development and an L0 card cannot ship one, so the default
        // draws nothing but the base colour and looks like a broken widget.
        out.push_str(" use_network: true use_local_mbtiles: false");
        // The shipping card's 3..19 zoom range, restored.
        //
        // I widened it, found the app at 441% CPU and 3 GB, and reverted — but
        // the cause was an IMPOSSIBLE CENTRE, not the range: `sys.gps` answers
        // -9999 with no fix and the camera fitted an extent spanning it at world
        // scale. That is now rejected in the widget, where the number is known,
        // so the range is safe again at the layer that was never the problem.
        //
        // And the narrow clamp had a cost that only shows up under a finger. The
        // widget defaults to 11..17; a card asking for `zoom: 11` — a whole-route
        // preview — then sits exactly ON the floor, so pinching to zoom OUT does
        // nothing at all. Half the gesture is dead and the map reads as broken
        // rather than as clamped.
        out.push_str(" min_zoom: 3.0 max_zoom: 19.0 nav_period: 100");
        let mode = a.variant.as_deref().unwrap_or("");
        if !mode.is_empty() {
            let _ = write!(out, " nav_mode: {mode:?}");
        }
        // The ribbon is drawn in ground metres, so a route seen from a
        // whole-route preview needs a far wider line than one seen from the car.
        let ribbon = if mode == "plan" { 40.0 } else { 14.0 };
        let _ = write!(out, " nav_route_width: {ribbon}");
        if let Some(z) = a.zoom {
            let _ = write!(out, " zoom: {z}");
        }
        // The CENTRE IS A NUMBER, and a followed map ignores it.
        //
        // This emitted `center_lat: sys.gps("lat")` as a live expression on the
        // theory that the widget would re-evaluate it per frame. Nothing does: the
        // property is set once when the tree is built, so the camera went from a
        // number that at least changed on every card re-resolve to a constant
        // string that never changed at all. Measured on a OnePlus 6 — 21 fixes,
        // ~105 m of travel, 0.0% of pixels different. The map was frozen, and the
        // epoch threshold had just been raised on the strength of the same theory.
        //
        // A follow camera reads the platform's last fix directly instead — see
        // `update_nav_camera`. That is where a per-frame value belongs, and it needs
        // no rebuild and no property at all. These numbers stay as the fallback for
        // the frame before the first fix lands.
        if let (Some(lat), Some(lon)) = (a.lat, a.lon) {
            let _ = write!(out, " center_lat: {lat} center_lon: {lon}");
        }
        // Omitted rather than written empty: `ensure_nav_route` returns early on
        // a blank one, and an empty string would read as a deliberate "no route"
        // instead of a route still in flight.
        if let Some(poly) = a.polyline.as_deref().filter(|p| !p.trim().is_empty()) {
            let _ = write!(out, " nav_polyline: {poly:?}");
        }
        // The route's pins. Same omit-rather-than-blank rule as the polyline: an
        // empty string is a deliberate "no pins", and a trip still resolving has
        // not said that.
        if let Some(pins) = a.markers.as_deref().filter(|m| !m.trim().is_empty()) {
            let _ = write!(out, " route_markers: {pins:?}");
        }
        // What the route costs, drawn ON it. Same omit-rather-than-blank rule.
        if let Some(b) = a.route_badge.as_deref().filter(|b| !b.trim().is_empty()) {
            let _ = write!(out, " route_badge: {b:?}");
        }
    }
    if node.children.is_empty() {
        out.push_str(" }\n");
        return;
    }
    out.push('\n');
    // The SHEET's handle and summary form ONE swipe target, with the revealed row
    // BELOW it — copied from the shipping nav card, including why.
    //
    // The transparent button sits over the handle and the summary so it (a) catches
    // the swipe across the whole area a thumb lands on and (b) occludes the map, so
    // the gesture never leaks through and pans it. A first attempt put the button
    // over the handle alone: `height: Fill` inside a `Fit` overlay resolved to the
    // handle's 10px, so the target was a hairline and every swipe missed it.
    //
    // The revealed row stays outside that overlay, or the button would sit on top of
    // it and swallow the tap it exists to receive.
    //
    // `set_visible` rather than a notify: the sheet opens and closes without the card
    // re-resolving, which is the whole point — a rebuild here tears down the map.
    let is_sheet = node.attrs.action.as_deref() == Some("sheet");
    if is_sheet {
        let h = "  ".repeat((depth + 1).min(32));
        let (compact, revealed) = split_reveals(node);
        // NO REVEAL, NO SWIPE. A sheet with nothing hidden in it needs neither a
        // handle nor a swipe target, and emitting them anyway is what killed the
        // plan screen: the same `Panel(dock: .bottom)` carries the summary on the
        // driving screen and every control on the planning one, so the transparent
        // button went over the travel-mode chips, both search fields and `Go`. Taps
        // landed on it and nothing dispatched — measured, three targets, no
        // `[l0]` line for any of them.
        if revealed.is_empty() {
            for child in &compact.children {
                emit(child, out, depth + 1);
            }
            let _ = writeln!(out, "{pad}}}");
            return;
        }
        // There IS something to reveal, so the swipe target has to exist. Where the
        // summary is inert — the driving sheet, a distance and a time — it spans the
        // whole area, which is the shipping card's shape and deliberate: it catches
        // the swipe wherever a thumb lands and occludes the map so the gesture
        // cannot leak through and pan it. Where the summary has controls, the target
        // shrinks to the handle strip, because a swipe that needs a precise thumb is
        // a smaller loss than a button that cannot be pressed.
        let summary_has_controls = compact.children.iter().any(takes_a_tap);
        if summary_has_controls {
            let _ = writeln!(out, "{h}View{{ width: Fill height: Fit flow: Down{}",
                match node.attrs.alignx {
                    Some(x) => format!(" align: Align{{x: {x}}}"),
                    None => String::new(),
                });
            // An explicit height, not `Fill`: inside a `Fit` overlay `Fill` resolved
            // to the handle's own 5px and every swipe missed the hairline.
            let _ = writeln!(out, "{h}  View{{ width: Fill height: 28 flow: Overlay");
            let _ = writeln!(
                out,
                "{h}    View{{ width: Fill height: Fill flow: Right align: Align{{x: 0.5 y: 0.5}}"
            );
            let _ = writeln!(
                out,
                "{h}      RoundedView{{ width: 44 height: 5 draw_bg.color: #3a4658 draw_bg.border_radius: 3 }}"
            );
            let _ = writeln!(out, "{h}    }}");
            let _ = writeln!(
                out,
                "{h}    Button{{ width: Fill height: Fill draw_bg.color: #00000000 \
                 draw_bg.border_size: 0.0 text: \"\" swipe: true \
                 on_swipe_up: || ui.l0reveal.set_visible(true) \
                 on_swipe_down: || ui.l0reveal.set_visible(false) }}"
            );
            let _ = writeln!(out, "{h}  }}");
            for child in &compact.children {
                emit(child, out, depth + 2);
            }
            let _ = writeln!(out, "{h}}}");
            for child in &revealed {
                emit(child, out, depth + 1);
            }
            let _ = writeln!(out, "{pad}}}");
            return;
        }
        let _ = writeln!(out, "{h}View{{ width: Fill height: Fit flow: Overlay");
        // The sheet's own `alignx` has to be restated HERE.
        //
        // It is written on the sheet container, and these two boxes — the overlay
        // and the summary column — sit between it and the text. Both are
        // `width: Fill`, so centring the sheet's children centred boxes that
        // already spanned it, and the number inside stayed hard against the left
        // margin. The shipping card solves it the same way, per-label rather than
        // per-container: `Label{ width: Fill align: Align{x: 0.5} }`.
        let inner_align = match node.attrs.alignx {
            Some(x) => format!(" align: Align{{x: {x}}}"),
            None => String::new(),
        };
        let _ = writeln!(
            out,
            "{h}  View{{ width: Fill height: Fit flow: Down{inner_align}"
        );
        let _ = writeln!(
            out,
            "{h}    View{{ width: Fill height: 12 flow: Right align: Align{{x: 0.5 y: 0.5}}"
        );
        let _ = writeln!(
            out,
            "{h}      RoundedView{{ width: 44 height: 5 draw_bg.color: #3a4658 draw_bg.border_radius: 3 }}"
        );
        let _ = writeln!(out, "{h}    }}");
        for child in &compact.children {
            emit(child, out, depth + 3);
        }
        let _ = writeln!(out, "{h}  }}");
        let _ = writeln!(
            out,
            "{h}  Button{{ width: Fill height: Fill draw_bg.color: #00000000 \
             draw_bg.border_size: 0.0 text: \"\" swipe: true \
             on_swipe_up: || ui.l0reveal.set_visible(true) \
             on_swipe_down: || ui.l0reveal.set_visible(false) }}"
        );
        let _ = writeln!(out, "{h}}}");
        for child in &revealed {
            emit(child, out, depth + 1);
        }
        let _ = writeln!(out, "{pad}}}");
        return;
    }
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
        // Through the CONSTANT the handler branches on, not a literal retyped
        // here — a test that spells the channel out again agrees with the
        // emitter and says nothing about the receiver.
        assert!(
            dsl.contains(&format!("agent.notify({TAP_CHANNEL:?}")),
            "must reach the host on the channel the handler listens to:\n{dsl}"
        );
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

    /// An image with no source asks for nothing.
    ///
    /// This emitted `http_resource("")`, so a photo whose subject had not
    /// resolved yet still went out as a request — once per redraw, for nothing.
    /// It paired badly with `sys.photo`, which was handed the placeholder while
    /// the geocode was in flight and generated an AI image of an em dash before
    /// generating the real one.
    #[test]
    fn an_image_with_no_source_asks_for_nothing() {
        let image = |src: &str| UiNode {
            kind: NodeKind::Image,
            attrs: Attrs { src: Some(src.to_owned()), ..Default::default() },
            children: vec![],
        };
        let page = |kid: UiNode| UiNode {
            kind: NodeKind::Column,
            attrs: Attrs::default(),
            children: vec![kid],
        };
        let with = to_dsl(&page(image("https://example.test/a.jpg")));
        assert!(
            with.contains("src: http_resource(\"https://example.test/a.jpg\")"),
            "a real source still reaches the widget:\n{with}"
        );
        let without = to_dsl(&page(image("")));
        assert!(
            !without.contains("http_resource"),
            "an empty source must not become a request:\n{without}"
        );
    }

    /// The sheet's three shapes, and which of them may cover its own content.
    ///
    /// A swipe target is a transparent `Button` over the summary, so it decides
    /// whether anything under it can be pressed:
    ///
    ///   - nothing to reveal  -> no handle, no button. The planning screen puts every
    ///     control it has in the bottom panel, and covering it made the whole screen
    ///     inert: the travel-mode chips, both search fields and `Go` all stopped
    ///     dispatching while rendering perfectly.
    ///   - a reveal, inert summary -> the button spans the sheet. The shipping card's
    ///     shape: it catches a swipe wherever a thumb lands, and occludes the map so
    ///     the gesture cannot pan it.
    ///   - a reveal AND controls -> the button shrinks to the handle strip, at an
    ///     explicit height because `Fill` inside a `Fit` overlay collapsed to the
    ///     handle's 5px and every swipe missed.
    ///
    /// The alignment is checked on the summary column, not the sheet: the sheet's
    /// `alignx` sits two full-width boxes above the text, so centring the sheet
    /// centred those and left the hero hard against the left margin.
    #[test]
    fn a_sheet_covers_its_own_content_only_when_it_has_something_to_reveal() {
        let text = |s: &str| UiNode {
            kind: NodeKind::Text,
            attrs: Attrs {
                text: Some(s.into()),
                ..Default::default()
            },
            children: vec![],
        };
        let tappable = || UiNode {
            kind: NodeKind::Column,
            attrs: Attrs {
                tapto: Some("l0:{\"e\":\"pick_mode\",\"k\":\"root/Chip\"}".into()),
                ..Default::default()
            },
            children: vec![text("Walk")],
        };
        let reveal = || UiNode {
            kind: NodeKind::Column,
            attrs: Attrs {
                action: Some("reveal".into()),
                ..Default::default()
            },
            children: vec![text("End")],
        };
        let sheet = |kids: Vec<UiNode>| {
            to_dsl(&UiNode {
                kind: NodeKind::Column,
                attrs: Attrs {
                    action: Some("sheet".into()),
                    fillw: Some(1),
                    alignx: Some(0.5),
                    ..Default::default()
                },
                children: kids,
            })
        };

        // Nothing to reveal: no swipe target at all, so nothing is covered.
        let plain = sheet(vec![tappable()]);
        assert!(
            !plain.contains("swipe: true"),
            "a sheet with nothing hidden needs no swipe target:\n{plain}"
        );
        assert!(
            !plain.contains("height: 12"),
            "and no handle to suggest one:\n{plain}"
        );
        assert!(
            plain.contains("agent.notify"),
            "its control still dispatches:\n{plain}"
        );

        // A reveal over an inert summary: the button spans the sheet, and the column
        // the text sits in carries the centring.
        let inert = sheet(vec![text("31 min"), reveal()]);
        assert!(
            inert.contains("swipe: true"),
            "something to reveal needs a swipe:\n{inert}"
        );
        assert!(
            inert.contains("flow: Down align: Align{x: 0.5}"),
            "the column the text sits in must be centred:\n{inert}"
        );

        // A reveal over controls: the swipe lives on the handle strip, so the button
        // is emitted BEFORE the controls rather than over them.
        let both = sheet(vec![tappable(), reveal()]);
        assert!(
            both.contains("swipe: true") && both.contains("height: 28"),
            "the swipe shrinks to the handle strip:\n{both}"
        );
        let button_at = both.find("swipe: true").expect("has a swipe");
        let control_at = both.find("agent.notify").expect("has a control");
        assert!(
            button_at < control_at,
            "the swipe target must not be laid over the control:\n{both}"
        );
    }
}

#[cfg(test)]
mod wire_tests {
    use super::{parse_tap, to_dsl, TAP_CHANNEL};
    use splash_node::{Attrs, NodeKind, UiNode};

    /// The tap payload the emitter writes is the one the handler reads.
    ///
    /// This is the wire that was broken: `l0_widgets` sent `"l0kit"` with
    /// `{target: "l0:{…}"}` and `main.rs` tested for `"l0"` with flat
    /// `key`/`event`/`value`, so every tap on every L0 card was silently
    /// dropped. The cards rendered correctly and nothing on them worked.
    ///
    /// Nothing caught it because the only tests that exercised a tap called
    /// `l0_card::tap` DIRECTLY — including the seeded `SEED_L0_EVENT` path — so
    /// dispatch was well covered and the wire between the button and dispatch
    /// was covered nowhere. This test starts at the emitted DSL.
    #[test]
    fn the_emitted_tap_is_the_one_the_handler_parses() {
        // Exactly what `splash_ui_l0::kit::tap_target` writes.
        let target = "l0:{\"e\":\"set_range\",\"k\":\"root/detail/ranges#0/Chip#2\",\"v\":\"m1\"}";
        let dsl = to_dsl(&UiNode {
            kind: NodeKind::Chip,
            attrs: Attrs {
                tapto: Some(target.to_owned()),
                fitw: Some(1),
                ..Default::default()
            },
            children: vec![],
        });
        assert!(
            dsl.contains(&format!("agent.notify({TAP_CHANNEL:?}, {{target:"))
                && dsl.contains("set_range"),
            "the button must notify the handler's channel with a `target`:\n{dsl}"
        );

        let (key, event, value) = parse_tap(target).expect("the handler parses it");
        assert_eq!(event, "set_range", "the event must survive the wire");
        assert_eq!(value, "m1", "the payload must survive — it is WHICH range");
        assert_eq!(
            key, "root/detail/ranges#0/Chip#2",
            "the instance key must survive: it is which chip was tapped"
        );

        // A target that is not ours is refused rather than half-read. The
        // renderer has its own `set:` verbs on the same channel shape.
        assert!(parse_tap("set:count:1").is_none(), "a foreign verb is not a tap");
    }
}

#[cfg(test)]
mod range_tests {
    /// Every declared range token must reach the chart as one it understands.
    ///
    /// `yahoo_range_params` falls back to intraday for anything unknown, so a
    /// mistranslated token is not an error — it is the 1D chart under a 1Y chip.
    /// All five were mistranslated, and the only visible symptom was that the
    /// chart looked plausible whichever chip was lit.
    #[test]
    fn every_declared_range_maps_to_one_the_chart_knows() {
        // The chart's own vocabulary, from `yahoo_range_params`.
        let known = ["1d", "1w", "1m", "6m", "1y"];
        for token in ["d1", "w1", "m1", "m6", "y1"] {
            let mapped = super::plot_range(token);
            assert!(
                known.contains(&mapped),
                "range `{token}` mapped to `{mapped}`, which the chart does not know"
            );
        }
        // And they stay DISTINCT: collapsing two onto one range is the same
        // failure with a smaller blast radius.
        let mut seen: Vec<&str> = ["d1", "w1", "m1", "m6", "y1"]
            .iter()
            .map(|t| super::plot_range(t))
            .collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 5, "each range must map to a different window");
    }
}

#[cfg(test)]
mod field_tests {
    use super::{to_dsl, TAP_CHANNEL};
    use splash_node::{Attrs, NodeKind, UiNode};

    /// A `Field` binds its RETURN key and is never covered by a hit target.
    ///
    /// Two things had to be different from a tap and both are easy to get wrong.
    /// A transparent button over a text input swallows the focus, so there would
    /// be nothing to type into. And the payload is what was TYPED, which does
    /// not exist at lowering time — so the target carries a `$$` hole that the
    /// emitted handler fills with the committed text.
    #[test]
    fn a_field_commits_what_was_typed_on_the_tap_channel() {
        let dsl = to_dsl(&UiNode {
            kind: NodeKind::Input,
            attrs: Attrs {
                text: Some("nvid".into()),
                placeholder: Some("Search a ticker".into()),
                tapto: Some(
                    "l0:{\"e\":\"search\",\"k\":\"root/Field#0\",\"v\":\"$$\"}".into(),
                ),
                fillw: Some(1),
                ..Default::default()
            },
            children: vec![],
        });

        assert!(dsl.contains("TextInput{"), "must be a real input:\n{dsl}");
        assert!(
            !dsl.contains("Button{"),
            "a hit target over a field eats the focus:\n{dsl}"
        );
        // The committed text is spliced in where `$$` was, and reaches the host
        // on the same channel a tap does.
        assert!(
            dsl.contains(&format!("on_return: |t| agent.notify({TAP_CHANNEL:?}")),
            "the return key must reach the handler:\n{dsl}"
        );
        assert!(
            dsl.contains(r#"\"v\":\"" + t + "\"}"#),
            "the typed text must be spliced into the payload:\n{dsl}"
        );
        // The placeholder is what the field shows when empty — dropping it
        // leaves a box with no indication of what it wants.
        assert!(
            dsl.contains(r#"empty_text: "Search a ticker""#),
            "the placeholder must survive:\n{dsl}"
        );
    }
}

#[cfg(test)]
mod map_tests {
    use super::*;

    /// A map's camera mode must reach the widget as the card declared it.
    ///
    /// This emitter is the last hop, and it is the one place where a wrong value
    /// is invisible in both directions. `MapView` treats an unknown `nav_mode` as
    /// a flat map — no route ribbon at all — and treats `"2d"` as a SIMULATED
    /// drive: a vehicle moving along the route at an assumed 34 mph off a looping
    /// clock. So a mistranslation either erases the route or fabricates the trip,
    /// and both render as a working map.
    ///
    /// `"follow"` is the mode a declared position earns. Splash lowers `.drive`
    /// with an `at:` to it precisely so the camera reports a measurement, and if
    /// this hop rewrote it to `"2d"` the whole argument would be undone one
    /// string substitution below the profile that makes it.
    fn dsl_for(variant: &str) -> String {
        let node = UiNode {
            kind: NodeKind::Map,
            attrs: Attrs {
                variant: Some(variant.to_owned()),
                lat: Some(37.3),
                lon: Some(-122.0),
                zoom: Some(17.0),
                ..Default::default()
            },
            children: vec![],
        };
        let mut out = String::new();
        emit(&node, &mut out, 0);
        out
    }

    #[test]
    fn a_maps_camera_mode_survives_the_last_hop() {
        let follow = dsl_for("follow");
        assert!(
            follow.contains("nav_mode: \"follow\""),
            "a declared position earns the follow camera:\n{follow}"
        );
        assert!(
            !follow.contains("nav_mode: \"2d\""),
            "and must never be rewritten to the SIMULATED drive:\n{follow}"
        );
        // The plan preview keeps its own mode, and its wider ribbon: the route is
        // drawn in ground metres, so a whole-trip view needs a far thicker line
        // than one seen from the car.
        let plan = dsl_for("plan");
        assert!(
            plan.contains("nav_mode: \"plan\"") && plan.contains("nav_route_width: 40"),
            "a preview keeps its mode and its ribbon:\n{plan}"
        );
        assert!(
            follow.contains("nav_route_width: 14"),
            "a driving view gets the narrow ribbon:\n{follow}"
        );
        // And the centre reaches the widget, because a follow camera that ignores
        // it is a static map wearing the mode.
        assert!(
            follow.contains("center_lat: 37.3") && follow.contains("center_lon: -122"),
            "the declared centre must reach the widget:\n{follow}"
        );
    }
}

#[cfg(test)]
mod hoisting {
    //! The hoister must not reach inside a string literal.
    //!
    //! Card state arrives as a quoted argument, so a place name may contain anything.
    //! One containing `sys.navsecs(1)` was hoisted out of its quotes into
    //! `let l0c0 = sys.navsecs(1)` — a host call the card never declared, with
    //! arguments chosen by whoever typed the name. Found in review.
    //!
    //! That is an authority escalation, not a cosmetic bug: the confinement argument
    //! is that card text is only ever data, and a hoister that treats it as code
    //! breaks precisely that.
    use super::hoist_constants;

    #[test]
    fn a_call_inside_a_place_name_is_text_not_code() {
        let live = vec![(
            "l0v0".to_string(),
            r#"sys.search("cafe sys.navsecs(1) bar", 0, "name")"#.to_string(),
        )];
        let (lets, ticks) = hoist_constants(&live);
        // The real call is hoisted; the one inside the name is untouched.
        assert_eq!(lets.len(), 1, "only the outer call is a call: {lets:?}");
        assert_eq!(lets[0].1, r#"sys.search("cafe sys.navsecs(1) bar", 0, "name")"#);
        assert_eq!(ticks[0].1, "l0c0");
        for (_, e) in &lets {
            assert!(
                !e.starts_with("sys.navsecs"),
                "a name's contents became a binding: {e}"
            );
        }
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_literal() {
        // `x\") sys.navsecs(1)` — the escape means the literal continues, so what
        // follows is still text. Tracking quotes without escapes would see the
        // literal close here and hoist the rest.
        let live = vec![(
            "l0v0".to_string(),
            r#"sys.search("x\") sys.navsecs(1)", 0, "name")"#.to_string(),
        )];
        let (lets, _) = hoist_constants(&live);
        assert_eq!(lets.len(), 1, "escaped quotes must not open code: {lets:?}");
        assert!(lets[0].1.starts_with("sys.search("));
    }

    #[test]
    fn the_real_nested_calls_are_still_hoisted() {
        // The point of hoisting: the constant place lookups leave the per-frame path
        // and only the position stays live.
        let live = vec![(
            "l0v0".to_string(),
            "sys.navstep(sys.searchnum(\"A\", 0, \"lat\"), sys.navprog(sys.searchnum(\"A\", 0, \"lat\"), sys.gps(\"lat\")), \"instr\")"
                .to_string(),
        )];
        let (lets, ticks) = hoist_constants(&live);
        assert!(!lets.is_empty(), "the place lookup must be hoisted");
        assert!(
            ticks[0].1.contains("sys.gps("),
            "the live position must STAY in the tick: {}",
            ticks[0].1
        );
        assert!(
            !ticks[0].1.contains("sys.searchnum("),
            "the constant lookup must leave the tick: {}",
            ticks[0].1
        );
    }
}

#[cfg(test)]
mod two_maps {
    use super::to_dsl;
    use splash_node::{Attrs, NodeKind, UiNode};

    fn map() -> UiNode {
        UiNode {
            kind: NodeKind::NavMap,
            attrs: Attrs::default(),
            children: vec![],
        }
    }

    fn control(action: &str) -> UiNode {
        UiNode {
            kind: NodeKind::Column,
            attrs: Attrs {
                action: Some(action.to_owned()),
                ..Attrs::default()
            },
            children: vec![],
        }
    }

    fn surface(kids: Vec<UiNode>) -> UiNode {
        UiNode {
            kind: NodeKind::Column,
            attrs: Attrs::default(),
            children: kids,
        }
    }

    /// Each map is its own instance, and each control drives the one above it.
    ///
    /// Every `MapView` used to be emitted as `l0map` and every control called
    /// `ui.l0map`, so two maps on one screen produced a duplicate id and two control
    /// columns pointing at whichever one the document resolved. The nav card never
    /// showed it because its guards leave exactly one map realized — a property of
    /// that card, not of this emitter, and the next card to compare two routes would
    /// have inherited the bug rather than found it.
    #[test]
    fn each_map_is_its_own_instance_and_its_controls_follow_it() {
        let dsl = to_dsl(&surface(vec![
            surface(vec![map(), control("zoomin")]),
            surface(vec![map(), control("recenter")]),
        ]));
        assert!(dsl.contains("l0map0 := MapView"), "the first map is named:\n{dsl}");
        assert!(dsl.contains("l0map1 := MapView"), "and so is the second:\n{dsl}");
        assert!(
            dsl.contains("ui.l0map0.nav_zoom_by"),
            "the first column drives the first map:\n{dsl}"
        );
        assert!(
            dsl.contains("ui.l0map1.set_nav_recenter"),
            "the second drives the second:\n{dsl}"
        );
        assert!(
            !dsl.contains("ui.l0map."),
            "and nothing calls the old shared name:\n{dsl}"
        );
    }

    /// The counter is per document. Two renders in one process must not drift.
    #[test]
    fn the_names_restart_for_every_tree() {
        let one = to_dsl(&surface(vec![map(), control("zoomin")]));
        let two = to_dsl(&surface(vec![map(), control("zoomin")]));
        assert_eq!(one, two, "the same tree lowers the same way twice");
    }

    /// A control with no map above it draws dead rather than blanking the card.
    ///
    /// `ui.<name>` on a name the document never declares is a parse failure for the
    /// WHOLE document, so one stray control would render nothing at all.
    #[test]
    fn a_control_with_no_map_emits_no_call() {
        let dsl = to_dsl(&surface(vec![control("zoomin")]));
        assert!(!dsl.contains("nav_zoom_by"), "no call without a map:\n{dsl}");
    }

}

#[cfg(test)]
mod stroke_and_shadow_tests {
    use super::*;
    use splash_node::{Attrs, NodeKind, UiNode};

    fn card_with(border: Option<f32>, ink: Option<u32>, lift: Option<f32>) -> String {
        let a = Attrs { border, bordercolor: ink, elevation: lift, ..Default::default() };
        let n = UiNode { kind: NodeKind::Card, attrs: a, children: vec![] };
        to_dsl(&n)
    }

    /// A border with no ink used to emit `border_size` and leave `border_color`
    /// at the prototype's `instance(#0000)` — a correctly sized, fully
    /// transparent stroke. That is the shape of "emits but never renders".
    #[test]
    fn a_border_without_an_ink_still_gets_one() {
        let dsl = card_with(Some(4.0), None, None);
        assert!(dsl.contains("border_size: 4"), "border width missing: {dsl}");
        assert!(dsl.contains("border_color:"), "border ink missing: {dsl}");
        assert!(!dsl.contains("border_color: #00000000"), "ink is transparent: {dsl}");
    }

    /// Elevation is declared on the node and was written by nothing.
    #[test]
    fn elevation_reaches_the_shadow_uniforms() {
        let dsl = card_with(None, None, Some(6.0));
        assert!(dsl.contains("shadow_color:"), "no shadow ink: {dsl}");
        assert!(dsl.contains("shadow_radius:"), "no shadow radius: {dsl}");
        assert!(!dsl.contains("shadow_color: #00000000"), "shadow is transparent: {dsl}");
    }

    /// And a card that asks for no lift must stay byte-identical to before, or
    /// the four device goldens move.
    #[test]
    fn no_elevation_means_an_explicitly_transparent_shadow() {
        let dsl = card_with(None, None, None);
        assert!(dsl.contains("shadow_color: #00000000"), "default shadow not cleared: {dsl}");
    }
}

#[cfg(test)]
mod dump_real_card {
    /// Not an assertion — prints the lowered DSL for the light baseline so the
    /// border/shadow properties can be read on the wire rather than inferred.
    #[test]
    #[ignore]
    fn print_light_baseline_dsl() {
        let src = include_str!("../../../../lab/style-factory/baselines/baseline-light.card");
        let kit = crate::app::l0_card::kit_with_theme(src);
        let head: String = kit.lines().filter(|l| l.contains("l0_stroke") || l.contains("panel_border"))
            .collect::<Vec<_>>().join("\n");
        println!("KIT KNOBS:\n{head}");
    }
}
