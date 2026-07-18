//! Post-generation card validation, and the repair prompt it feeds.
//!
//! One-shot generation drops a random structural detail every few rolls (a
//! row's tap overlay, the `// name:` line, a chip's state key), and the
//! app.md "failure conditions" are prose the model can ignore. This makes
//! them EXECUTABLE: the a2app tree ships machine-checkable rules per app
//! (`apps/<domain>/lint.json`, deployed with the memory tree), the completed
//! card is counted against them, and a violating card triggers ONE automatic
//! repair prompt back to the owning app agent (see the turn-complete handler
//! in main.rs). Rules are plain substring counts — no regex, no DSL parsing —
//! so a rule can never crash a card that would have rendered.

use makepad_widgets::*;

/// One executable requirement: `pattern` must occur at least `min` times in
/// the card body. `desc` is echoed into the repair prompt, so write it as an
/// instruction the model can act on.
pub struct LintRule {
    pub desc: String,
    pub pattern: String,
    pub min: usize,
}

/// Load `apps/<domain>/lint.json` from the deployed a2app memory tree.
/// Prefers the profile memory location (the tree the kernel injects), then
/// the `octos-home/a2app` skill zone. `None` — including on desktop, where
/// neither path exists — disables linting entirely; no rules, no repair.
pub fn load_rules(domain: &str) -> Option<Vec<LintRule>> {
    let home = std::env::var("HOME").ok()?;
    let base = std::path::Path::new(&home).join("octos-home");
    let candidates = [
        base.join(".octos/profiles/_main/data/memory/app-cards/apps")
            .join(domain)
            .join("lint.json"),
        base.join("a2app/apps").join(domain).join("lint.json"),
    ];
    // Try EACH candidate and return the first that parses into a non-empty
    // rule set. A malformed preferred file (e.g. a half-written AMA-authored
    // lint) must not disable a valid shipped fallback — parse-then-fall-through
    // rather than picking the first readable file and giving up on it.
    candidates.iter().find_map(|p| {
        let bytes = std::fs::read(p).ok()?;
        let root: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("card lint: unparseable {}: {e}", p.display());
                return None;
            }
        };
        // Parse ALL rules strictly: if ANY rule is malformed (missing/wrong
        // desc/pattern), reject the WHOLE file and fall through to the next
        // candidate — a half-written rule must not silently reduce enforcement
        // while the file still yields a non-empty set. `min` accepts ints and
        // JSON floats (`22.0`); a missing/invalid min defaults to 1.
        let arr = root.get("rules")?.as_array()?;
        let mut rules = Vec::with_capacity(arr.len());
        for r in arr {
            let desc = r.get("desc")?.as_str()?.to_string();
            let pattern = r.get("pattern")?.as_str()?.to_string();
            let min = r
                .get("min")
                .and_then(|m| m.as_f64())
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| n as usize)
                .unwrap_or(1);
            rules.push(LintRule { desc, pattern, min });
        }
        if rules.is_empty() { None } else { Some(rules) }
    })
}

/// Count each rule's pattern in the card body; return the violated rules as
/// repair-prompt lines. Empty = the card passed.
pub fn lint(body: &str, rules: &[LintRule]) -> Vec<String> {
    rules
        .iter()
        .filter_map(|r| {
            let n = body.matches(r.pattern.as_str()).count();
            (n < r.min).then(|| {
                format!("{} (found {} of the required {} `{}`)", r.desc, n, r.min, r.pattern)
            })
        })
        .collect()
}

/// The one-shot repair message. It reuses the agent's own session context and
/// asks for the FULL corrected card (the client replaces cards wholesale, so
/// a diff would be useless).
pub fn repair_prompt(violations: &[String]) -> String {
    let mut out = String::from(
        "REPAIR PASS — the card you just emitted FAILED validation against the app spec. Violations:\n",
    );
    for v in violations {
        out.push_str("- ");
        out.push_str(v);
        out.push('\n');
    }
    out.push_str(
        "\nRe-emit the ENTIRE corrected card now: one ```runsplash block with the same `// name:` line and the same structure otherwise. Fix ONLY the violations; change nothing else.",
    );
    out
}
