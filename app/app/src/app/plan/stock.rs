//! Stock plans → Splash DSL.
//!
//! ## What the model may say
//!
//! A ticker, because the user named a company and resolving "apple" to `AAPL` is world
//! knowledge — the same job as resolving "nvidia" to Santa Clara. Nothing else about
//! the market: no price, no change, no direction, no company name, no market cap. All
//! of those are `sys.stock` / `sys.movers` calls the runtime writes.
//!
//! The direction colour is the subtle one. A card that hardcodes green because the
//! model believed the stock was up will show green on a red day, confidently, forever.
//! So `sys.stockrange(sym, range, "up")` decides it at render time, and the plan has no
//! field for it.
//!
//! ## Movers vs a named quote
//!
//! `MoversList` needs no ticker at all — the market decides who is moving. A
//! `QuoteHeader` needs one, and that is the only place a ticker appears.
//!
//! ## No selection, deliberately
//!
//! The DSL spec this replaces builds a list→detail app with five range chips, all
//! driven by interactive state. A state write currently rebuilds the whole card body,
//! which for this domain means tearing down and rebuilding `StockPlot` on every chip
//! tap. So a plan lowers to ONE view — either the movers list or a single quote — until
//! the in-place binding bridge lands (`docs/CARD-STATE-IDENTITY.md`).

use super::common::{dark_root, text, theme};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub plan: String,
    #[serde(default)]
    pub locale: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub block: String,
    #[serde(default)]
    pub args: Args,
}

/// Note what is absent: no `price`, no `change`, no `up`/`down`, no company `name`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// The ticker, for a named quote. Resolving a company to its symbol is world
    /// knowledge and the model's job; everything the symbol then *means* is not.
    #[serde(default)]
    pub ticker: String,
    /// A chart window. A closed set, so a typo is rejected rather than silently
    /// producing an empty plot.
    #[serde(default)]
    pub range: String,
    /// How many movers rows. Presentation, not fact.
    #[serde(default)]
    pub count: Option<u32>,
    /// Eyebrow and title wording — editorial, no correct answer to look up.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub label: String,
    /// Which stat tiles, from a closed set.
    #[serde(default)]
    pub stats: Vec<String>,
    /// The universe to rank, as tickers. Empty means the whole market.
    ///
    /// Which companies count as "AI" is world knowledge and so the model's job --
    /// the same job as resolving "apple" to `AAPL`. Who among them actually
    /// moved, and by how much, is a fact and stays with the runtime. Without
    /// this the only universe was Yahoo's `day_gainers` screener, so "top AI
    /// movers" could only be answered by putting an AI headline over market-wide
    /// gainers -- confidently wrong, with nothing able to contradict it.
    #[serde(default)]
    pub symbols: Vec<String>,
}

const RANGES: &[&str] = &["1d", "5d", "1mo", "6mo", "1y"];

/// Stat key → (caption_en, caption_zh, `sys.stock` key).
fn stat_spec(k: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match k {
        "price" => ("PRICE", "现价", "price"),
        "prev" => ("PREV CLOSE", "昨收", "prev"),
        "high" => ("HIGH", "最高", "high"),
        "low" => ("LOW", "最低", "low"),
        "open" => ("OPEN", "开盘", "open"),
        "currency" => ("CURRENCY", "货币", "currency"),
        _ => return None,
    })
}

fn validate_ticker(t: &str, i: usize, block: &str) -> Result<(), String> {
    if t.is_empty() {
        return Err(format!("sections[{i}] {block}: needs a ticker"));
    }
    // A symbol is upper-case letters with an optional class suffix. Rejecting a
    // company NAME here is deliberate: "Apple" would fetch nothing and render dashes.
    if !t.chars().all(|c| c.is_ascii_uppercase() || c == '.' || c == '-') || t.len() > 6 {
        return Err(format!(
            "sections[{i}] {block}: {t:?} is not a ticker symbol — \
             resolve the company to its symbol (e.g. \"apple\" → \"AAPL\")"
        ));
    }
    Ok(())
}

pub fn lower(json: &str) -> Result<String, String> {
    let plan: Plan =
        serde_json::from_str(json).map_err(|e| format!("stock plan is not valid: {e}"))?;
    if plan.plan != "stock" {
        return Err(format!("expected a stock plan, got {:?}", plan.plan));
    }
    if plan.sections.is_empty() {
        return Err("stock plan has no sections — name at least a MoversList or QuoteHeader"
            .to_string());
    }
    let zh = plan.locale.starts_with("zh");

    let mut body = String::new();
    for (i, sec) in plan.sections.iter().enumerate() {
        let a = &sec.args;
        let s = match sec.block.as_str() {
            "MoversList" => {
                // Typed, so a malformed universe is named before lowering rather
                // than rendering ten em-dashes.
                for t in &a.symbols {
                    if t.is_empty()
                        || t.len() > 6
                        || !t.bytes().all(|b| b.is_ascii_uppercase() || b == b'.')
                    {
                        return Err(format!(
                            "sections[{i}]: {t:?} is not a ticker — \
                             uppercase letters and dots, up to 6 characters"
                        ));
                    }
                }
                movers_list(a, zh)
            }
            "QuoteHeader" => {
                validate_ticker(&a.ticker, i, "QuoteHeader")?;
                quote_header(a, zh)
            }
            "PriceChart" => {
                validate_ticker(&a.ticker, i, "PriceChart")?;
                price_chart(a, i)?
            }
            "StatGrid" => {
                validate_ticker(&a.ticker, i, "StatGrid")?;
                stat_grid(a, zh, i)?
            }
            other => {
                return Err(format!(
                    "sections[{i}]: unknown block {other:?} — permitted: \
                     MoversList, QuoteHeader, PriceChart, StatGrid"
                ))
            }
        };
        body.push_str(&s);
        body.push('\n');
    }
    Ok(dark_root("stock-app", &body))
}

fn movers_list(a: &Args, zh: bool) -> String {
    let n = a.count.unwrap_or(10).clamp(1, 10) as usize;
    let universe = a.symbols.join(",");
    let eyebrow = if a.label.is_empty() {
        if zh { "今日涨幅榜" } else { "TODAY · TOP GAINERS" }.to_string()
    } else {
        a.label.clone()
    };
    let title = if a.title.is_empty() {
        if zh { "涨幅榜" } else { "Movers" }.to_string()
    } else {
        a.title.clone()
    };
    let mut rows = String::new();
    for r in 0..n {
        rows.push_str(&format!(
            "\x20       View{{ width: Fill height: Fit flow: Right align: Align{{y: 0.5}} \
             padding: Inset{{top: 10 bottom: 10}}\n\
             \x20           {rank}\n\
             \x20           View{{ width: Fill height: Fit flow: Down spacing: 2\n\
             \x20               {sym}\n\
             \x20               {name}\n\
             \x20           }}\n\
             \x20           View{{ width: Fit height: Fit flow: Down align: Align{{x: 1.0}} spacing: 2\n\
             \x20               {price}\n\
             \x20               {pct}\n\
             \x20           }}\n\
             \x20       }}\n\
             \x20       SolidView{{ width: Fill height: 1 draw_bg.color: {hair} }}\n",
            rank = text(
                "TextRow",
                &format!("{:?}", (r + 1).to_string()),
                theme::TEXT_MUTED,
                "width: 28 "
            ),
            sym = text(
                "TextRow",
                &format!("sys.movers({r}, \"symbol\", {universe:?})"),
                theme::TEXT,
                ""
            ),
            name = text(
                "TextCaption",
                &format!("sys.movers({r}, \"name\", {universe:?})"),
                theme::TEXT_MUTED,
                ""
            ),
            price = text(
                "TextRow",
                &format!("\"$\" + sys.movers({r}, \"price\", {universe:?})"),
                theme::TEXT,
                ""
            ),
            // Movers are gainers by definition, so green is a fact here rather than a
            // guess — unlike a named quote, whose direction must be fetched.
            pct = text(
                "TextCaption",
                &format!("sys.movers({r}, \"changepct\", {universe:?})"),
                theme::UP,
                ""
            ),
            hair = theme::HAIRLINE,
        ));
    }
    format!(
        "\x20   View{{ width: Fill height: Fit flow: Down\n\
         \x20       {eb}\n\
         \x20       {ti}\n\
         \x20   }}\n\
         \x20   RoundedView{{ width: Fill height: Fit flow: Down new_batch: true \
         draw_bg.color: {panel} draw_bg.border_radius: {r} margin: Inset{{top: {gap}}} \
         padding: Inset{{left: 14 right: 14 top: 4 bottom: 4}}\n{rows}\x20   }}",
        eb = text("TextCaption", &format!("{eyebrow:?}"), theme::UP, ""),
        // `width: Fill`, because `TextHero` is a 76pt role meant for a hero
        // NUMBER and the title beside it is editorial text of any length. The
        // default "Movers" fits; "AI Movers" ran off the screen and clipped
        // mid-word.
        ti = text("TextHero", &format!("{title:?}"), theme::TEXT, "width: Fill "),
        panel = theme::DARK_PANEL,
        r = theme::PANEL_RADIUS,
        gap = theme::GAP,
    )
}

fn quote_header(a: &Args, _zh: bool) -> String {
    let t = &a.ticker;
    let range = if a.range.is_empty() { "1d" } else { &a.range };
    // Direction is FETCHED, not stated. A plan that asserted "up" would paint a red
    // day green, confidently, for as long as the card exists.
    let up = format!("sys.stockrange({t:?}, {range:?}, \"up\")");
    let sep = " · ";
    format!(
        "\x20   View{{ width: Fill height: Fit flow: Down\n\
         \x20       {sym}\n\
         \x20       {name}\n\
         \x20       {price}\n\
         \x20       View{{ width: Fill height: Fit flow: Right spacing: 8\n\
         \x20           {chg}\n\
         \x20           {dir}\n\
         \x20       }}\n\
         \x20   }}",
        sym = text("TextTitle", &format!("sys.stock({t:?}, \"symbol\")"), theme::TEXT_SOFT, ""),
        name = text(
            "TextCaption",
            &format!("sys.stock({t:?}, \"name\") + {sep:?} + sys.stock({t:?}, \"currency\")"),
            theme::TEXT_MUTED,
            ""
        ),
        price = text(
            "TextHero",
            &format!("\"$\" + sys.stock({t:?}, \"price\")"),
            theme::TEXT,
            "margin: Inset{top: 2} "
        ),
        chg = text(
            "TextStat",
            &format!(
                "sys.stockrange({t:?}, {range:?}, \"change\") + \"  (\" \
                 + sys.stockrange({t:?}, {range:?}, \"changepct\") + \")\""
            ),
            theme::TEXT_DIM,
            ""
        ),
        // An explicit ▲/▼ derived from the live flag, so direction is legible without
        // relying on colour alone.
        dir = text(
            "TextStat",
            &format!("if {up} == \"1\" {{ \"▲\" }} else {{ \"▼\" }}"),
            theme::TEXT_DIM,
            ""
        ),
    )
}

fn price_chart(a: &Args, i: usize) -> Result<String, String> {
    let range = if a.range.is_empty() { "1d" } else { &a.range };
    if !RANGES.contains(&range) {
        return Err(format!(
            "sections[{i}] PriceChart: range {range:?} is not one of {}",
            RANGES.join(", ")
        ));
    }
    // StockPlot already takes semantic arguments — symbol and range, not pixel data.
    // It is the precedent AqiContour was refactored to match.
    Ok(format!(
        "\x20   StockPlot{{ width: Fill height: 170 symbol: {t:?} range: {range:?} \
         margin: Inset{{top: {gap}}} }}",
        t = a.ticker,
        gap = theme::GAP,
    ))
}

fn stat_grid(a: &Args, zh: bool, i: usize) -> Result<String, String> {
    if a.stats.is_empty() {
        return Err(format!(
            "sections[{i}] StatGrid: stats is empty — name at least two of \
             price, prev, high, low, open, currency"
        ));
    }
    let t = &a.ticker;
    let mut cells: Vec<String> = Vec::new();
    for k in &a.stats {
        let (cap_en, cap_zh, key) = stat_spec(k).ok_or_else(|| {
            format!(
                "sections[{i}] StatGrid: unknown stat {k:?} — permitted: \
                 price, prev, high, low, open, currency"
            )
        })?;
        let cap = if zh { cap_zh } else { cap_en };
        cells.push(format!(
            "\x20           RoundedView{{ width: Fill height: Fit flow: Down \
             draw_bg.color: {tile} draw_bg.border_radius: {r} \
             padding: Inset{{left: 14 top: 12 right: 14 bottom: 12}} spacing: 6\n\
             \x20               {c}\n\
             \x20               {v}\n\
             \x20           }}",
            tile = theme::PHOTO_TILE,
            r = theme::CARD_RADIUS,
            c = text("TextCaption", &format!("{cap:?}"), theme::TEXT_FAINT, ""),
            v = text(
                "TextValue",
                &format!("sys.stock({t:?}, {key:?})"),
                theme::TEXT,
                ""
            ),
        ));
    }
    let mut rows = String::new();
    for pair in cells.chunks(2) {
        rows.push_str(&format!(
            "\x20       View{{ width: Fill height: Fit flow: Right spacing: 10 \
             margin: Inset{{top: 10}}\n{}\n\x20       }}\n",
            pair.join("\n")
        ));
    }
    Ok(format!(
        "\x20   View{{ width: Fill height: Fit flow: Down margin: Inset{{top: {gap}}}\n{rows}\x20   }}",
        gap = theme::GAP,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOVERS: &str = r#"{
        "plan": "stock", "locale": "en",
        "sections": [{ "block": "MoversList", "args": { "count": 10 } }]
    }"#;

    const QUOTE: &str = r#"{
        "plan": "stock", "locale": "en",
        "sections": [
            { "block": "QuoteHeader", "args": { "ticker": "AAPL", "range": "1d" } },
            { "block": "PriceChart", "args": { "ticker": "AAPL", "range": "1mo" } },
            { "block": "StatGrid", "args": { "ticker": "AAPL",
              "stats": ["price","prev","high","low"] } }
        ]
    }"#;

    #[test]
    fn every_market_number_is_live() {
        let out = lower(MOVERS).unwrap();
        for r in 0..10 {
            assert!(out.contains(&format!("sys.movers({r}, \"symbol\", \"\")")));
            assert!(out.contains(&format!("sys.movers({r}, \"price\", \"\")")));
        }
        assert!(out.contains("// name: stock-app"));
        // No dollar figure may be baked in.
        assert!(!out.contains("$1"), "no literal price");
    }

    /// The direction of a named quote is FETCHED. Asserting it would paint a red day
    /// green for as long as the card exists.
    #[test]
    fn direction_is_derived_not_asserted() {
        let out = lower(QUOTE).unwrap();
        assert!(out.contains("\"up\")"), "direction comes from sys.stockrange");
        assert!(out.contains("▲"), "and drives an explicit glyph, not colour alone");
    }

    #[test]
    fn a_price_cannot_be_stated_by_the_model() {
        let bad = r#"{"plan":"stock","locale":"en","sections":[
            {"block":"QuoteHeader","args":{"ticker":"AAPL","price":"192.30"}}]}"#;
        let err = lower(bad).unwrap_err();
        assert!(err.contains("price"), "unknown field must be named: {err}");
    }

    /// A company NAME would silently fetch nothing and render dashes, so it is
    /// rejected with the fix spelled out.
    /// "top 10 ai stock movers and shakers" — the query that had no answer.
    #[test]
    fn ai_movers_query_lowers_with_its_own_universe() {
        let plan = r#"{
            "plan": "stock", "locale": "en",
            "sections": [ { "block": "MoversList", "args": {
                "count": 10, "title": "AI Movers", "label": "AI · TOP MOVERS AND SHAKERS",
                "symbols": ["NVDA","AMD","AVGO","SMCI","MU","TSM","MRVL","ARM","CRWV",
                            "PLTR","SNOW","AI","VRT","ANET","ORCL","MSFT","GOOGL","META"]
            } } ]
        }"#;
        let out = lower(plan).expect("AI movers plan must lower");
        assert!(
            out.contains("sys.movers(0, \"symbol\", \"NVDA,AMD,AVGO"),
            "the universe reaches the runtime:\n{out}"
        );
        // Not one company, price or percentage is written into the card.
        for banned in ["NVIDIA", "206.", "+2.9", "%\"" ] {
            assert!(!out.contains(banned), "{banned:?} must not be baked in:\n{out}");
        }
        assert!(out.contains("AI Movers"), "the editorial title is the model's");
    }

    /// A universe is typed: a company NAME there is rejected before lowering.
    #[test]
    fn a_universe_of_names_is_rejected() {
        let plan = r#"{
            "plan": "stock", "locale": "en",
            "sections": [ { "block": "MoversList",
                            "args": { "symbols": ["Nvidia"] } } ]
        }"#;
        let err = lower(plan).expect_err("a name is not a ticker");
        assert!(err.contains("is not a ticker"), "{err}");
    }

    #[test]
    fn a_company_name_is_not_a_ticker() {
        let bad = r#"{"plan":"stock","locale":"en","sections":[
            {"block":"QuoteHeader","args":{"ticker":"Apple"}}]}"#;
        let err = lower(bad).unwrap_err();
        assert!(err.contains("AAPL"), "must say how to fix it: {err}");
    }

    #[test]
    fn an_unknown_range_is_rejected() {
        let bad = r#"{"plan":"stock","locale":"en","sections":[
            {"block":"PriceChart","args":{"ticker":"AAPL","range":"3h"}}]}"#;
        let err = lower(bad).unwrap_err();
        assert!(err.contains("3h") && err.contains("1mo"), "{err}");
    }

    #[test]
    fn locale_drives_the_furniture() {
        let zh = MOVERS.replace(r#""locale": "en""#, r#""locale": "zh""#);
        let out = lower(&zh).unwrap();
        assert!(out.contains("涨幅榜"));
        assert!(!out.contains("TOP GAINERS"));
    }

    #[test]
    fn no_font_or_colour_can_come_from_the_plan() {
        let out = lower(QUOTE).unwrap();
        assert!(!out.contains("font_family"));
        assert!(!out.contains("crate_resource"));
        assert_eq!(out.lines().filter(|l| l.starts_with("SolidView{")).count(), 1);
    }
}
