//! Semantic plans → Splash DSL.
//!
//! The generating model emits a small typed PLAN; the runtime builds the card. This
//! module is the runtime half of separating intent from realization.
//!
//! ## Why
//!
//! When the model emits the whole card, the error surface IS the whole card. In one
//! domain that produced six silent failures: 71 invented coordinates per card, a
//! guessed temperature range that flattened every gradient, weekday names off by one
//! in *every* card ever generated, tofu boxes where CJK belonged, a fixed root height
//! that truncated half the card, and a condition word stated for weather the model had
//! never observed. Each was patched by adding a prohibition to the app's `app.md`,
//! which reached 448 lines of which 28 were MUST/NEVER rules — a scar log, not a spec.
//!
//! ## The rule
//!
//! **Anything a tool call can answer belongs to the runtime.** The model supplies only
//! what no tool can: which entity the user meant, what the screen should look like,
//! the user's remembered preferences, and how to compose apps.
//!
//! Enforced in order of preference:
//!
//! 1. The field does not exist and the runtime derives it. A place is a NAME, so a
//!    coordinate is *unexpressible*; the week's extent and the weekday names are not
//!    inputs at all; nor is the weather itself, which comes from `weather_code`.
//! 2. The field is typed, and a violation is REJECTED before lowering — with the
//!    offending field named, so a retry fixes one field instead of regenerating 16 KB.
//! 3. The model is asked to get it right. This is where a 448-line spec was living.
//!
//! `serde(deny_unknown_fields)` throughout: silently dropping a field a card asked for
//! is the failure mode this exists to remove.
//!
//! ## Layout
//!
//! | module | domain | blocks |
//! |---|---|---|
//! | [`weather`] | forecast for a place | CurrentConditions, Forecast, AirQualityField, SunMoon, Details |
//! | [`news`] | a headline feed | Masthead, LeadStory, StoryFeed |
//! | [`stock`] | market movers | MoversList, QuoteHeader, PriceChart, StatGrid |
//!
//! Each domain owns its own schema and lowering, because a block IS the domain — the
//! stocks prototype showed zero block reuse across domains while the *infrastructure*
//! reused entirely. What is shared is this dispatch, the rule above, and the
//! invariants in [`common`].
//!
//! ## What is deliberately NOT here
//!
//! Interactive state. `news` and `stock` both want a selected item, and a state write
//! currently takes the REBUILDING update path (`agent.notify("set")` →
//! `refresh_a2app_templates` → full `set_text`) rather than the in-place one that
//! already exists for `fn tick()` cards. So both lower to a single non-interactive
//! view for now, and the selection bridge is tracked in `docs/CARD-STATE-IDENTITY.md`.
//! Shipping a tappable plan card before that bridge exists would rebuild the tree on
//! every tap — the exact problem `sys.navsecs` was added to avoid for `MapView`.

pub mod common;
pub mod news;
pub mod stock;
pub mod weather;

/// Which domains hand the model a PLAN spec instead of a DSL spec.
///
/// ONE list to revert. `card_splash_body` accepts either fence regardless, so
/// removing a domain here returns it to the DSL path immediately with no other
/// change.
pub const PLAN_DOMAINS: &[&str] = &["weather", "news", "stock"];

pub fn domain_uses_plan(domain: &str) -> bool {
    PLAN_DOMAINS.contains(&domain)
}

/// Lower any plan to Splash DSL, or explain why it cannot be lowered.
///
/// Dispatch is on the plan's own `plan` field rather than on the routed domain, so a
/// plan that claims to be something it is not is rejected instead of being lowered by
/// the wrong builder.
pub fn lower_plan(json: &str) -> Result<String, String> {
    let kind = plan_kind(json)?;
    match kind.as_str() {
        "weather" => weather::lower(json),
        "news" => news::lower(json),
        "stock" => stock::lower(json),
        other => Err(format!(
            "unsupported plan kind {other:?} — expected one of {}",
            PLAN_DOMAINS.join(", ")
        )),
    }
}

/// Read just the `plan` discriminant, so dispatch does not depend on the rest of the
/// document parsing against any particular domain's schema.
fn plan_kind(json: &str) -> Result<String, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("plan is not valid JSON: {e}"))?;
    v.get("plan")
        .and_then(|k| k.as_str())
        .map(str::to_string)
        .ok_or_else(|| "plan is missing its \"plan\" kind field".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_plan_domain_can_lower_a_minimal_plan() {
        // Each domain in PLAN_DOMAINS must actually have a builder — a domain listed
        // there without one would silently fall through to "no card".
        for d in PLAN_DOMAINS {
            let minimal = match *d {
                "weather" => {
                    r#"{"plan":"weather","locale":"en","place":{"query":"Kyoto"},
                        "sections":[{"block":"CurrentConditions"}]}"#
                }
                "news" => {
                    r#"{"plan":"news","locale":"en",
                        "sections":[{"block":"Masthead","args":{"title":"Top Stories"}}]}"#
                }
                "stock" => {
                    r#"{"plan":"stock","locale":"en",
                        "sections":[{"block":"MoversList","args":{"count":3}}]}"#
                }
                other => panic!("PLAN_DOMAINS has {other:?} with no test plan"),
            };
            let out = lower_plan(minimal)
                .unwrap_or_else(|e| panic!("{d} plan must lower, got: {e}"));
            assert!(out.contains("// name:"), "{d} card needs a name line");
        }
    }

    #[test]
    fn a_plan_claiming_an_unknown_kind_is_rejected() {
        let err = lower_plan(r#"{"plan":"pollen","locale":"en"}"#).unwrap_err();
        assert!(err.contains("pollen") && err.contains("weather"), "{err}");
    }

    #[test]
    fn a_plan_with_no_kind_is_rejected() {
        let err = lower_plan(r#"{"locale":"en"}"#).unwrap_err();
        assert!(err.contains("kind"), "{err}");
    }
}
