//! News plans → Splash DSL.
//!
//! ## What the model may say, and what it may not
//!
//! A headline feed is almost entirely live data: `sys.news(i, "key")` serves title,
//! author, points, comments and url from the Hacker News front page. So the plan
//! carries **no story content at all** — not a title, not a rank, not a byline. There
//! are no fields for them.
//!
//! What is left for the model is genuinely its own: the masthead wording, the section
//! label, how many rows to show, and the language. Those are editorial choices with no
//! correct answer a tool could supply.
//!
//! This is the sharpest domain for the rule, because a hallucinated headline is worse
//! than a wrong coordinate: it is indistinguishable from a real one, it is *quotable*,
//! and a card that invents news is not a bug but a fabrication.
//!
//! ## No selection, deliberately
//!
//! The DSL spec this replaces builds a two-view app — a feed plus a story detail,
//! switched by tapping a row. That needs interactive state, and a state write currently
//! rebuilds the whole card body. A plan-lowered feed is therefore read-only until the
//! in-place binding bridge lands (`docs/CARD-STATE-IDENTITY.md`); the alternative is
//! shipping a card that tears down and rebuilds its scroll view on every tap, losing
//! the reader's position.

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

/// Note what is absent: no `title`/`author`/`points` for any story, and no `url`.
/// Every one of those is a `sys.news` call the runtime writes.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// Masthead wording — the model's editorial voice, e.g. "Top Stories", "头条".
    #[serde(default)]
    pub title: String,
    /// A small label above the feed, e.g. "LATEST".
    #[serde(default)]
    pub label: String,
    /// How many story rows. Presentation, not fact.
    #[serde(default)]
    pub count: Option<u32>,
}

pub fn lower(json: &str) -> Result<String, String> {
    let plan: Plan =
        serde_json::from_str(json).map_err(|e| format!("news plan is not valid: {e}"))?;
    if plan.plan != "news" {
        return Err(format!("expected a news plan, got {:?}", plan.plan));
    }
    if plan.sections.is_empty() {
        return Err("news plan has no sections — name at least a Masthead".to_string());
    }
    let zh = plan.locale.starts_with("zh");

    let mut body = String::new();
    for (i, sec) in plan.sections.iter().enumerate() {
        let s = match sec.block.as_str() {
            "Masthead" => masthead(&sec.args, zh),
            "LeadStory" => lead_story(zh),
            "StoryFeed" => story_feed(&sec.args, zh),
            other => {
                return Err(format!(
                    "sections[{i}]: unknown block {other:?} — permitted: \
                     Masthead, LeadStory, StoryFeed"
                ))
            }
        };
        body.push_str(&s);
        body.push('\n');
    }
    Ok(dark_root("news-app", &body))
}

fn masthead(a: &Args, zh: bool) -> String {
    let eyebrow = if a.label.is_empty() {
        if zh {
            "头条".to_string()
        } else {
            "TOP STORIES".to_string()
        }
    } else {
        a.label.clone()
    };
    let title = if a.title.is_empty() {
        if zh {
            "新闻".to_string()
        } else {
            "News".to_string()
        }
    } else {
        a.title.clone()
    };
    format!(
        "\x20   View{{ width: Fill height: Fit flow: Down\n\
         \x20       {eb}\n\
         \x20       {ti}\n\
         \x20   }}",
        eb = text("TextCaption", &format!("{eyebrow:?}"), theme::ACCENT, ""),
        // `width: Fill`, as the stock title needs for the same reason: `TextHero`
        // is a 76pt role meant for a hero NUMBER, and the masthead beside it is
        // editorial text of any length. "News" fits; "Top Tech News" clipped
        // mid-word on device.
        ti = text("TextHero", &format!("{title:?}"), theme::TEXT, "width: Fill "),
    )
}

/// The top story, given room to breathe. Every field is live.
fn lead_story(zh: bool) -> String {
    let by = if zh { " · 作者 " } else { " · by " };
    let pts = if zh { " 分" } else { " pts" };
    let cmt = if zh { " 评论" } else { " comments" };
    format!(
        "\x20   RoundedView{{ width: Fill height: Fit flow: Down new_batch: true \
         draw_bg.color: {card} draw_bg.border_radius: {r} margin: Inset{{top: {gap}}} \
         padding: Inset{{left: 16 right: 16 top: 14 bottom: 14}} spacing: 8\n\
         \x20       {kick}\n\
         \x20       {title}\n\
         \x20       {meta}\n\
         \x20       {url}\n\
         \x20   }}",
        card = theme::DARK_CARD,
        r = theme::CARD_RADIUS,
        gap = theme::GAP,
        kick = text(
            "TextCaption",
            &format!("{:?}", if zh { "焦点" } else { "LEAD" }),
            theme::ACCENT,
            ""
        ),
        title = text(
            "TextBody",
            "sys.news(0, \"title\")",
            theme::TEXT,
            "width: Fill "
        ),
        meta = text(
            "TextCaption",
            &format!(
                "sys.news(0, \"points\") + {pts:?} + {cmt_sep:?} + sys.news(0, \"comments\") \
                 + {cmt:?} + {by:?} + sys.news(0, \"author\")",
                cmt_sep = " · "
            ),
            theme::TEXT_MUTED,
            ""
        ),
        url = text("TextCaption", "sys.news(0, \"url\")", theme::ACCENT, ""),
    )
}

/// The dense feed. Rows start at index 1 — index 0 is the lead above, and showing it
/// twice reads as a bug.
fn story_feed(a: &Args, zh: bool) -> String {
    let n = a.count.unwrap_or(7).clamp(1, 20) as usize;
    let label = if a.label.is_empty() {
        if zh {
            "最新".to_string()
        } else {
            "LATEST".to_string()
        }
    } else {
        a.label.clone()
    };
    let pts = if zh { " 分" } else { " pts" };
    let mut rows = String::new();
    for r in 1..=n {
        rows.push_str(&format!(
            "\x20       View{{ width: Fill height: Fit flow: Right align: Align{{y: 0.5}} \
             padding: Inset{{top: 10 bottom: 10}}\n\
             \x20           {rank}\n\
             \x20           View{{ width: Fill height: Fit flow: Down spacing: 3\n\
             \x20               {title}\n\
             \x20               {meta}\n\
             \x20           }}\n\
             \x20       }}\n\
             \x20       SolidView{{ width: Fill height: 1 draw_bg.color: {hair} }}\n",
            rank = text(
                "TextRow",
                &format!("{:?}", r.to_string()),
                theme::ACCENT,
                "width: 30 "
            ),
            title = text(
                "TextRow",
                &format!("sys.news({r}, \"title\")"),
                theme::TEXT_SOFT,
                "width: Fill "
            ),
            meta = text(
                "TextCaption",
                &format!(
                    "sys.news({r}, \"points\") + {pts:?} + \" · \" + sys.news({r}, \"author\")"
                ),
                theme::TEXT_MUTED,
                ""
            ),
            hair = theme::HAIRLINE,
        ));
    }
    format!(
        "\x20   {lab}\n\
         \x20   RoundedView{{ width: Fill height: Fit flow: Down new_batch: true \
         draw_bg.color: {row} draw_bg.border_radius: {r} \
         padding: Inset{{left: 14 right: 14 top: 4 bottom: 4}}\n{rows}\x20   }}",
        lab = text(
            "TextCaption",
            &format!("{label:?}"),
            theme::ACCENT,
            &format!("margin: Inset{{top: {} bottom: 6}} ", theme::GAP)
        ),
        row = theme::DARK_ROW,
        r = theme::CARD_RADIUS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"{
        "plan": "news", "locale": "en",
        "sections": [
            { "block": "Masthead", "args": { "title": "Top Stories", "label": "HACKER NEWS" } },
            { "block": "LeadStory" },
            { "block": "StoryFeed", "args": { "count": 7, "label": "LATEST" } }
        ]
    }"#;

    #[test]
    fn every_story_field_is_live() {
        let out = lower(FEED).unwrap();
        // The lead plus seven rows, all bound through sys.news.
        assert!(out.contains("sys.news(0, \"title\")"));
        for r in 1..=7 {
            assert!(
                out.contains(&format!("sys.news({r}, \"title\")")),
                "row {r} title must be live"
            );
        }
        // Row 0 must not appear in the feed as well as the lead.
        assert_eq!(out.matches("sys.news(0, \"title\")").count(), 1);
        assert!(out.contains("// name: news-app"));
    }

    /// A headline the model wrote would be indistinguishable from a real one, so there
    /// is no field for it at all.
    #[test]
    fn a_story_cannot_be_authored_by_the_model() {
        let bad = r#"{"plan":"news","locale":"en","sections":[
            {"block":"LeadStory","args":{"headline":"Apple buys Nintendo"}}]}"#;
        let err = lower(bad).unwrap_err();
        assert!(err.contains("headline"), "unknown field must be named: {err}");
    }

    #[test]
    fn unknown_block_is_rejected_with_the_permitted_set() {
        let bad = r#"{"plan":"news","locale":"en","sections":[{"block":"Podcasts"}]}"#;
        let err = lower(bad).unwrap_err();
        assert!(err.contains("Podcasts") && err.contains("Masthead"), "{err}");
    }

    #[test]
    fn locale_drives_the_furniture() {
        let zh = FEED
            .replace(r#""locale": "en""#, r#""locale": "zh""#)
            .replace(r#""title": "Top Stories""#, r#""title": "头条""#)
            .replace(r#""label": "HACKER NEWS""#, r#""label": "科技新闻""#)
            .replace(r#""label": "LATEST""#, r#""label": "最新""#);
        let out = lower(&zh).unwrap();
        assert!(out.contains("头条"));
        assert!(out.contains("评论"), "meta furniture must localise");
        assert!(!out.contains(" pts"), "no English unit on a zh card");
    }

    #[test]
    fn no_font_or_colour_can_come_from_the_plan() {
        let out = lower(FEED).unwrap();
        assert!(!out.contains("font_family"));
        assert!(!out.contains("crate_resource"));
        // Exactly one top-level node.
        assert_eq!(out.lines().filter(|l| l.starts_with("SolidView{")).count(), 1);
    }
}
