//! What every plan-lowered card shares: the theme, and the layout invariants that
//! were each a live bug.
//!
//! None of this is an input. A plan cannot set a colour, a size, a spacing or a root
//! height — those are not fields, so they cannot be wrong. Changing a value here
//! restyles every card with **no model call**, which is the other half of the point: a
//! look change costs a token edit rather than a 45-second regeneration that may alter
//! unrelated things.

/// Colour, type and metric tokens.
///
/// Two card families, because the domains genuinely differ: weather sits over a
/// full-bleed photo and needs translucent panels, while news and stock are dark
/// surfaces where a panel must be opaque enough to read against.
pub mod theme {
    /// Weather: panels float over a city photograph.
    pub const PHOTO_BASE: &str = "#0a0e14";
    pub const PHOTO_SCRIM: &str = "#00000066";
    pub const PHOTO_PANEL: &str = "#00000055";
    pub const PHOTO_TILE: &str = "#ffffff1f";
    /// A FIXED height taller than the content. A `Fit` Overlay takes its tallest
    /// child, so a photo shorter than the column ends in a hard edge of bare base
    /// colour partway down the card.
    pub const PHOTO_H: u32 = 2000;

    /// News and stock: an opaque dark surface.
    pub const DARK_BASE: &str = "#0b0b0d";
    pub const DARK_PANEL: &str = "#141821";
    pub const DARK_CARD: &str = "#ffffff12";
    pub const DARK_ROW: &str = "#ffffff0d";
    pub const HAIRLINE: &str = "#ffffff1a";

    pub const TEXT: &str = "#ffffff";
    pub const TEXT_SOFT: &str = "#ffffffe6";
    pub const TEXT_DIM: &str = "#ffffffb3";
    pub const TEXT_FAINT: &str = "#ffffff99";
    pub const TEXT_MUTED: &str = "#ffffff77";
    pub const ROW_DIM: &str = "#ffffff88";

    /// Accents. `UP`/`DOWN` are market direction; `ACCENT` is the news warm orange.
    pub const ACCENT: &str = "#ff9f0a";
    pub const UP: &str = "#32d74b";
    // Direction colours come as a pair; DOWN has no emitter yet because the
    // builders fetch direction live (`sys.stockrange(..,"up")`).
    #[allow(dead_code)]
    pub const DOWN: &str = "#ff453a";

    pub const PAGE_PAD: &str = "Inset{left: 20 top: 54 right: 20 bottom: 24}";
    pub const PANEL_RADIUS: &str = "20.0";
    pub const CARD_RADIUS: &str = "14.0";
    pub const TILE_RADIUS: &str = "18.0";
    pub const GAP: u32 = 16;
    pub const ROW_H: u32 = 40;
    pub const MAP_H: u32 = 190;

    /// Forecast-row geometry.
    pub const DAY_W: u32 = 92;
    pub const ICON_W: u32 = 34;
    pub const TEMP_W: u32 = 46;
    /// A right-aligned label sets its text flush to the box edge and `°` overhangs the
    /// clip, rendering as `29ᶜ`. Widening the box does not help — alignment moves the
    /// text with the edge. This padding pulls the digits back inside AND makes the gaps
    /// either side of the bar equal.
    pub const TEMP_PAD: u32 = 5;
    pub const BAR_MARGIN: u32 = 10;
    pub const BAR_H: u32 = 8;
}

/// Wrap a lowered body in the card root.
///
/// EXACTLY ONE top-level node, by construction. Sibling top-level nodes lay out SIDE
/// BY SIDE, so an extra background node does not sit behind the card — it takes half
/// the width and squeezes the card into the other half. That was a real generated bug,
/// and it is now impossible rather than forbidden.
///
/// The root is `Fit` and the card list does the scrolling. A fixed root height
/// truncates everything past it — nothing scrolls because nothing *can*, and the
/// content below is simply discarded.
pub fn photo_root(name: &str, photo_query: &str, body: &str) -> String {
    format!(
        "// name: {name}\n\
         // LOWERED from a semantic plan — do not edit.\n\
         SolidView{{ width: Fill height: Fit flow: Overlay new_batch: true draw_bg.color: {base}\n\
         \x20   Image{{ src: http_resource(sys.photo({photo_query:?})) fit: ImageFit.CropToFill \
         width: Fill height: {ph} }}\n\
         \x20   SolidView{{ width: Fill height: Fill draw_bg.color: {scrim} }}\n\
         \x20   View{{ width: Fill height: Fit flow: Down padding: {pad}\n\
         {body}\x20   }}\n\
         }}\n",
        base = theme::PHOTO_BASE,
        ph = theme::PHOTO_H,
        scrim = theme::PHOTO_SCRIM,
        pad = theme::PAGE_PAD,
    )
}

/// The same guarantees for an opaque dark card (news, stock).
pub fn dark_root(name: &str, body: &str) -> String {
    format!(
        "// name: {name}\n\
         // LOWERED from a semantic plan — do not edit.\n\
         SolidView{{ width: Fill height: Fit flow: Down new_batch: true draw_bg.color: {base} \
         padding: {pad}\n\
         {body}}}\n",
        base = theme::DARK_BASE,
        pad = theme::PAGE_PAD,
    )
}

/// A text role plus a colour. Roles carry weight, size and the COMPLETE glyph chain
/// (Roboto → NotoSans for arrows → LXGWWenKai for CJK → NotoColorEmoji), so a card
/// cannot lose coverage — which is what rendered 上海 as tofu boxes when the model
/// wrote its own Roboto-only `font_family`. Colour stays per-use because it genuinely
/// varies; weight and coverage do not.
pub fn text(role: &str, expr: &str, color: &str, extra: &str) -> String {
    format!("{role}{{ {extra}text: {expr} draw_text.color: {color} }}")
}

/// A locale tag for the `sys.*` helpers that localise their own output.
pub fn locale_tag(locale: &str) -> &'static str {
    if locale.starts_with("zh") {
        "zh"
    } else {
        "en"
    }
}
