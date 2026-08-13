pub use makepad_code_editor;
// Linking the kit activates its `script_mod!` block, which registers
// `mod.widgets.DiagramView`. Without this `pub use`, the DSL can't resolve
// the template below.
pub use makepad_diagram_kit;
pub use makepad_widgets;

mod app;
mod backend;

use makepad_ai::*;
use makepad_widgets::makepad_draw::svg::{
    collect_edges, collect_text_cmds, parse_svg, SvgDocument, SvgEdge, SvgTextAnchor, SvgTextCmd,
};
// `makepad_micro_serde` was used by the dropped flat-file persistence layer;
// W04 will reintroduce it (or `serde_json`) for the SQLite cache.
use makepad_widgets::*;
use octos_app_store::auth::ProfileId;
use octos_app_transport::{
    Capabilities, ProfileId as TransportProfileId, SecretString, StdioSpawn, TransportConfig,
};
use streaming_markdown_kit::{
    streaming_display_with_latex_autowrap_remend, wrap_bare_latex, SanitizeOptions,
};

use crate::backend::OctosUiAgent;

/// Octos profiles supply system prompts server-side, so the client ships an
/// empty placeholder. Replaces aichat's `BackendType::system_prompt` (and the
/// `splash.md` `include_str!`) which baked huge LLM-shaped diagram preambles
/// into the client. See `05-AICHAT-REUSE-MAP.md` "Stuff we drop or replace".
const OCTOS_PLACEHOLDER_SYSTEM_PROMPT: &str = "";

/// The Makepad Splash scripting manual, baked into the client. When "Splash"
/// mode is on, this is prepended to the user's message so the LLM emits a
/// ```runsplash fenced block that the Markdown widget renders as live,
/// clickable UI (see `app_splash_prompt`). Mirrors aichat's
/// `app_generation_session_system_prompt`, but delivered per-message because
/// octos serves system prompts server-side and the protocol carries no
/// client system-prompt field.
// The Splash DSL manual lives in the framework fork, which the documented
// clone layout mandates at `octos-one/aichat` beside `app/` (see
// docs/BUILDING-ANDROID.md § 1). Reference it there directly — a fresh
// clone has no `app/splash.md` copy, so the old relative path failed the
// very first build.
const SPLASH_MANUAL: &str = include_str!("../../../aichat/splash.md");

/// The L0 memory — what an app agent is given instead of the Splash manual.
///
/// 706 lines against the old memory's 5,919, and the difference is not
/// compression: most of that manual taught things L0 does not have. A language
/// with no expression form needs no chapter on building strings, on when a `let`
/// freezes, or on reading a `-9999` sentinel.
const L0_FRAMEWORK: &str = include_str!("../../../a2app-l0/framework.md");
const L0_LANGUAGE: &str = include_str!("../../../a2app-l0/framework/l0.md");
const L0_CATALOG: &str = include_str!("../../../a2app-l0/framework/catalog.md");

/// Per-app: the requirements, and a card that meets them.
///
/// The exemplar is not an illustration — every one reports `valid = true,
/// level = L0` from the same checker that will judge what the model writes.
const L0_APPS: &[(&str, &str, &str)] = &[
    ("weather", include_str!("../../../a2app-l0/apps/weather/app.md"),
     include_str!("../../../a2app-l0/apps/weather/exemplar.card")),
    ("news", include_str!("../../../a2app-l0/apps/news/app.md"),
     include_str!("../../../a2app-l0/apps/news/exemplar.card")),
    ("stock", include_str!("../../../a2app-l0/apps/stock/app.md"),
     include_str!("../../../a2app-l0/apps/stock/exemplar.card")),
    ("activity", include_str!("../../../a2app-l0/apps/activity/app.md"),
     include_str!("../../../a2app-l0/apps/activity/exemplar.card")),
    ("nav", include_str!("../../../a2app-l0/apps/nav/app.md"),
     include_str!("../../../a2app-l0/apps/nav/exemplar.card")),
    ("chart", include_str!("../../../a2app-l0/apps/chart/app.md"),
     include_str!("../../../a2app-l0/apps/chart/exemplar.card")),
    ("youtube", include_str!("../../../a2app-l0/apps/youtube/app.md"),
     include_str!("../../../a2app-l0/apps/youtube/exemplar.card")),
    // Ported from the pre-L0 tree on main. L1, and only just: it computes one
    // value from numbers it already declared. Its keypad did not survive — digit
    // entry is arithmetic in a transition, which L0 has no form for — so the
    // amount comes from the request and three chips adjust it.
    ("convert", include_str!("../../../a2app-l0/apps/convert/app.md"),
     include_str!("../../../a2app-l0/apps/convert/exemplar.card")),
    // Ported from main. A feed card — the shape L0 renders best: it names the
    // source and shows rows, computes nothing, and states no fact.
    ("quake", include_str!("../../../a2app-l0/apps/quake/app.md"),
     include_str!("../../../a2app-l0/apps/quake/exemplar.card")),
    // COMPOSED, and the app that made value guards work. It branches on live
    // SCALARS — `now.precip`, `air.aqi`, `now.temp` — and guards are decided at
    // realize against injected data, where only rows and `sys.gps` used to be. So
    // both halves of every complementary pair were false: on the 6T it drew a
    // correct header, a rain tile reading 100 %, and no verdict under either. It
    // is registered now because `resolve_guards` answers the fields a card GUARDS
    // on before realize, using the same call the tile beside them displays.
    ("weather-activity", include_str!("../../../a2app-l0/apps/weather-activity/app.md"),
     include_str!("../../../a2app-l0/apps/weather-activity/exemplar.card")),
    // COMPOSED, and the first app above L0. `city-picks` compares the user's
    // saved cities, which needs one arithmetic expression — how much warmer it
    // feels than it is — and that is L1. Everything else about it is L0.
    ("city-picks", include_str!("../../../a2app-l0/apps/city-picks/app.md"),
     include_str!("../../../a2app-l0/apps/city-picks/exemplar.card")),
];

/// The prompt for an app that has an L0 spec, or `None` for one that does not.
///
/// `None` falls through to the Splash-DSL path, so an app without an L0 spec
/// keeps working exactly as before. That is what makes this switchable rather
/// than a cutover.
/// The level an app is APPROVED for, derived from its exemplar by the same checker
/// that will judge what the model writes.
///
/// One derivation, two uses: it picks which grammar the prompt states, and it is the
/// ceiling a generated card is held to. §7 says a record needing a wider grammar is
/// rejected until the level is explicitly raised and that escalation is never silent
/// — but the level was derived here for the prompt's sake and then never compared to
/// what came back, so a card that declared `# level: L1` for an L0 app was accepted
/// and drawn. `valid` does not carry it: an L1 card with no diagnostics is valid at
/// L1, which is exactly the case that needed catching.
fn l0_level_for(domain: &str) -> Option<splash_ui_l0::Level> {
    // The same resolution the prompt uses, so a composed app is approved for the
    // level its stand-in exemplar demonstrates rather than for nothing. Approving
    // it for nothing would have let `l0_level_refusal` pass anything through.
    let (_, exemplar) = l0_spec_and_exemplar(domain)?;
    Some(splash_ui_l0::check_ui_l0_named(domain, exemplar).level)
}

/// Is `card` wider than `domain` is approved for? The refusal, if so.
///
/// Reported as a repair reason rather than a render-time error, because this is
/// where a second attempt is still possible — the same place a checker diagnostic
/// goes.
fn l0_level_refusal(domain: &str, report: &splash_ui_l0::UiL0Report) -> Option<String> {
    let approved = l0_level_for(domain)?;
    let rank = |l: splash_ui_l0::Level| match l {
        splash_ui_l0::Level::L0 => 0,
        splash_ui_l0::Level::L1 => 1,
        splash_ui_l0::Level::L2 => 2,
    };
    (rank(report.level) > rank(approved)).then(|| {
        format!(
            "line 1: this card declares level {:?} and `{domain}` is approved for              {approved:?} — remove the `# level:` header and the construct that needed              it (profile §7: escalation is never silent)",
            report.level
        )
    })
}

/// The spec and worked exemplar an L0 prompt is built from.
///
/// A baked app has both. A RUNTIME-COMPOSED app (`<a>-<b>`, written by the AMA
/// into the on-device tree) has only a spec — nobody authored it an exemplar —
/// and without one it fell through to the pre-L0 DSL prompt, so every app the
/// AMA composed came out non-L0 however carefully its spec was written.
///
/// Its PRIMARY PARENT's exemplar stands in. The composed id names its parents,
/// the framework says a composed app inherits the primary parent's identity, and
/// an exemplar is a worked example of the LANGUAGE more than of the app — so
/// weather's card is the right thing to show an agent writing `weather-activity`.
/// Only when that parent is itself an L0 app: otherwise there is nothing to show
/// and the caller should keep its old path.
fn l0_spec_and_exemplar(domain: &str) -> Option<(String, &'static str)> {
    if let Some((_, spec, exemplar)) = L0_APPS.iter().find(|(d, _, _)| *d == domain) {
        return Some(((*spec).to_owned(), exemplar));
    }
    // Composed, and only composed: a bare unknown domain is not one.
    let (primary, _) = domain.split_once('-')?;
    let (_, _, exemplar) = L0_APPS.iter().find(|(d, _, _)| *d == primary)?;
    let spec = app_cards_root_dir()
        .and_then(|r| std::fs::read_to_string(r.join("apps").join(domain).join("app.md")).ok())?;
    Some((spec, exemplar))
}

fn l0_prompt_for(domain: &str, intent: &str) -> Option<String> {
    let (spec, exemplar) = l0_spec_and_exemplar(domain)?;
    // The level comes from the EXEMPLAR, judged by the same checker that will
    // judge what the model writes — not from a fourth list to keep in sync.
    //
    // "There is no arithmetic" is L0's rule. Sending it to an app whose spec
    // asks for one expression tells the model to disobey the spec shipped in
    // the same prompt, and it has no way to know which to believe.
    let level = l0_level_for(domain)?;
    let (level_name, expression_rule) = match level {
        splash_ui_l0::Level::L1 => (
            "L1",
            "This app is L1, so it declares `# level: L1` and may use ONE thing L0 \
cannot: arithmetic over values it already declared. Everything else is L0's — no string \
building, no `if`, no `let`, no functions. An expression must READ something: a \
coefficient is fine, but an expression made only of literals states a fact rather than \
computing one and is REFUSED. There is no grouping and no unary minus, so precedence is \
fixed. Do not reach for L1 anywhere the spec does not ask for it.",
        ),
        _ => (
            "L0",
            "There is no arithmetic, no string building, no `if`, no `let`, no functions. \
Everything you would reach for those with has a declared form.",
        ),
    };
    // The request named a LOOK. A card may not describe one, but it may declare
    // which catalogued mood it is in, and the kit answers that with a palette. Told
    // to the agent explicitly because the alternative — hoping it infers `theme`
    // from a word in the request — fails silently: the card renders in the default
    // and looks entirely correct.
    let theme_hint = match detect_theme(intent) {
        Some(theme) => format!(
            "\n\nThe request asks for the **{theme}** look. Declare `theme {theme}` \
at the top level of the card — that is the ONLY way to ask for it, and it is not a \
colour."
        ),
        None => String::new(),
    };
    Some(format!(
        "You ARE the {domain} app agent. Everything you need is INLINED BELOW — do \
NOT claim anything is missing, do NOT read or fetch files, and do NOT ask questions.\n\n\
Write an {level_name} CARD. It is not a program: it DECLARES what data it needs, what \
state it keeps, what a tap does, and what to show. {expression_rule} Read the language \
reference before writing.\n\n\
NEVER write a fact. Not a temperature, a price, a headline, a venue or a distance. \
Every one comes from a declared `source`. A card with a number typed into it is wrong \
the moment the world changes, and nothing downstream can tell it from a card that is \
right.\n\n\
NEVER write a colour, a font size or a pixel dimension. You say what a thing IS; a \
theme decides what it looks like.{theme_hint}\n\n\
Emit EXACTLY ONE ```runl0 fenced block as your ENTIRE answer — the complete card, no \
prose before or after, never truncated. A card that names anything outside the catalog \
is REFUSED and the reasons are shown instead of your card.\n\n\
===== FRAMEWORK =====\n{L0_FRAMEWORK}\n\
===== LANGUAGE =====\n{L0_LANGUAGE}\n\
===== CATALOG =====\n{L0_CATALOG}\n\
===== REQUIREMENTS: {domain} =====\n{spec}\n\
===== A CARD THAT MEETS THEM =====\n{exemplar}\n\
===== END REFERENCE =====\n\nUser request: {intent}"
    ))
}

/// Build the message actually sent to the LLM in Splash mode: instructions +
/// the Splash manual + the user's request. The chat bubble still shows only
/// the user's original `request` text.
/// Minimal router the app prepends to a splash request. It does NOT carry any
/// generation logic — that lives in the `a2app/` memory the splash-gen sub-agent
/// reads in its own clean context. This is only the spawn TRIGGER, delivered in
/// the message (not the profile system prompt, where `build_system_prompt` buries
/// it under the octos base prompt and the model ignores it).
/// AMA (Activity Management Agent) system prompt. The AMA runs as its OWN
/// session, concurrently with the app agents. Each user intent is BROADCAST to
/// both the AMA and the app agents (fan-out); the AMA classifies which app's
/// domain the intent belongs to. MVP: one app agent (weather), which always
/// takes the screen; the AMA's job is to prove the routing brain runs
/// concurrently (and, later, to prune non-relevant app agents once intent is
/// clear). The AMA renders NOTHING — its output is routing metadata.
const AMA_SYSTEM_PROMPT: &str = "You are the AMA (Activity Management Agent) of an agent OS — a ROUTER and, when needed, an APP COMPOSER. You never generate UI: do NOT emit `runsplash` or any card. Your context includes the APP AGENT MEMORY manual — you do NOT follow its card-generation rules (those are for app agents), but its `framework.md` routing list and its `## Composing a NEW app (AMA composer)` section ARE yours.\n\nROUTING (the default): read the user message, pick the app whose domain it belongs to, and reply EXACTLY ONE short line: `<app-id> — <brief reason>`. The app ids and domains are the routing list in framework.md (weather, stock, news, activity, weather-activity, nav, plus any `apps/<id>/app.md` present in memory). A BARE place name → `weather`; a BARE ticker/company → `stock`; top/best/gainers/movers about the market → `stock`; headlines → `news`; nearby places / things to do → `activity`; COMPARING COUNTRIES on an economic or development measure over time — 'china gdp growth vs india', 'india vs vietnam gdp per capita', 'life expectancy japan korea', 'population of nigeria and brazil', '中国和印度的 GDP' → `chart` (a COUNTRY-level statistic over YEARS, from the World Bank; a company's share price is still `stock` and a city's weather is still `weather`); WHERE-SHOULD-I-GO across the user's SAVED cities — 'where should I go', 'compare my cities', 'which of my cities is nicest', '去哪儿好' → `city-picks` (this one is about the SET the user saved, so it needs no place name; a request naming ONE place is still `weather`, and 'what is nearby' is still `activity`); ASKING WHAT TO DO is `weather-activity`, and it does NOT need the word weather — 'what can I do in Beijing', 'what should I do today', 'anything to do this afternoon', '在北京能做什么', '今天适合干什么' → `weather-activity` (the conditions decide the answer, which is the whole point of the app; the router previously read 'no weather hinge' from the absence of the word and sent these to `activity`, which answers a different question). Asking for a LIST of places is still `activity`: 'what's nearby', 'museums in Beijing', 'parks near me', 'coffee around here'. Advice → `weather-activity`; a list → `activity`. DIRECTIONS / navigation / a route to a place — any go-there request with a travel verb ('directions to SFO', 'navigate home', 'route to the airport', 'how do I get to X', 'map to X', 'show me a map of X', '导航去北京', '怎么去外滩', '去机场怎么走') → `nav` (NOT `weather`: a bare place name stays `weather`, and 'what's nearby / things to do' stays `activity` — `nav` is specifically GOING somewhere). When routing to `nav`, ALSO parse the trip and APPEND `; from=<origin>; to=<destination>` to your decision line — split 'from A to B' (leave `from` empty when no origin is named), and QUALIFY an ambiguous place with its city/region from WORLD KNOWLEDGE so the geocoder resolves it (e.g. 'nvidia headquarters' → 'nvidia santa clara', 'apple park' → 'apple park cupertino', 'googleplex' → 'googleplex mountain view'; leave a clear street address as-is). Example line: `nav — directions; from=Saratoga High School; to=NVIDIA Santa Clara`. ANY video / music / live-stream / watching request (e.g. 'play despacito', 'lofi music', 'watch news live', '放点音乐') → `youtube`; UNIT CONVERSION — 'km to miles', 'how many miles is 42 km', 'kg to lbs', '20°C in fahrenheit', '多少英里' → `convert`; EARTHQUAKES / seismic — 'earthquakes', 'recent quakes', 'any earthquakes today?', '地震' → `quake` (UNITS ONLY: a CURRENCY request — 'usd to eur', '汇率' — needs a live rate this profile has no capability for, so reply `none` rather than routing to a card that would have to invent one); a single general app / tool / utility / game / dashboard that no other domain covers → `web`. A weather request stays `weather` EVEN IF it also names a visual style (`dark`/`light`/`minimal`/`glass`/`vibrant`/`photo`/`深色`/`简约`/`毛玻璃`) — those are STYLE modifiers for the weather card, NOT a `web` app (so `glass weather tokyo`, `dark weather`, `minimal weather shanghai` are ALL `weather`). Never call a clear single-domain request ambiguous. No tools are needed to route.\n\nMECHANICS: you output ONE decision for ONE app, and the system renders ONE card from that ONE app. There is NO 'route each separately' and NO 'two cards' — those actions do not exist. Therefore a request that asks for two domains TOGETHER (combined card, dashboard, X and Y in one view) can ONLY be served by a COMPOSED app: route to the existing composed app that covers the pair, else COMPOSE it now.\n\nCOMPOSING (when NO app in the routing list — composed ones included — covers a MULTI-domain request): follow the composer section in framework.md. Your working directory IS the app-cards `apps/` directory, so use your file tools with RELATIVE paths: write_file `<a>-<b>/app.md` (a requirements spec that MERGES the parent apps' named BLOCKS and binds data ONLY via existing sys.* helpers) and `<a>-<b>/lint.json`, then reply `compose <a>-<b> — <brief reason>`. This authoring write is sanctioned — it is the ONE exception to the manual's never-edit-memory rule. Create a NEW `<id>/` for the composed app; never modify an EXISTING app's files. If your file tools fail, reply `none` and say why.\n\nReply `none` ONLY if no domain's data bears on the message. Be terse; output only the one decision line (after any composing writes).";

/// Findings ferried in by the poll thread, drained on the UI thread.
static DEV_FINDINGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The dev master's charter. Deliverable protocol, not editorial guidance —
/// the whole point of the run is that nobody on the host side authors.
const DEV_MASTER_PROMPT: &str = "You are the on-device DEV MASTER for a self-evolving app experiment. \
You iterate a Splash-DSL catalog screen against MECHANICAL findings only. \
Round protocol: (1) In your FIRST reply, list which of these tools you actually have available: goal_create, peer_handoff, peer_gather, monitor tools — one line, then proceed. If goal tools exist, create a goal for this work and record findings to it; if peer tools exist, you MAY hand the card-writing off to one peer and gather it. (2) EVERY reply must contain the complete revised screen between the exact markers BEGIN_CARD and END_CARD, alone on their own lines. No prose inside the markers. (3) After each reply you will receive a findings message: translate errors, mount diagnostics (built/nodes), ink fraction, and tap-sweep results. Fix what the findings name. (4) When the findings show no errors and the feature works, say DONE before the markers and emit the final card once more. Never invent widgets: only what the embedded contract lists as verified. State slots are global; prefix with sb_.";

const APP_SPLASH_ROUTER: &str = "You ARE the app agent and you OWN the entire card generation. Your COMPLETE memory (the app framework procedure, the widget helpers, and the app specs) is ALREADY IN YOUR CONTEXT — it was injected as your memory. USE it. Do NOT read or fetch any files. Do NOT use the spawn tool. Do NOT delegate. Do NOT summarize.\n\nYou have ALREADY been told which app to build (see the routing line below) — follow THAT app's `apps/<id>/app.md` spec, assembling it from the injected widget patterns (there are no exemplars). It may be weather, stock, news, activity, a composed app (e.g. weather-activity), or any other app whose spec is in your memory — build whichever one you were routed to, using ONLY the sys.* helpers ITS spec names. Bind LIVE data via those helpers — NEVER hardcode or invent numbers/headlines/venues.\n\nWrite the card YOURSELF and stream it as your answer: emit EXACTLY ONE ```runsplash fenced block as your ENTIRE final answer — the COMPLETE card DSL, with ALL mandatory sections the chosen app's spec lists (e.g. for weather: current block, 7-day forecast, BOTH map panes each as its own full-width row — satellite 卫星云图 then air-quality 空气质量图, NEVER side by side — and the detail grid). No prose before or after the block. NEVER truncate — emit the whole card in one block.";

/// Weather card STYLE CHOICES — the exact `.splash` template per style, baked in
/// so a "dark/glass/minimal/photo weather" request
/// reproduces that style precisely without needing the profile MEMORY updated.
/// The default (no style keyword) still uses the injected canonical exemplar.

/// Map a weather request to an explicit style template, if one is named. Bare
/// `dark` is treated as a style keyword (a weather intent never means "is it
/// dark"); `light` is required to be qualified (mode/theme/style/minimal) so it
/// is never confused with "light rain".
/// The theme the request names, as a catalogued L0 mood — or `None`.
///
/// A word like "dark" or "glass" is the user asking for a LOOK, which an L0 card
/// may not describe. It may declare a `theme`, so this maps the words onto that
/// closed set and `l0_prompt_for` asks the agent to declare it. Detected here
/// rather than left to the model because the words are a fixed vocabulary and a
/// missed one is silent: the card renders in the default and looks correct.
fn detect_theme(intent: &str) -> Option<&'static str> {
    let q = intent.to_lowercase();
    let has = |ss: &[&str]| ss.iter().any(|s| q.contains(s));
    let name = if has(&["glass", "vibrant", "gradient", "\u{6bdb}\u{73bb}\u{7483}", "\u{73bb}\u{7483}"]) {
        "glass"
    } else if has(&[
        "minimal", "\u{7b80}\u{7ea6}", "\u{6d45}\u{8272}", "light mode", "light theme", "light style", "clean",
    ]) {
        "light"
    } else if has(&["dark", "\u{6df1}\u{8272}"]) {
        "dark"
    } else if has(&["immersive", "photo", "\u{5927}\u{56fe}"]) {
        "photo"
    } else {
        return None;
    };
    // Never offer a mood the language does not admit: the card would be refused
    // for a word this function chose.
    splash_ui_l0::catalog::theme(name)
}

/// Live channels the youtube agent can offer instantly. (handle, label)
const YOUTUBE_LIVE_CHANNELS: [(&str, &str); 4] = [
    ("LofiGirl", "Lofi Girl lofi radio"),
    ("SkyNews", "Sky News world news"),
    ("aljazeeraenglish", "Al Jazeera English news"),
    ("NASA", "NASA space"),
];

/// Refresh the `live:1` video ids in a composed youtube card.
///
/// Catalog ids are curated by the app agent when the card is generated, and the
/// live ones go stale within days — a stale live id does not degrade to
/// anything watchable, the player just reports "this live stream recording is
/// not available". The card ships an `octos.handles` map (channel name ->
/// youtube handle), which is the same key `youtube_live_cache` is stored under,
/// so the two join without guessing.
fn patch_youtube_live_ids(card: &str) -> String {
    let mut handles: Vec<(String, String)> = Vec::new();
    if let Some(at) = card.find("octos.handles") {
        if let Some(open) = card[at..].find('{') {
            let s = at + open + 1;
            if let Some(close) = card[s..].find('}') {
                for pair in card[s..s + close].split(',') {
                    let mut it = pair.splitn(2, ':');
                    if let (Some(name), Some(handle)) = (it.next(), it.next()) {
                        let name = name.trim().trim_matches('"');
                        let handle = handle.trim().trim_matches('"');
                        if !name.is_empty() && !handle.is_empty() {
                            handles.push((name.to_string(), handle.to_string()));
                        }
                    }
                }
            }
        }
    }

    let cache = youtube_live_cache().lock().unwrap();
    let mut out = String::with_capacity(card.len());
    let mut patched = 0usize;
    for line in card.split_inclusive('\n') {
        let fresh_line = (|| {
            if !line.contains("live:1") {
                return None;
            }
            let ch_at = line.find("ch:\"")? + 4;
            let ch_end = ch_at + line[ch_at..].find('"')?;
            let handle = handles
                .iter()
                .find(|(name, _)| name == &line[ch_at..ch_end])
                .map(|(_, h)| h.as_str())?;
            let fresh = cache.get(handle)?;
            let id_at = line.find("id:\"")? + 4;
            let id_end = id_at + line[id_at..].find('"')?;
            if &line[id_at..id_end] == fresh {
                return None;
            }
            Some(format!("{}{}{}", &line[..id_at], fresh, &line[id_end..]))
        })();
        match fresh_line {
            Some(l) => {
                patched += 1;
                out.push_str(&l);
            }
            None => out.push_str(line),
        }
    }
    log::info!("youtube card: refreshed {patched} live id(s)");
    out
}

/// handle -> current live video id, resolved by the app runtime (ground truth
/// for the youtube agent — memorized live ids in the model are always stale).
fn youtube_live_cache() -> &'static std::sync::Mutex<std::collections::HashMap<&'static str, String>>
{
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<&'static str, String>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Resolve each channel's CURRENT live video id in a background thread by
/// fetching `youtube.com/@handle/live` (through the OCTOS proxy when set) and
/// pulling the first `"videoId":"..."`. Results land in `youtube_live_cache`;
/// the youtube router prompt injects whatever is cached at generation time.
fn refresh_youtube_live_ids() {
    std::thread::spawn(|| {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(_) => return,
        };
        rt.block_on(async {
            let mut builder = reqwest::Client::builder();
            if let Ok(proxy) = std::env::var("MAKEPAD_OCTOS_PROXY") {
                let proxy = proxy.trim().to_owned();
                if !proxy.is_empty() {
                    if let Ok(p) = reqwest::Proxy::all(&proxy) {
                        builder = builder.proxy(p);
                    }
                }
            }
            let Ok(client) = builder
                .user_agent("Mozilla/5.0 (Linux; Android 11) AppleWebKit/537.36")
                .timeout(std::time::Duration::from_secs(8))
                .build()
            else {
                return;
            };
            for (handle, _) in YOUTUBE_LIVE_CHANNELS {
                if youtube_live_cache().lock().unwrap().contains_key(handle) {
                    continue;
                }
                let url = format!("https://www.youtube.com/@{handle}/live");
                let mut body = String::new();
                for attempt in 0..3 {
                    if attempt > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                    }
                    if let Ok(resp) = client.get(&url).send().await {
                        if let Ok(text) = resp.text().await {
                            if text.contains("videoId") {
                                body = text;
                                break;
                            }
                        }
                    }
                }
                if body.is_empty() {
                    continue;
                }
                if let Some(pos) = body.find("\"videoId\":\"") {
                    let start = pos + "\"videoId\":\"".len();
                    if let Some(id) = body.get(start..start + 11) {
                        if id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                        {
                            log::info!("youtube live resolve: @{handle} -> {id}");
                            youtube_live_cache()
                                .lock()
                                .unwrap()
                                .insert(handle, id.to_string());
                        }
                    }
                }
            }
        });
    });
}

/// Condition code → (label, glass tint hex, WeatherIcon cond, photo scene). The
/// glass tint tracks the sky: blue=clear, gray=overcast/fog, slate=rain, cool=snow.
fn wx_cond_meta(cond: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match cond.trim() {
        "0" => ("Clear Sky", "0f3e73", "0.0", "clear blue sky bright sunny"),
        "1" => ("Partly Cloudy", "24425f", "1.0", "partly cloudy blue sky"),
        "2" => ("Overcast", "3f454d", "2.0", "overcast grey clouds"),
        "3" => ("Rain", "213645", "3.0", "rain wet reflective streets"),
        "4" => ("Thunderstorm", "241f38", "4.0", "thunderstorm dramatic dark clouds"),
        "5" => ("Snow", "3f5163", "5.0", "snow winter white"),
        "6" => ("Windy", "2a3f4a", "6.0", "windy dramatic sky"),
        "7" => ("Fog", "40454b", "7.0", "fog mist haze"),
        _ => ("Weather", "1a2b40", "2.0", "skyline"),
    }
}

/// Build a REAL-glass single-city detail card (glass.Panel = gaussian backdrop
/// blur + lensing) over a condition-matched live photo, all numbers live via
/// `sys.weather(lat, lon, …)`. Rendered directly on a list tap — no LLM.
fn glass_detail_card(city: &str, lat: &str, lon: &str, cond: &str) -> String {
    let (label, tint, icon, scene) = wx_cond_meta(cond);
    GLASS_DETAIL_TEMPLATE
        .replace("__CITY__", city)
        .replace("__LAT__", lat)
        .replace("__LON__", lon)
        .replace("__TINT__", tint)
        .replace("__ICON__", icon)
        .replace("__LABEL__", label)
        .replace("__SCENE__", scene)
}

/// Placeholders (`__CITY__ __LAT__ __LON__ __TINT__ __ICON__ __LABEL__ __SCENE__`)
/// are substituted by `glass_detail_card`. `.replace()` (not `format!`) so the
/// DSL's own `{ }` need no escaping. Roboto loaded from the bundled resources.
const GLASS_DETAIL_TEMPLATE: &str = r##"SolidView{ width: Fill height: 940 flow: Overlay new_batch: true draw_bg.color: #05070c
    Image{ src: http_resource(sys.photo("__CITY__ skyline __SCENE__")) fit: ImageFit.CropToFill width: Fill height: Fill }
    View{ width: Fill height: Fill flow: Down padding: Inset{left: 16 top: 56 right: 16 bottom: 40} spacing: 14
        glass.Panel{ width: Fill height: Fit flow: Down new_batch: true padding: Inset{left: 22 top: 20 right: 22 bottom: 18} spacing: 2
            draw_bg +: { tint_color: #x__TINT__ tint_alpha: 0.36 border_color: #xcdd9e6 border_alpha: 0.5 corner_radius: 26.0 highlight_strength: 0.3 }
            Label{ text: "__CITY__" draw_text.color: #ffffff draw_text.text_style: TextStyle{ font_family: FontFamily{ latin := FontMember{ res: crate_resource("makepad_widgets:resources/Roboto-Regular.ttf") asc: 0.0 desc: 0.0 } } font_size: 30 } }
            View{ width: Fill height: 82 flow: Right align: Align{y: 0.5} spacing: 12
                Label{ text: sys.weather(__LAT__, __LON__, "current.temperature_2m") + "°" draw_text.color: #ffffff draw_text.text_style: TextStyle{ font_family: FontFamily{ latin := FontMember{ res: crate_resource("makepad_widgets:resources/Roboto-Thin.ttf") asc: 0.0 desc: 0.0 } } font_size: 62 } }
                View{ width: Fill height: Fit flow: Down spacing: 3
                    Label{ text: "__LABEL__" draw_text.color: #ffffff draw_text.text_style: TextStyle{ font_family: FontFamily{ latin := FontMember{ res: crate_resource("makepad_widgets:resources/Roboto-Medium.ttf") asc: 0.0 desc: 0.0 } } font_size: 16 } }
                    Label{ text: "Feels " + sys.weather(__LAT__, __LON__, "current.apparent_temperature") + "°" draw_text.color: #ffffffcc draw_text.text_style.font_size: 12 }
                }
                WeatherIcon{ draw_bg.cond: __ICON__ width: 44 height: 44 }
            }
            Label{ text: "H:" + sys.weather(__LAT__, __LON__, "daily.temperature_2m_max.0") + "°    L:" + sys.weather(__LAT__, __LON__, "daily.temperature_2m_min.0") + "°" draw_text.color: #ffffffdd draw_text.text_style.font_size: 13 margin: Inset{top: 4} }
        }
        glass.Panel{ width: Fill height: Fit flow: Right new_batch: true padding: Inset{left: 8 top: 14 right: 8 bottom: 14}
            draw_bg +: { tint_color: #x__TINT__ tint_alpha: 0.32 border_color: #xcdd9e6 border_alpha: 0.45 corner_radius: 24.0 }
            View{ width: Fill height: Fit flow: Down align: Align{x: 0.5} spacing: 3
                Label{ text: "HUMIDITY" draw_text.color: #ffffffaa draw_text.text_style.font_size: 10 }
                Label{ text: sys.weather(__LAT__, __LON__, "current.relative_humidity_2m") + "%" draw_text.color: #ffffff draw_text.text_style: TextStyle{ font_family: FontFamily{ latin := FontMember{ res: crate_resource("makepad_widgets:resources/Roboto-Medium.ttf") asc: 0.0 desc: 0.0 } } font_size: 19 } }
            }
            View{ width: Fill height: Fit flow: Down align: Align{x: 0.5} spacing: 3
                Label{ text: "WIND" draw_text.color: #ffffffaa draw_text.text_style.font_size: 10 }
                Label{ text: sys.weather(__LAT__, __LON__, "current.wind_speed_10m") draw_text.color: #ffffff draw_text.text_style: TextStyle{ font_family: FontFamily{ latin := FontMember{ res: crate_resource("makepad_widgets:resources/Roboto-Medium.ttf") asc: 0.0 desc: 0.0 } } font_size: 19 } }
            }
            View{ width: Fill height: Fit flow: Down align: Align{x: 0.5} spacing: 3
                Label{ text: "UV" draw_text.color: #ffffffaa draw_text.text_style.font_size: 10 }
                Label{ text: sys.weather(__LAT__, __LON__, "daily.uv_index_max.0") draw_text.color: #ffffff draw_text.text_style: TextStyle{ font_family: FontFamily{ latin := FontMember{ res: crate_resource("makepad_widgets:resources/Roboto-Medium.ttf") asc: 0.0 desc: 0.0 } } font_size: 19 } }
            }
        }
        glass.Panel{ width: Fill height: Fit flow: Down new_batch: true padding: Inset{left: 12 top: 12 right: 12 bottom: 14} spacing: 8
            draw_bg +: { tint_color: #x__TINT__ tint_alpha: 0.32 border_color: #xcdd9e6 border_alpha: 0.45 corner_radius: 24.0 }
            Label{ text: "7-DAY FORECAST" draw_text.color: #ffffffaa draw_text.text_style.font_size: 10 margin: Inset{left: 4} }
            View{ width: Fill height: Fit flow: Right spacing: 4
                View{ width: Fill height: Fit flow: Down align: Align{x: 0.5} spacing: 6
                    Label{ text: "Today" draw_text.color: #ffffffcc draw_text.text_style.font_size: 11 }
                    WeatherIcon{ draw_bg.cond: __ICON__ width: 28 height: 28 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_max.0") + "°" draw_text.color: #ffffff draw_text.text_style.font_size: 13 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_min.0") + "°" draw_text.color: #ffffff99 draw_text.text_style.font_size: 12 }
                }
                View{ width: Fill height: Fit flow: Down align: Align{x: 0.5} spacing: 6
                    Label{ text: "Sun" draw_text.color: #ffffffcc draw_text.text_style.font_size: 11 }
                    WeatherIcon{ draw_bg.cond: __ICON__ width: 28 height: 28 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_max.1") + "°" draw_text.color: #ffffff draw_text.text_style.font_size: 13 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_min.1") + "°" draw_text.color: #ffffff99 draw_text.text_style.font_size: 12 }
                }
                View{ width: Fill height: Fit flow: Down align: Align{x: 0.5} spacing: 6
                    Label{ text: "Mon" draw_text.color: #ffffffcc draw_text.text_style.font_size: 11 }
                    WeatherIcon{ draw_bg.cond: __ICON__ width: 28 height: 28 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_max.2") + "°" draw_text.color: #ffffff draw_text.text_style.font_size: 13 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_min.2") + "°" draw_text.color: #ffffff99 draw_text.text_style.font_size: 12 }
                }
                View{ width: Fill height: Fit flow: Down align: Align{x: 0.5} spacing: 6
                    Label{ text: "Tue" draw_text.color: #ffffffcc draw_text.text_style.font_size: 11 }
                    WeatherIcon{ draw_bg.cond: __ICON__ width: 28 height: 28 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_max.3") + "°" draw_text.color: #ffffff draw_text.text_style.font_size: 13 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_min.3") + "°" draw_text.color: #ffffff99 draw_text.text_style.font_size: 12 }
                }
                View{ width: Fill height: Fit flow: Down align: Align{x: 0.5} spacing: 6
                    Label{ text: "Wed" draw_text.color: #ffffffcc draw_text.text_style.font_size: 11 }
                    WeatherIcon{ draw_bg.cond: __ICON__ width: 28 height: 28 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_max.4") + "°" draw_text.color: #ffffff draw_text.text_style.font_size: 13 }
                    Label{ text: sys.weather(__LAT__, __LON__, "daily.temperature_2m_min.4") + "°" draw_text.color: #ffffff99 draw_text.text_style.font_size: 12 }
                }
            }
        }
    }
}"##;

/// The domain-specialised app-agent prompt. The AMA routed `intent` to `domain`,
/// so tell THAT agent to generate a card of exactly that app type (following the
/// matching `apps/<domain>/app.md` spec in its injected memory).
/// Deliberately generic over ANY id — dynamically composed apps (`compose_app`)
/// reuse it unchanged: the fresh session's injected memory carries the
/// AMA-authored `apps/<domain>/app.md`, which this prompt points the agent at.
/// The complete hand-authored YouTube app card (home / watch / search / library,
/// composing the `octos.media` kit). Served DIRECTLY on a youtube route so the
/// full app renders reliably — the on-device model under-generates a 14 KB app
/// down to a bare player, so youtube is a deterministic card, not a generation.
const YOUTUBE_REFERENCE_CARD: &str = include_str!("../../../docs/youtube-player-reference.html");

/// The complete Google-Maps-style trip-planner card (search → preview → plan →
/// turn-by-turn drive over the native 2.5D `MapView`). Served DIRECTLY on a
/// `nav` route — same rationale as [`YOUTUBE_REFERENCE_CARD`]: the on-device
/// model under-generates / truncates this ~14 KB card when asked to re-emit it,
/// so nav is a deterministic served card, not a generation. This is the SAME
/// file embedded in `apps/nav/app.md` (the contract/reference); the drift-guard
/// test keeps them identical. Interactivity survives the direct-serve because
/// the render pipeline tags `agent.notify` calls and substitutes `{{state.*}}`
/// by the card's slot id, independent of whether an LLM produced the body.
const NAV_CANONICAL_CARD: &str =
    include_str!("../../../a2app/apps/nav/exemplars/trip-planner.splash");

/// Pull the destination place out of a natural-language nav intent so the served
/// nav card can open straight on it (intent-based navigation) instead of an
/// empty search box: "directions to SFO" -> "SFO", "how do I get to the Ferry
/// Building from SOMA" -> "the Ferry Building", "show me a map of Golden Gate
/// Bridge" -> "Golden Gate Bridge", "导航去北京南站" -> "北京南站", "去外滩怎么走"
/// -> "外滩". Best-effort: returns None when nothing clear is named (the card
/// then opens on its search box). An origin clause ("… from <place>") is dropped
/// — only the destination is seeded; the origin default stays San Jose.
fn extract_nav_destination(intent: &str) -> Option<String> {
    let s = intent.trim();
    let lower = s.to_lowercase();
    let mut raw: Option<String> = None;
    // English: the destination follows the LAST " to " (covers "from A to B",
    // "get to X", "navigate to X"); a trailing "from <origin>" clause is cut.
    if let Some(pos) = lower.rfind(" to ") {
        let after = s[pos + 4..].trim();
        let after = match after.to_lowercase().find(" from ") {
            Some(f) => after[..f].trim(),
            None => after,
        };
        if !after.is_empty() {
            raw = Some(after.to_string());
        }
    }
    // "map of X" / "picture of X" style.
    if raw.is_none() {
        if let Some(pos) = lower.rfind(" of ") {
            let after = s[pos + 4..].trim();
            if !after.is_empty() {
                raw = Some(after.to_string());
            }
        }
    }
    // Chinese: the destination follows the LAST 去 or 到.
    if raw.is_none() {
        if let Some((idx, ch)) = s.char_indices().rev().find(|(_, c)| *c == '去' || *c == '到') {
            let after = s[idx + ch.len_utf8()..].trim();
            if !after.is_empty() {
                raw = Some(after.to_string());
            }
        }
    }
    let mut d = raw?;
    // Strip trailing qualifiers / politeness / punctuation until stable.
    const TRAIL: &[&str] = &[
        "怎么走", "怎么去", "怎么到", "路线", "导航", "，", "。", "！", "？", ",", ".", "!", "?",
    ];
    loop {
        let before = d.clone();
        d = d.trim().to_string();
        for suf in TRAIL {
            if let Some(p) = d.strip_suffix(suf) {
                d = p.trim().to_string();
            }
        }
        for suf in ["please", "thanks", "thank you"] {
            if d.to_lowercase().ends_with(suf) {
                d = d[..d.len() - suf.len()].trim().to_string();
            }
        }
        if d == before {
            break;
        }
    }
    if d.is_empty() {
        None
    } else {
        Some(d)
    }
}

/// Parse the origin/destination the AMA appended to a `nav` decision line as
/// `from=<origin>; to=<destination>` (see AMA_SYSTEM_PROMPT — the AMA splits
/// "from A to B" and qualifies ambiguous places with world knowledge). Either
/// may be absent (no origin stated). Each value runs to the next `;`/`|`/end and
/// tolerates stray quotes. Returns (origin, destination), each trimmed non-empty.
fn parse_nav_places(decision: &str) -> (Option<String>, Option<String>) {
    fn field(s: &str, key: &str) -> Option<String> {
        let i = s.find(key)?;
        let r = &s[i + key.len()..];
        let end = r.find([';', '|', '\n']).unwrap_or(r.len());
        let v = r[..end]
            .trim()
            .trim_matches(['"', '\'', '\u{201c}', '\u{201d}'])
            .trim();
        if v.is_empty() || v == "0" {
            None
        } else {
            Some(v.to_string())
        }
    }
    (field(decision, "from="), field(decision, "to="))
}

/// Google OAuth (device-code) client for YouTube sign-in, injected at BUILD time from
/// env vars so the client id/secret never live in committed source. Build with
/// `OCTOS_GOOGLE_CLIENT_ID=… OCTOS_GOOGLE_CLIENT_SECRET=… cargo makepad android build …`.
/// Unset → the placeholders stay and the card disables sign-in gracefully.
/// Seed plans for `OCTOS_SEED_CARD=aimovers|shanghai` — the two live queries,
/// as the generating model would emit them.
const SEED_PLAN_AI_MOVERS: &str = r#"{
    "plan": "stock", "locale": "en",
    "sections": [ { "block": "MoversList", "args": {
        "count": 10, "title": "AI Movers", "label": "AI · TOP MOVERS AND SHAKERS",
        "symbols": ["NVDA","AMD","AVGO","SMCI","MU","TSM","MRVL","ARM","CRWV",
                    "PLTR","SNOW","AI","VRT","ANET","ORCL","MSFT","GOOGL","META"]
    } } ]
}"#;

const SEED_PLAN_SHANGHAI: &str = r#"{
    "plan": "weather", "locale": "en",
    "place": { "query": "Shanghai" },
    "photo": "shanghai bund skyline summer",
    "sections": [
        { "block": "CurrentConditions" },
        { "block": "Forecast", "args": { "days": 7 } },
        { "block": "Details", "args": { "tiles": ["aqi","uv","humidity","wind"] } },
        { "block": "Attractions", "args": { "places":
            ["The Bund","Yu Garden","Tianzifang","Longhua Temple","Zhujiajiao"] } }
    ]
}"#;

const GOOGLE_CLIENT_ID: Option<&str> = option_env!("OCTOS_GOOGLE_CLIENT_ID");
const GOOGLE_CLIENT_SECRET: Option<&str> = option_env!("OCTOS_GOOGLE_CLIENT_SECRET");

/// (live-channel handle, the video id it occupies in the reference card). The
/// freshest ids from `youtube_live_cache` replace these so the card always opens
/// on a currently-live stream (live ids rotate).
const YOUTUBE_REF_PLACEHOLDER_IDS: [(&str, &str); 4] = [
    ("LofiGirl", "VAlMDl00mYY"),
    ("SkyNews", "YDvsBbKfLPA"),
    ("aljazeeraenglish", "gCNeDWCI0vo"),
    ("NASA", "awQzjn72bI0"),
];

/// The reference youtube card with the freshest resolved live ids substituted in.
fn youtube_reference_card() -> String {
    let cache = youtube_live_cache().lock().unwrap();
    let mut html = YOUTUBE_REFERENCE_CARD.to_string();
    for (handle, placeholder) in YOUTUBE_REF_PLACEHOLDER_IDS {
        if let Some(fresh) = cache.get(handle) {
            if fresh.len() == 11 && fresh.as_str() != placeholder {
                html = html.replace(placeholder, fresh);
            }
        }
    }
    // Build-time Google OAuth creds (empty when unset → card disables sign-in).
    html = html.replace("__GOOGLE_CLIENT_ID__", GOOGLE_CLIENT_ID.unwrap_or(""));
    html = html.replace("__GOOGLE_CLIENT_SECRET__", GOOGLE_CLIENT_SECRET.unwrap_or(""));
    html
}

/// Root of the deployed app-cards tree on device. The current octos main this
/// branch builds against no longer assembles/injects app-cards as agent memory,
/// so the app reads the routed app's spec + shared widget docs from here and
/// INLINES them into the generation prompt (`app_card_docs` + `splash_gen_prompt`)
/// — the same self-contained pattern the youtube/weather-style paths already use.
#[cfg(target_os = "android")]
const APP_CARDS_ROOT: &str = "/data/user/0/dev.makepad.octos_app/files/octos-home/.octos/profiles/_main/data/memory/app-cards";

fn app_cards_root_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "android")]
    {
        Some(std::path::PathBuf::from(APP_CARDS_ROOT))
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::var("OCTOS_APP_CARDS_DIR")
            .ok()
            .map(std::path::PathBuf::from)
    }
}

/// The shared widget doc `name`, compiled into the binary. This is the SAME
/// file that gets deployed to the on-device app-cards tree — baked in so a plain
/// `git clone → build → install` renders cards even when the on-device
/// app-cards dir was never provisioned (nothing in the normal build deploys it
/// there). Returns None for an unknown widget name.
fn baked_widget_md(name: &str) -> Option<&'static str> {
    Some(match name {
        "design-system" => include_str!("../../../a2app/widgets/design-system.md"),
        "containers" => include_str!("../../../a2app/widgets/containers.md"),
        "interaction" => include_str!("../../../a2app/widgets/interaction.md"),
        "sys-helpers" => include_str!("../../../a2app/widgets/sys-helpers.md"),
        "weather-icon" => include_str!("../../../a2app/widgets/weather-icon.md"),
        _ => return None,
    })
}

/// A built-in app's `app.md` spec, compiled into the binary — the baked-in
/// fallback for [`app_card_docs`] (see [`baked_widget_md`]). Covers every domain
/// the AMA routes to; runtime-composed apps (`<a>-<b>`) live only on-device, so
/// they have no baked copy and rely on the deployed tree.
use crate::app::plan::{domain_uses_plan, PLAN_DOMAINS};

fn baked_app_md(domain: &str) -> Option<&'static str> {
    Some(match domain {
        "weather" => {
            if domain_uses_plan("weather") {
                include_str!("../../../a2app/apps/weather/plan.md")
            } else {
                include_str!("../../../a2app/apps/weather/app.md")
            }
        }
        "stock" => {
            if domain_uses_plan("stock") {
                include_str!("../../../a2app/apps/stock/plan.md")
            } else {
                include_str!("../../../a2app/apps/stock/app.md")
            }
        }
        "news" => {
            if domain_uses_plan("news") {
                include_str!("../../../a2app/apps/news/plan.md")
            } else {
                include_str!("../../../a2app/apps/news/app.md")
            }
        }
        "activity" => include_str!("../../../a2app/apps/activity/app.md"),
        // The L0 spec, for the reason youtube's is below: this app is in
        // `L0_APPS` now, so the DSL branch under it is unreachable and a second
        // spec here could only drift out of agreement with the one that runs.
        "weather-activity" => include_str!("../../../a2app-l0/apps/weather-activity/app.md"),
        "nav" => include_str!("../../../a2app/apps/nav/app.md"),
        "web" => include_str!("../../../a2app/apps/web/app.md"),
        // The L0 spec, not `a2app/apps/youtube/app.md`. The old one is the HTML
        // contract that asks the agent for video ids; handing it out here would
        // put the two youtube specs one fallback apart and disagreeing.
        "youtube" => include_str!("../../../a2app-l0/apps/youtube/app.md"),
        _ => return None,
    })
}

/// Read the docs an app agent needs to generate a `domain` card — the shared
/// widget pattern docs plus the routed app's `apps/<domain>/app.md` spec,
/// formatted for inlining into the prompt. Each doc is taken from the DEPLOYED
/// on-device app-cards tree when present (so a device can override with newer
/// specs), otherwise from the copy baked into the binary
/// (`baked_widget_md`/`baked_app_md`) — so a plain build+install works with an
/// empty on-device app-cards dir. The spec goes LAST so it's the freshest
/// context. Empty string only for a runtime-composed `domain` with nothing
/// deployed (caller then falls back to the older memory-reliant prompt).
fn app_card_docs(domain: &str) -> String {
    let root = app_cards_root_dir();
    let mut out = String::new();
    for w in [
        "design-system",
        "containers",
        "interaction",
        "sys-helpers",
        "weather-icon",
    ] {
        let body = root
            .as_ref()
            .and_then(|r| std::fs::read_to_string(r.join("widgets").join(format!("{w}.md"))).ok())
            .or_else(|| baked_widget_md(w).map(|s| s.to_string()));
        if let Some(s) = body {
            out.push_str(&format!("\n----- widgets/{w}.md -----\n{}\n", s.trim_end()));
        }
    }
    let app_md = root
        .as_ref()
        .and_then(|r| std::fs::read_to_string(r.join("apps").join(domain).join("app.md")).ok())
        .or_else(|| baked_app_md(domain).map(|s| s.to_string()));
    if let Some(s) = app_md {
        out.push_str(&format!(
            "\n----- apps/{domain}/app.md — THIS IS YOUR SPEC, follow it EXACTLY -----\n{}\n",
            s.trim_end()
        ));
    }
    out
}

/// Assemble the SELF-CONTAINED Splash generation prompt for `domain`: the baked
/// syntax manual + the inlined app-cards `docs` + the output contract. Pure
/// (`docs` passed in) so the assembly is unit-testable off-device. The explicit
/// "only real Splash syntax" clause targets the observed GLM-5.2 failure mode of
/// inventing `Card {}` / `layout: {}` / `background:` pseudo-DSL.
fn splash_gen_prompt(domain: &str, intent: &str, docs: &str) -> String {
    if domain_uses_plan(domain) {
        // A PLAN domain: the model emits typed intent, not a card. Almost all of
        // the DSL prompt's warnings are about syntax it can no longer write, so
        // they are omitted rather than left to confuse it.
        return format!(
            "You ARE the {domain} app agent. Your PLAN SPEC is INLINED BELOW — you \
have everything you need, so do NOT claim anything is missing, do NOT read or fetch \
files, and do NOT ask questions.\n\n\
You do NOT write the card. You choose WHAT IT SHOWS and the runtime builds it. Emit \
EXACTLY ONE ```runplan fenced block containing the JSON the spec describes, as your \
ENTIRE final answer — no prose before or after, never truncated.\n\n\
Only the fields the spec lists exist. Anything absent from the schema — a \
coordinate, a temperature, a font, a colour, a size — is supplied by the runtime, \
and a plan carrying one is REJECTED with the offending field named. Choose the \
place, the condition words, the sections and the locale; nothing else.\n\
{docs}\n\nUser request: {intent}"
        );
    }
    // An app with an L0 spec is generated as an L0 card. One without falls
    // through to the Splash-DSL path below and keeps working unchanged — which
    // is what makes this switchable per app rather than a cutover.
    if let Some(l0) = l0_prompt_for(domain, intent) {
        return l0;
    }

    format!(
        "You ARE the {domain} app agent and you OWN the entire card generation. Your \
SPEC and the widget patterns are INLINED BELOW — you have everything you need, so \
do NOT claim anything is missing, do NOT read or fetch files, and do NOT ask \
questions. Follow the `apps/{domain}/app.md` spec EXACTLY, assembling it from the \
widget patterns and the SYNTAX MANUAL, using ONLY the `sys.*` helpers the spec \
names and binding live data through them (NEVER hardcode or invent numbers, \
headlines, or venues). Use ONLY real Makepad Splash syntax from the manual — real \
widgets (`SolidView`/`View`/`RoundedView`/`Label`/`Image`/…) with inline \
attributes. NEVER invent syntax such as `Card {{ }}`, `layout: {{ }}`, or \
`background:` — those are not Splash and render blank.\n\n\
Emit EXACTLY ONE ```runsplash fenced block as your ENTIRE final answer — the \
COMPLETE card DSL with ALL mandatory sections the spec lists, no prose before or \
after, never truncated.\n\n\
===== SPLASH SYNTAX MANUAL =====\n{SPLASH_MANUAL}\n{docs}\n===== END REFERENCE =====\
\n\nThe AMA routed this request to the {domain} app.\n\nUser request: {intent}"
    )
}

fn app_splash_router_for(domain: &str, intent: &str) -> String {
    // `youtube` USED to be overridden here with the old HTML contract: it told
    // the agent to `web_fetch` a channel's live page, dig an 11-char videoId out
    // of the markup, and emit a `runhtml` document. That override survived the L0
    // refactor and short-circuited the L0 branch below, so the youtube app ran on
    // its predecessor's prompt. Measured on the 6T with "top trump videos": the
    // agent spent the whole turn narrating a hunt for ids it could verify — "Both
    // 404 — dead ids", "Let me add 2 more famous ones to round out the catalog" —
    // and the screen never showed a video. Ids are facts (§4), which is exactly
    // why the L0 spec says "You are NOT the search engine. Never write a video id
    // into a card." Removed, so youtube reaches `l0_prompt_for` like every other
    // app and its card DECLARES `sys.video(query: state.q, …)`.
    // A styled weather request USED to return a hand-authored Splash-DSL template
    // from here, ahead of the L0 branch — so "dark weather tokyo" was served the
    // pre-L0 path. Measured on the 6T: it emitted `runsplash`, took ~3.5 minutes
    // against ~1 for an L0 card, and the log said `runsplash card has no // name:
    // line — not saved`, so it did not survive a restart either.
    //
    // Style is a THEME the card declares now — `theme dark` — which the component
    // kit answers with a palette (§1.1's middle layer). That keeps §4 intact: the
    // card names a mood from a closed catalogue and still never names a colour,
    // and it means one code path renders every weather card.
    if domain == "web" {
        return format!(
            "You ARE the web app agent and you OWN the entire card generation. Your \
memory contains the apps/web/app.md CONTRACT — follow it exactly. Build the app the \
user asked for as ONE complete, self-contained HTML document (inline <style> and \
<script>, <meta charset=\"utf-8\">, dark theme, 54px top padding) and stream it as \
your answer: emit EXACTLY ONE ```runhtml fenced block as your ENTIRE final answer — \
the COMPLETE document, no prose before or after, never truncated. First line inside \
the block: <!-- name: <short-kebab-slug> -->. Bind live data with fetch() on keyless \
JSON APIs and NEVER hardcode live values; for media use the documented embed \
patterns (e.g. the YouTube iframe with autoplay+playsinline+mute).\n\nUser \
request: {intent}"
        );
    }
    // Default (weather-no-style, stock, news, activity, weather-activity, …).
    // octos no longer injects the app-cards tree as memory, so inline the routed
    // app's spec + the widget/syntax docs directly (self-contained prompt). Fall
    // back to the old memory-reliant prompt only if the tree isn't deployed.
    let docs = app_card_docs(domain);
    if docs.is_empty() {
        format!(
            "{APP_SPLASH_ROUTER}\n\nThe AMA routed this request to the {domain} app — \
generate a {domain} card: follow the apps/{domain}/app.md spec in \
your memory, and bind live data with the matching sys.* helper. Do NOT generate any \
other app type.\n\nUser request: {intent}"
        )
    } else {
        splash_gen_prompt(domain, intent, &docs)
    }
}

fn app_splash_prompt(request: &str) -> String {
    format!(
        "You are a UI-generation agent. Respond with EXACTLY ONE ```runsplash \
fenced code block containing Makepad Splash syntax — no prose before, \
between, or after it, and no other fenced blocks.\n\n\
Hard rules:\n\
- `use mod.prelude.widgets.*` is auto-prepended; do NOT write imports.\n\
- NAME the card: the FIRST line inside the block is `// name: <short-kebab-slug>` \
(a unique, descriptive, STABLE id — e.g. `weather-sf`, `stocks-watchlist`). It is \
stripped before rendering. If you are refining a card from YOUR SAVED CARDS below, \
REUSE its exact same name.\n\
- Do NOT wrap output in Root{{}} or Window{{}}; it is inserted into an \
existing container.\n\
- Interactivity + state: each card has its OWN independent state (keys you \
choose). Read a value with `{{{{state.<key>}}}}` inside a string; change it \
from a button. Events: `inc`/`dec`/`reset` adjust a NUMERIC key, `set` stores a \
string. The payload names the key (default key is `count`):\n\
    Button{{ text: \"+1\" on_click: || agent.notify(\"inc\", {{key: \"count\"}}) }}\n\
    Label{{ text: \"Count: {{{{state.count}}}}\" }}\n\
    Button{{ text: \"Happy\" on_click: || agent.notify(\"set\", {{key: \"mood\", value: \"happy\"}}) }}\n\
- Internet images: fetch a remote picture with `http_resource` in an Image \
widget (downloads asynchronously, appears when ready). Use a real, \
publicly-reachable HTTPS URL (png/jpg/webp/svg):\n\
    Image{{ src: http_resource(\"https://picsum.photos/400/240\") fit: ImageFit.Smallest width: Fill height: 180 }}\n\
  For a REFRESHABLE image, bake the base URL literally and vary ONLY a \
cache-buster query param bound to a counter, plus a button that increments it \
— each tap loads a new picture (never put `{{{{state.*}}}}` as the WHOLE url):\n\
    Image{{ src: http_resource(\"https://picsum.photos/400/240?sig={{{{state.count}}}}\") fit: ImageFit.Smallest width: Fill height: 180 }}\n\
    Button{{ text: \"New Photo\" on_click: || agent.notify(\"inc\", {{}}) }}\n\
- IMMERSIVE FULL-SCREEN iOS WEATHER CARD (the DEFAULT for weather): a REAL photo of \
the city fills the whole screen; the CURRENT conditions sit at the top, a translucent \
7-DAY FORECAST panel sits directly below them, then TWO FULL-WIDTH MAP PANES stacked \
vertically — first a LIVE 卫星云图 (real satellite cloud imagery), then a LIVE 空气质量图 \
(air-quality map) — each on its own row so the maps read large, then a frosted \
6-TILE DETAIL GRID (air quality, UV, sunrise, sunset, humidity, wind) — like a refined iOS \
Weather app. Reproduce this EXACT structure (a full-screen Overlay: photo, dark scrim, \
then a Down column = current block, the 7-day forecast, the two map panes, then the detail \
grid), substituting real, plausible data:\n\
    SolidView{{ width: Fill height: 1500 flow: Overlay new_batch: true draw_bg.color: #000000\n\
        Image{{ src: http_resource(sys.photo(\"tokyo skyline clear sky\")) fit: ImageFit.CropToFill width: Fill height: Fill }}\n\
        GradientYView{{ width: Fill height: Fill new_batch: true draw_bg.color: #00000022 draw_bg.color_2: #000000EE }}\n\
        View{{ width: Fill height: Fill flow: Down padding: Inset{{left: 22 top: 6 right: 22 bottom: 8}} spacing: 2\n\
            Label{{ text: \"Tokyo\" draw_text.color: #ffffff draw_text.text_style.font_size: 30 }}\n\
            Label{{ text: \"72°\" draw_text.color: #ffffff draw_text.text_style.font_size: 50 margin: Inset{{top: 2 bottom: 0}} }}\n\
            View{{ width: Fill height: 60 flow: Right align: Align{{y: 0.5}} spacing: 10\n\
                WeatherIcon{{ draw_bg.cond: 0.0 width: 60 height: 60 }}\n\
                Label{{ text: \"Sunny\" draw_text.color: #ffffff draw_text.text_style.font_size: 20 }}\n\
            }}\n\
            Label{{ text: \"H:78°   L:64°   Feels 74°\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 14 }}\n\
            RoundedView{{ width: Fill height: Fit flow: Down spacing: 0 new_batch: true padding: Inset{{left: 16 top: 2 right: 16 bottom: 2}} draw_bg.color: #00000055 draw_bg.border_radius: 20.0\n\
                SolidView{{ width: Fill height: 40 flow: Right align: Align{{y: 0.5}} new_batch: true padding: Inset{{top: 0 bottom: 0}} draw_bg.color: #00000000\n\
                    Label{{ width: 92 text: \"Today\" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }}\n\
                    Label{{ width: 34 text: \"☀️\" draw_text.text_style.font_size: 14 }}\n\
                    Filler{{}}\n\
                    Label{{ text: \"64°\" draw_text.color: #ffffff88 draw_text.text_style.font_size: 14 }}\n\
                    Label{{ width: 48 text: \"78°\" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }}\n\
                }}\n\
                SolidView{{ width: Fill height: 40 flow: Right align: Align{{y: 0.5}} new_batch: true padding: Inset{{top: 0 bottom: 0}} draw_bg.color: #00000000\n\
                    Label{{ width: 92 text: \"Mon\" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }}\n\
                    Label{{ width: 34 text: \"⛅\" draw_text.text_style.font_size: 14 }}\n\
                    Filler{{}}\n\
                    Label{{ text: \"61°\" draw_text.color: #ffffff88 draw_text.text_style.font_size: 14 }}\n\
                    Label{{ width: 48 text: \"75°\" draw_text.color: #ffffff draw_text.text_style.font_size: 14 }}\n\
                }}\n\
                // …repeat that SolidView row for 7 DAYS total (Today, then the next six \
day names Tue Wed Thu Fri Sat Sun), each with its own weather emoji and lo/hi.\n\
            }}\n\
            RoundedView{{ width: Fill height: Fit flow: Down spacing: 3 new_batch: true padding: Inset{{left: 6 top: 6 right: 6 bottom: 6}} draw_bg.color: #000000aa draw_bg.border_radius: 16.0\n\
                Image{{ src: http_resource(sys.satellite(35.68, 139.65)) fit: ImageFit.CropToFill width: Fill height: 190 }}\n\
                Label{{ text: \"卫星云图\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 11 }}\n\
            }}\n\
            RoundedView{{ width: Fill height: Fit flow: Down spacing: 3 new_batch: true padding: Inset{{left: 6 top: 6 right: 6 bottom: 6}} draw_bg.color: #000000aa draw_bg.border_radius: 16.0\n\
                View{{ width: Fill height: 190 flow: Overlay\n\
                    Image{{ src: http_resource(sys.basemap(35.68, 139.65)) fit: ImageFit.CropToFill width: Fill height: 190 }}\n\
                    Image{{ src: http_resource(sys.airmap(35.68, 139.65)) fit: ImageFit.CropToFill width: Fill height: 190 }}\n\
                }}\n\
                Label{{ text: \"空气质量图\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 11 }}\n\
            }}\n\
            View{{ width: Fill height: Fit flow: Down spacing: 2\n\
                View{{ width: Fill height: Fit flow: Right spacing: 8\n\
                    RoundedView{{ width: Fill height: Fit flow: Down spacing: 1 new_batch: true padding: Inset{{left: 14 top: 8 right: 14 bottom: 8}} draw_bg.color: #ffffff1f draw_bg.border_radius: 18.0\n\
                        Label{{ text: \"AIR QUALITY\" draw_text.color: #ffffff99 draw_text.text_style.font_size: 11 }}\n\
                        Label{{ text: \"42\" draw_text.color: #32d74b draw_text.text_style.font_size: 20 }}\n\
                        Label{{ text: \"Good\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 12 }}\n\
                    }}\n\
                    RoundedView{{ width: Fill height: Fit flow: Down spacing: 1 new_batch: true padding: Inset{{left: 14 top: 8 right: 14 bottom: 8}} draw_bg.color: #ffffff1f draw_bg.border_radius: 18.0\n\
                        Label{{ text: \"UV INDEX\" draw_text.color: #ffffff99 draw_text.text_style.font_size: 11 }}\n\
                        Label{{ text: \"5\" draw_text.color: #ffffff draw_text.text_style.font_size: 20 }}\n\
                        Label{{ text: \"Moderate\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 12 }}\n\
                    }}\n\
                }}\n\
                View{{ width: Fill height: Fit flow: Right spacing: 8\n\
                    RoundedView{{ width: Fill height: Fit flow: Down spacing: 1 new_batch: true padding: Inset{{left: 14 top: 8 right: 14 bottom: 8}} draw_bg.color: #ffffff1f draw_bg.border_radius: 18.0\n\
                        Label{{ text: \"SUNRISE\" draw_text.color: #ffffff99 draw_text.text_style.font_size: 11 }}\n\
                        Label{{ text: \"5:42 AM\" draw_text.color: #ffffff draw_text.text_style.font_size: 20 }}\n\
                        Label{{ text: \"🌅 Dawn\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 12 }}\n\
                    }}\n\
                    RoundedView{{ width: Fill height: Fit flow: Down spacing: 1 new_batch: true padding: Inset{{left: 14 top: 8 right: 14 bottom: 8}} draw_bg.color: #ffffff1f draw_bg.border_radius: 18.0\n\
                        Label{{ text: \"SUNSET\" draw_text.color: #ffffff99 draw_text.text_style.font_size: 11 }}\n\
                        Label{{ text: \"6:58 PM\" draw_text.color: #ffffff draw_text.text_style.font_size: 20 }}\n\
                        Label{{ text: \"🌇 Dusk\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 12 }}\n\
                    }}\n\
                }}\n\
                View{{ width: Fill height: Fit flow: Right spacing: 8\n\
                    RoundedView{{ width: Fill height: Fit flow: Down spacing: 1 new_batch: true padding: Inset{{left: 14 top: 8 right: 14 bottom: 8}} draw_bg.color: #ffffff1f draw_bg.border_radius: 18.0\n\
                        Label{{ text: \"HUMIDITY\" draw_text.color: #ffffff99 draw_text.text_style.font_size: 11 }}\n\
                        Label{{ text: \"64%\" draw_text.color: #ffffff draw_text.text_style.font_size: 20 }}\n\
                        Label{{ text: \"Dew point 58°\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 12 }}\n\
                    }}\n\
                    RoundedView{{ width: Fill height: Fit flow: Down spacing: 1 new_batch: true padding: Inset{{left: 14 top: 8 right: 14 bottom: 8}} draw_bg.color: #ffffff1f draw_bg.border_radius: 18.0\n\
                        Label{{ text: \"WIND\" draw_text.color: #ffffff99 draw_text.text_style.font_size: 11 }}\n\
                        Label{{ text: \"8 mph\" draw_text.color: #ffffff draw_text.text_style.font_size: 20 }}\n\
                        Label{{ text: \"NW\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 12 }}\n\
                    }}\n\
                }}\n\
            }}\n\
        }}\n\
    }}\n\
  RULES: the background Image MUST use `fit: ImageFit.CropToFill` (fills the whole \
box, cropping overflow — a true edge-to-edge photo). NEVER use Smallest/Biggest/\
Vertical/Horizontal on it: those size the photo to its own aspect and leave bare \
letterbox bands. The ROOT Overlay container and the Image MUST have NO `padding` and NO \
`margin` — an Overlay child's Fill height = parent height MINUS parent padding MINUS \
its own margin, so ANY inset there SHRINKS the photo and exposes bare background. Put \
ALL insets (the top: 44 status-bar clearance, side and bottom padding) ONLY on the \
inner `flow: Down` column, exactly as in the template. STRUCTURE top-to-bottom: (1) a \
CURRENT block — city (font 30), the hero temperature ALONE on its line (font 60, \
`margin: Inset{{top: 6 bottom: 0}}` so its tall glyphs are not clipped), \
then a `flow: Right` row (height 60, align y 0.5, spacing 10) holding an ANIMATED \
`WeatherIcon{{ draw_bg.cond: <N> width: 60 height: 60 }}` followed by the condition \
`Label` (font 20) — `WeatherIcon` is a live shader-animated weather glyph (rays \
rotate, rain/snow falls, wind/fog drifts, lightning flashes); pick `draw_bg.cond` by \
CURRENT condition: 0 clear/sunny, 1 partly cloudy, 2 cloudy/overcast, 3 rain/drizzle, \
4 thunderstorm, 5 snow, 6 wind, 7 fog/haze/mist. Then `H:__°   L:__°   Feels __°` \
(font 15, #ffffffcc); \
(2) a 7-DAY FORECAST directly under the current block (this comes BEFORE the detail \
grid) — a translucent RoundedView (draw_bg.color #00000055, border_radius 20) with ONE \
SolidView row per day, EACH ROW a FIXED `height: 40` (roomy iOS-style rows; the fixed \
height still clips color-emoji line-box inflation so rows stay uniform): day name width 92 (font 14), a weather EMOJI width 34 (☀️ sunny, \
⛅ partly, ☁️ cloudy, 🌧️ rain, ⛈️ storm, ❄️ snow), a Filler, then lo° dim (#ffffff88) and \
hi° white width 48, all font 14. Give SEVEN rows: Today, then the next six days by name; \
(3) TWO FULL-WIDTH MAP PANES, stacked vertically (NOT side by side — each pane is its \
own row so the maps read large), each a `width: Fill` RoundedView (draw_bg.color \
#000000aa, border_radius 16, flow: Down): the FIRST pane is the 卫星云图 — REAL satellite \
cloud imagery — `Image{{ src: http_resource(sys.satellite(LAT, LON)) fit: \
ImageFit.CropToFill width: Fill height: 190 }}` (sys.satellite(LAT, LON) takes the city's \
real lat/lon, SAME as the air map below) + a `卫星云图` caption (font 11, #ffffffcc); the \
SECOND pane is the LIVE 空气质量图 air-quality map — a `height: 190 flow: Overlay` View \
stacking `Image{{ src: http_resource(sys.basemap(LAT, LON)) fit: ImageFit.CropToFill \
width: Fill height: 190 }}` UNDER `Image{{ src: http_resource(sys.airmap(LAT, LON)) fit: \
ImageFit.CropToFill width: Fill height: 190 }}` (fixed height, NOT Fill — Fill inside an \
Overlay wrongly resolves to the whole card) — pass the CITY's real decimal LAT, LON \
(e.g. Tokyo 35.68, 139.65; both maps take the SAME lat/lon) — + a `空气质量图` caption \
(font 11, #ffffffcc); (4) a DETAIL GRID below the map panes — a `flow: Down` View \
of THREE `flow: Right` rows, \
each holding TWO equal frosted tiles (`width: Fill`). Every tile is a RoundedView \
(draw_bg.color #ffffff1f, border_radius 18) stacking an UPPERCASE caption (font 11, \
#ffffff99), a big value (font 20), and a sub-line (font 12, #ffffffcc). The SIX tiles in \
order: AIR QUALITY (value = the AQI NUMBER; set its `draw_text.color` by category — \
Good #32d74b, Moderate #ffd60a, Unhealthy #ff9f0a, Very Unhealthy #ff453a — and put the \
category word in the sub-line), UV INDEX (a 0–11 value; sub Low/Moderate/High/Very High), \
SUNRISE (a clock time; sub `🌅 Dawn`), SUNSET (a clock time; sub `🌇 Dusk`), HUMIDITY \
(a percent; sub `Dew point __°`), WIND (e.g. `8 mph`; sub the compass direction like \
`NW`). The WHOLE \
inner column is a TALL, VERTICALLY-SCROLLING page (~1500dp) — it does NOT need to fit \
one screen; the user DRAGS to scroll down and reveal the forecast, the maps row and the \
detail grid, so use comfortable, breathable spacing rather than cramming everything in. Image: `sys.photo(\"<city> <scene/weather>\")` matching the actual \
conditions.\n\
- Keep it self-contained and visually clean (padding, spacing, rounded \
containers, readable labels).\n\
- CRITICAL OVERRIDE (takes precedence over the manual's `let` examples): the \
block MUST BEGIN DIRECTLY with a single root container widget — e.g. \
`RoundedView{{` or `View{{`. Do NOT start with, or use, any top-level `let \
X = …` component definitions. Inline/repeat any shared structure directly, \
even if it makes the output longer. A leading `let` will fail to render.\n\
- NO custom shaders/MPSL: never write `pixel: fn`, `fn(`, `let`, `mut`, `Sdf2d`, \
`uniform(`, `instance(`, or `.mix(` inside `draw_bg` — they crash the WHOLE card \
into ugly raw source. WIDGET-PROPERTY RULES (setting a property a widget does not \
have ALSO crashes the card): a ROUNDED card is \
`RoundedView{{ draw_bg.color: #hex draw_bg.border_radius: 20.0 }}` (solid fill, \
supports border_radius). A GRADIENT is \
`GradientYView{{ draw_bg.color: #topHex draw_bg.color_2: #botHex }}` (vertical; \
`GradientXView` = horizontal) — it is a full-width RECTANGLE and has NO \
border_radius, so NEVER put `border_radius` on a Gradient*View. Pick one per \
container; don't mix. Style ONLY with: draw_bg.color, draw_bg.color_2 \
(gradient views only), draw_bg.border_radius (rounded views only), \
draw_text.color, draw_text.text_style.font_size.\n\
- iOS REFINEMENT (make it look like a real iOS app): prefer \
`RoundedShadowView{{ draw_bg.color: #hex draw_bg.border_radius: 24.0 draw_bg.shadow_color: #00000055 draw_bg.shadow_offset: vec2(0.0, 8.0) draw_bg.shadow_radius: 24.0 margin: 14 }}` \
as the CARD container — rounded corners + a soft iOS drop shadow (it DOES support \
border_radius; keep a `margin` so the shadow has room). WRAP long text: any \
headline/sentence Label MUST set `width: Fill` so it wraps to multiple lines instead \
of clipping. Size hierarchy via font_size: hero value 52-72 (a very large number like a \
temperature MUST have `margin: Inset{{top: 10 bottom: 6}}` and its OWN line, or \
its tall glyph tops get clipped by the label above it), title 16-18, row 15, \
caption 12-13; make secondary text translucent `draw_text.color: #ffffff99` (or \
`#8e8e93` on light cards). Hairline row dividers: \
`SolidView{{ width: Fill height: 1 draw_bg.color: #ffffff14 }}`. iOS system colors: \
blue #0a84ff, red #ff453a, green #32d74b, dark card #1c1c1e, light card #f2f2f7. \
Generous, consistent padding (18-24) and spacing (10-14).\n\
- LIVE DATA: you may fetch real data with a web tool, but it reliably returns only \
SIMPLE single-endpoint sources — e.g. weather `https://wttr.in/<City>?format=j1`. \
Multi-request or big-JSON APIs (stock quotes, news lists) usually FAIL; if the user did \
not supply those numbers, ask for them — never invent live prices or headlines.\n\
- ITERATE: if the user asks to refine a card you built earlier in this chat, reuse its \
structure and change only what they asked; still exactly one runsplash block.\n\n\
Follow this Splash manual EXACTLY (except the overrides above):\n\n{manual}\n\n\
User request: {request}",
        manual = SPLASH_MANUAL,
        request = request,
    )
}

/// Per-card A2App/Splash state: `{{state.<key>}}` key → value. Each rendered
/// card owns one of these (keyed by message index in `CHAT_DATA.a2app_state`)
/// so independent cards never share state.
type CardState = std::collections::BTreeMap<String, String>;

/// Tag every `agent.notify("<ev>"` / `agent.notify('<ev>'` in a Splash body with
/// the owning card's id → `agent.notify("<item_id>:<ev>"`. The framework's
/// `SplashAction::Notify` carries no source card, so this prefix is how a button
/// press is routed back to the card that fired it (per-card state isolation).
fn tag_notify_calls(body: &str, item_id: usize) -> String {
    if !body.contains("agent.notify(") {
        return body.to_string();
    }
    body.replace("agent.notify(\"", &format!("agent.notify(\"{item_id}:"))
        .replace("agent.notify('", &format!("agent.notify('{item_id}:"))
}

/// Rewrite a bare `View{` — the transparent layout container LLMs reach for —
/// into `SolidView{show_bg: false `. A bare `View{` crashes the Splash eval,
/// which dumps the WHOLE card as raw source instead of UI; a `SolidView` with
/// its background disabled is an equivalent invisible layout container that
/// renders. Only rewrites `View{` NOT preceded by an ASCII letter, so
/// `RoundedView{`, `SolidView{`, `GradientYView{`, `ScrollXView{`, … stay intact.
fn neutralize_bare_view(body: &str) -> String {
    if !body.contains("View{") {
        return body.to_string();
    }
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len() + 32);
    let mut last = 0;
    let mut search = 0;
    while let Some(rel) = body[search..].find("View{") {
        let pos = search + rel;
        if pos > 0 && bytes[pos - 1].is_ascii_alphabetic() {
            // part of a longer widget name (RoundedView, SolidView, …) — skip
            search = pos + "View{".len();
            continue;
        }
        out.push_str(&body[last..pos]);
        // Bare `View{}` crashes the Splash eval, so substitute a safe container.
        // NOT SolidView — this fork's SolidView paints an uninitialized red fill
        // regardless of draw_bg.color (seen as red bands where a card didn't
        // opaquely cover). RoundedView honours draw_bg.color, so a transparent
        // fill makes the substitute invisible.
        out.push_str("RoundedView{new_batch: true draw_bg.color: #00000000 draw_bg.border_radius: 0.0 ");
        last = pos + "View{".len();
        search = last;
    }
    out.push_str(&body[last..]);
    out
}

/// Force every full-bleed background `Image` (one sized `height: Fill`) to
/// `fit: ImageFit.CropToFill`. In `flow: Overlay`, `ImageFit.Biggest`/`.Smallest`
/// size the image's walk from a mis-resolved available height — an Overlay+Fill
/// child peeks a too-short height — so the photo renders shorter than its box and
/// letterboxes, exposing bare backing that reads as RED bands on this device.
/// `CropToFill` keeps the quad at the full box and crops via UV coords, so the
/// photo always covers edge-to-edge regardless of the peeked height. Saved cards
/// authored before this rule — and an LLM that reproduces them verbatim — still
/// carry the old fit, so enforce it at render time rather than trusting the DSL.
fn force_fullbleed_image_fit(body: &str) -> String {
    if !body.contains("Image{") {
        return body.to_string();
    }
    // Pin the full-screen card root to the same height as the background image.
    // The immersive template's Overlay root is `height: 700`; a background image
    // TALLER than its container (the old `height: 920`) is mis-positioned by the
    // Overlay and leaves a red strip above the photo. Matching root == image so
    // the image fills the container exactly removes the offset.
    let body = body.replace("height: 700", &format!("height: {FULLBLEED_CARD_HEIGHT}"));
    // Pin full-bleed images to THIS card's root height (legacy cards are
    // 1200dp, current weather cards 1500dp) so root == image always holds.
    let full_h = card_root_height(&body).unwrap_or(FULLBLEED_CARD_HEIGHT);
    let body = body.as_str();
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len() + 32);
    let mut i = 0;
    while i < body.len() {
        let rel = match body[i..].find("Image{") {
            Some(r) => r,
            None => {
                out.push_str(&body[i..]);
                break;
            }
        };
        let start = i + rel;
        let brace = start + "Image".len(); // index of the '{'
        let part_of_name = start > 0 && bytes[start - 1].is_ascii_alphanumeric();
        // Find the matching close brace for this Image{ … } (props may nest
        // `Inset{…}`, `vec2(…)`, etc., so count depth).
        let mut depth = 0i32;
        let mut j = brace;
        while j < body.len() {
            match bytes[j] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        if part_of_name || j >= body.len() {
            // `…Image{` is a longer identifier, or braces are unbalanced — copy
            // through the brace and keep scanning.
            out.push_str(&body[i..brace + 1]);
            i = brace + 1;
            continue;
        }
        let inner = &body[brace + 1..j];
        let full_bleed = inner.contains("height: Fill") || inner.contains("height:Fill");
        out.push_str(&body[i..brace + 1]); // up to and including the '{'
        if full_bleed {
            out.push_str(&rewrite_image_fit_crop(inner, full_h));
        } else {
            out.push_str(inner);
        }
        out.push('}');
        i = j + 1;
    }
    out
}

/// Guarantee a full-bleed Image actually covers its box: force `CropToFill`
/// (crop-to-cover, never contain) AND replace `height: Fill` with a fixed
/// full-screen height. `height: Fill` on the FIRST child of a `flow: Overlay`
/// container resolves to a too-short intrinsic height (~500dp) and Overlay then
/// CENTERS the image, leaving equal letterbox gaps top and bottom that expose
/// bare (red) backing. A fixed height ≥ the screen makes the quad span the whole
/// card, so the photo is truly edge-to-edge and nothing shows through.
/// Fixed height (Makepad logical units) for a full-screen card root and its
/// background image — sized to fill this device's viewport. Root and image share
/// it so the Overlay image covers the card exactly (no offset, no letterbox).
const FULLBLEED_CARD_HEIGHT: u32 = 1200;
/// Height forced onto a full-bleed card ROOT that the model made `height: Fill`.
/// Matches the immersive weather template's `height: 1500`, so after the rewrite
/// `card_root_height` finds it and `force_fullbleed_image_fit` pins the
/// background image to the same value (root == image, nothing letterboxes).
const FULLBLEED_FALLBACK_HEIGHT: u32 = 1500;
fn rewrite_image_fit_crop(inner: &str, full_h: u32) -> String {
    let mut s = inner.to_string();
    for v in ["Biggest", "Smallest", "Vertical", "Horizontal", "Stretch", "Size"] {
        s = s.replace(&format!("ImageFit.{v}"), "ImageFit.CropToFill");
    }
    if !s.contains("ImageFit.") {
        s = format!(" fit: ImageFit.CropToFill{s}");
    }
    let h = format!("height: {full_h}");
    s = s.replace("height: Fill", &h).replace("height:Fill", &h);
    s
}

/// First explicit `height: <n>` (n ≥ 700) in a card body — the card root's
/// fixed height. Full-bleed background images are pinned to THIS instead of a
/// global constant, so legacy 1200dp cards and taller current cards (1500dp
/// weather) both end up with root == image and stay fully covered.
fn card_root_height(body: &str) -> Option<u32> {
    let mut i = 0;
    while let Some(rel) = body[i..].find("height: ") {
        let s = i + rel + "height: ".len();
        let end = body[s..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|e| s + e)
            .unwrap_or(body.len());
        if end > s {
            if let Ok(v) = body[s..end].parse::<u32>() {
                if v >= 700 {
                    return Some(v);
                }
            }
        }
        i = s;
    }
    None
}

/// A card whose ROOT container is `height: Fill` collapses to a blank slot: the
/// immersive card system sizes each card from its intrinsic height
/// (`card_root_height`), and a `Fill` root has none, so the Overlay root resolves
/// to zero. Layout still runs and images still decode — the card looks
/// "generated but invisible" — but nothing paints. The immersive template pins
/// the root to a fixed `height: 1500`; a model that reaches for `height: Fill` on
/// the root instead ships a blank card. Enforce the fixed height at render time
/// rather than trusting the DSL (same philosophy as `force_fullbleed_image_fit`,
/// and ordered BEFORE it so the background image pins to the now-fixed root
/// height: root == image, no letterbox).
///
/// Only the ROOT's own `height: Fill` is rewritten — the search is confined to
/// the root's attribute span (before its first nested `{`), so a child's
/// `height: Fill` is never touched — and only when the card declares NO fixed
/// height >= 700 anywhere. A card that already pins its root renders fine and is
/// left alone, as are small `height: Fit` cards (whose first `height: Fill`, if
/// any, belongs to a child this never reaches).
fn pin_fullbleed_root_height(body: &str) -> String {
    // Already has a fixed root height (>= 700) → it renders; don't touch it.
    if card_root_height(body).is_some() {
        return body.to_string();
    }
    let Some(root_open) = body.find('{') else {
        return body.to_string();
    };
    // Root's own attributes run from its `{` to the first nested `{` (a child
    // widget, or a brace-valued attr like `Inset{…}`). Confining the rewrite
    // there guarantees a child's `height: Fill` is never rewritten; the worst
    // case (a brace-valued attr ahead of `height`) is a no-op, not a misedit.
    let attr_end = body[root_open + 1..]
        .find('{')
        .map(|r| root_open + 1 + r)
        .unwrap_or(body.len());
    let attrs = &body[root_open + 1..attr_end];
    let fixed = format!("height: {FULLBLEED_FALLBACK_HEIGHT}");
    let new_attrs = if attrs.contains("height: Fill") {
        attrs.replacen("height: Fill", &fixed, 1)
    } else if attrs.contains("height:Fill") {
        attrs.replacen("height:Fill", &fixed, 1)
    } else {
        return body.to_string();
    };
    format!("{}{}{}", &body[..root_open + 1], new_attrs, &body[attr_end..])
}

/// Substitute `{{state.<key>}}` tokens with this card's live values. Missing
/// keys render `"0"` (keeps counter cards reading 0 before any interaction, and
/// is a safe default for a not-yet-set string).
fn substitute_state_keys(text: &str, state: &CardState) -> String {
    if !text.contains("{{state.") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("{{state.") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + "{{state.".len()..];
        if let Some(end) = after.find("}}") {
            let key = after[..end].trim();
            out.push_str(state.get(key).map(String::as_str).unwrap_or("0"));
            rest = &after[end + 2..];
        } else {
            out.push_str(&rest[pos..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

// ── Phase 1: card composition (Card{ use: … } embeds) ────────────────────────
// Coarse, reusable cards an LLM composes into apps. `Card{ use: "nav.navigate"
// props: {…} on: {…} }` is expanded HOST-SIDE by inlining the referenced card's
// body — with its `{{props.k}}` inputs bound and its internal state namespaced —
// into the one combined card before it reaches the Splash VM. So the composed
// app is a single card/VM assembled from parts (one live MapView at a time), and
// the LLM only has to emit a small host that wires pre-built cards. See
// a2app/apps/nav/DECOMPOSITION.md.

/// Registry: `use:` name → the direct-served component body. Extend as cards are
/// extracted (nav.picker / nav.planner next).
fn embeddable_card(name: &str) -> Option<&'static str> {
    match name.trim() {
        "nav.navigate" => Some(include_str!(
            "../../../a2app/apps/nav/cards/navigate.splash"
        )),
        _ => None,
    }
}

/// Cap on `Card{}` nesting expanded — guards against a card that embeds itself
/// (direct or mutual recursion) blowing the stack; unmatched depth is left as an
/// inert placeholder.
const MAX_CARD_EMBED_DEPTH: u8 = 4;

/// Index of the `}` matching the `{` at `open` (which must point AT that `{`).
/// String-aware and `{{…}}`-aware: braces inside `"…"` and the `{{state.x}}` /
/// `{{props.x}}` token delimiters do NOT count toward depth, so a prop value like
/// `"{{state.drop}}"` can't unbalance the scan. Returns None if unmatched.
fn matching_brace(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    if open >= b.len() || b[open] != b'{' {
        return None;
    }
    let mut depth = 0i32;
    let mut i = open;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        // Braces inside "…" are skipped via in_str above. A bare template token
        // `{{state.x}}` (e.g. `let ss = {{state.sel}}`) is self-balancing (+2/−2),
        // so it needs no special-casing — and special-casing `}}` would wrongly
        // swallow adjacent STRUCTURAL closers like `{key:"x"}}`.
        match c {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Split the inside of a `{ … }` block on TOP-LEVEL commas (commas nested inside
/// `{…}` or `"…"` are not separators). Used to parse `props:` / `on:` bodies.
fn split_top_level_commas(inner: &str) -> Vec<String> {
    let b = inner.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let tail = inner[start..].trim();
    if !tail.is_empty() {
        parts.push(tail.to_string());
    }
    parts
}

/// Strip one layer of matching quotes and trim.
fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

/// Parse a flat `key: value, key: value` body into a map (values unquoted). Used
/// for `props:`.
fn parse_flat_map(inner: &str) -> std::collections::BTreeMap<String, String> {
    let mut m = std::collections::BTreeMap::new();
    for part in split_top_level_commas(inner) {
        if let Some(colon) = part.find(':') {
            let k = part[..colon].trim().to_string();
            let v = unquote(&part[colon + 1..]);
            if !k.is_empty() {
                m.insert(k, v);
            }
        }
    }
    m
}

/// One `on:` handler: when the child emits `event`, write parent state `key`. If
/// `value` is Some, that literal is written; if None, the value carried by the
/// emit is passed through (so `nav.picker`'s `pick` stores the chosen place).
struct EmitHandler {
    key: String,
    value: Option<String>,
}

/// Parse an `on: { ev: { key: "k", value: "v" }, ev2: { key: "k2" } }` body.
fn parse_on_map(inner: &str) -> std::collections::BTreeMap<String, EmitHandler> {
    let mut m = std::collections::BTreeMap::new();
    for part in split_top_level_commas(inner) {
        let Some(colon) = part.find(':') else { continue };
        let ev = part[..colon].trim().to_string();
        let rhs = part[colon + 1..].trim();
        let obj = rhs.strip_prefix('{').and_then(|r| r.strip_suffix('}')).unwrap_or(rhs);
        let fields = parse_flat_map(obj);
        if let Some(key) = fields.get("key") {
            m.insert(
                ev,
                EmitHandler {
                    key: key.clone(),
                    value: fields.get("value").cloned(),
                },
            );
        }
    }
    m
}

/// Substitute `{{props.<k>}}` tokens with the embed's bound prop values. An
/// unbound prop renders `"0"` — matching the cards' "0 = unset" convention, so a
/// child's `if x != "0"` guard treats an omitted optional input as unset. Same
/// scanning shape as `substitute_state_keys`.
fn substitute_props(text: &str, props: &std::collections::BTreeMap<String, String>) -> String {
    if !text.contains("{{props.") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("{{props.") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + "{{props.".len()..];
        if let Some(end) = after.find("}}") {
            let key = after[..end].trim();
            out.push_str(props.get(key).map(String::as_str).unwrap_or("0"));
            rest = &after[end + 2..];
        } else {
            out.push_str(&rest[pos..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

/// Namespace a child card's INTERNAL state so two embeds of the same card (or the
/// parent) never collide: `{{state.X}}` reads and `agent.notify("set", {key: "X"`
/// writes both become `…_c<inst>_X`. The emit mechanism uses `event:` (not
/// `key:`), so it is untouched here and rewritten separately.
fn namespace_child_state(body: &str, inst: u32) -> String {
    let prefix = format!("_c{inst}_");
    let reads = body.replace("{{state.", &format!("{{{{state.{prefix}"));
    // writes: key: "X"  /  key:"X"  inside notify payloads
    let w1 = reads.replace("{key: \"", &format!("{{key: \"{prefix}"));
    w1.replace("{key:\"", &format!("{{key:\"{prefix}"))
}

/// Rewrite the child's `agent.notify("emit", {event: "<ev>" [, value: <v>]})`
/// calls into parent-state writes per the embed's `on:` map. An emit with no
/// matching handler becomes an inert namespaced write (so it never leaks a
/// dangling notify). Parent keys written here are NOT namespaced — they are the
/// composition bus the host wires.
fn rewrite_child_emits(
    body: &str,
    inst: u32,
    on: &std::collections::BTreeMap<String, EmitHandler>,
) -> String {
    let marker = "agent.notify(\"emit\"";
    if !body.contains(marker) {
        return body.to_string();
    }
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(pos) = rest.find(marker) {
        out.push_str(&rest[..pos]);
        // find the payload object `{ … }` and the enclosing `)`
        let after = &rest[pos..];
        let Some(brace_rel) = after.find('{') else {
            out.push_str(after);
            return out;
        };
        let Some(brace_end) = matching_brace(after, brace_rel) else {
            out.push_str(after);
            return out;
        };
        // the call ends at the first ')' after the payload
        let Some(paren_rel) = after[brace_end..].find(')') else {
            out.push_str(after);
            return out;
        };
        let call_end = brace_end + paren_rel + 1;
        let payload = &after[brace_rel + 1..brace_end];
        let fields = parse_flat_map(payload);
        let ev = fields.get("event").cloned().unwrap_or_default();
        let replacement = match on.get(&ev) {
            Some(h) => {
                let val = h
                    .value
                    .clone()
                    .or_else(|| fields.get("value").cloned())
                    .unwrap_or_else(|| "1".to_string());
                format!(
                    "agent.notify(\"set\", {{key: \"{}\", value: \"{}\"}})",
                    h.key, val
                )
            }
            None => format!(
                "agent.notify(\"set\", {{key: \"_c{inst}_unhandled_{ev}\", value: \"1\"}})"
            ),
        };
        out.push_str(&replacement);
        rest = &after[call_end..];
    }
    out.push_str(rest);
    out
}

/// Remove `//`-to-end-of-line comments that are NOT inside a string literal, so a
/// card's doc comments (which may legitimately mention `Card{`, `{{props.…}}`,
/// etc.) can't be misread as code by the embed scanner — and so the composed
/// output carries no stale component docs. Preserves newlines and string content
/// (incl. `://` inside a quoted URL).
fn strip_line_comments(body: &str) -> String {
    let b = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0usize;
    let mut seg_start = 0usize;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
        } else if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            out.push_str(&body[seg_start..i]);
            let mut j = i + 2;
            while j < b.len() && b[j] != b'\n' {
                j += 1;
            }
            i = j; // land on '\n' (or EOF); the newline resumes the next segment
            seg_start = i;
            continue;
        }
        i += 1;
    }
    out.push_str(&body[seg_start..]);
    out
}

/// Expand every `Card{ use: "<name>" props: {…} on: {…} }` embed by inlining the
/// referenced card body with props bound, state namespaced, and emits rewritten.
/// Recurses (bounded by `MAX_CARD_EMBED_DEPTH`) so a card may embed another;
/// `next_inst` threads a globally-unique instance counter across all levels.
fn expand_card_embeds(body: &str, depth: u8, next_inst: &mut u32) -> String {
    if depth >= MAX_CARD_EMBED_DEPTH || !body.contains("Card{") {
        return body.to_string();
    }
    // Comments can mention `Card{` (e.g. a component's own usage doc) — strip
    // them so only real code is scanned, and the composed card stays lean.
    let body = strip_line_comments(body);
    let body = body.as_str();
    if !body.contains("Card{") {
        return body.to_string();
    }
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    // Match `Card{` only when not part of a longer identifier (e.g. `MyCard{`).
    while let Some(rel) = rest.find("Card{") {
        let pre = rest.as_bytes().get(rel.wrapping_sub(1)).copied();
        if rel > 0 && pre.map(|c| c.is_ascii_alphanumeric() || c == b'_').unwrap_or(false) {
            // not a standalone `Card{` — copy through and keep scanning
            out.push_str(&rest[..rel + "Card{".len()]);
            rest = &rest[rel + "Card{".len()..];
            continue;
        }
        let brace = rel + "Card".len();
        let Some(end) = matching_brace(rest, brace) else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..rel]);
        let attrs = &rest[brace + 1..end];
        out.push_str(&expand_one_card(attrs, depth, next_inst));
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Expand the attributes of a single `Card{ … }` into inlined child body (or an
/// inert error placeholder for an unknown/omitted `use:`).
fn expand_one_card(attrs: &str, depth: u8, next_inst: &mut u32) -> String {
    // use: "<name>"
    let name = attrs
        .find("use:")
        .map(|p| {
            let after = attrs[p + "use:".len()..].trim_start();
            let after = after.strip_prefix('"').unwrap_or(after);
            after.split('"').next().unwrap_or("").to_string()
        })
        .unwrap_or_default();
    let props = extract_braced(attrs, "props:")
        .map(|b| parse_flat_map(&b))
        .unwrap_or_default();
    let on = extract_braced(attrs, "on:")
        .map(|b| parse_on_map(&b))
        .unwrap_or_default();

    let Some(raw) = embeddable_card(&name) else {
        return format!(
            "RoundedView{{width: Fill height: 40 draw_bg.color: #3a1420 \
Label{{text: \"⚠ unknown card: {name}\" draw_text.color: #ffb4b4}}}}"
        );
    };
    let inst = *next_inst;
    *next_inst += 1;

    let child = strip_card_name_line(raw).into_owned();
    // ORDER MATTERS: namespace the child's OWN state FIRST (so only its internal
    // {{state.x}} / {key:"x"} get the _c<inst>_ prefix), THEN inject props — a
    // prop bound to a parent {{state.drop}} must stay an un-namespaced PARENT ref.
    let child = namespace_child_state(&child, inst);
    let child = substitute_props(&child, &props);
    // Emits map to parent keys inserted last, so they too stay un-namespaced.
    let child = rewrite_child_emits(&child, inst, &on);
    // A child may itself embed cards (e.g. planner → picker).
    expand_card_embeds(&child, depth + 1, next_inst)
}

/// Return the inside of the `{ … }` block that follows `label` in `attrs`, if
/// present (string/`{{…}}`-aware brace matching).
fn extract_braced(attrs: &str, label: &str) -> Option<String> {
    let p = attrs.find(label)?;
    let after = &attrs[p + label.len()..];
    let open_rel = after.find('{')?;
    let end = matching_brace(after, open_rel)?;
    Some(after[open_rel + 1..end].to_string())
}

/// Persistent registry of named A2App cards, so a card can be retrieved by
/// name and refined/improved over time (`$HOME` is the app-private files dir
/// on Android; see `set_var("HOME", get_data_dir())` at startup).
fn a2app_cards_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(std::path::Path::new(&home).join("a2app_cards"))
}

/// Extract the `// name: <slug>` directive the model puts on the FIRST line of a
/// card body. Sanitized to a stable kebab slug so it names a file safely.
fn extract_card_name(body: &str) -> Option<String> {
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("// name:").or_else(|| t.strip_prefix("//name:")) {
            let slug: String = rest
                .trim()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
                .collect();
            let slug = slug.trim_matches('-').to_string();
            if !slug.is_empty() {
                return Some(slug.chars().take(48).collect());
            }
        }
        // The directive lives at the very top; stop once real widget code starts.
        if t.starts_with(|c: char| c.is_ascii_uppercase()) {
            break;
        }
    }
    None
}

/// Drop the `// name:` directive line before the body reaches the Splash VM
/// (which does not accept `//` line comments — leaving it in crashes the card).
/// Only matches a line whose trimmed text starts with `// name:`, so URLs
/// containing `//` inside strings are untouched.
fn strip_card_name_line(body: &str) -> std::borrow::Cow<'_, str> {
    if !body.contains("// name:") && !body.contains("//name:") {
        return std::borrow::Cow::Borrowed(body);
    }
    let kept: Vec<&str> = body
        .lines()
        .filter(|l| {
            let t = l.trim();
            !(t.starts_with("// name:") || t.starts_with("//name:"))
        })
        .collect();
    std::borrow::Cow::Owned(kept.join("\n"))
}

/// Persist a generated card so it can be retrieved by name and refined over
/// time — the "save" half of the generate→save loop. Writes three things:
/// the raw card body (`<name>.splash` / `<name>.html`, latest revision), a
/// `<name>.meta.json` sidecar (substrate, owning domain, session, triggering
/// prompt, timestamp), and one appended line in `index.jsonl` — the
/// append-only ledger that makes every generation/refinement traceable.
///
/// `plan_source` is the plan the card was lowered from, when there was one. It is
/// carried so the plain-data form can be published alongside the makepad one — the two
/// are siblings from a single plan, not translations of each other.
fn save_card_artifact(
    name: &str,
    substrate: &str,
    body: &str,
    domain: Option<&str>,
    prompt: Option<&str>,
    session_id: Option<&str>,
    // LAST, matching every call site. It was briefly 4th while the calls passed it 7th,
    // and because four consecutive parameters are all `Option<&str>` the compiler could
    // not tell — the domain silently arrived as the plan and the lowering was skipped
    // with "plan is not JSON: \"weather\"". Same-typed positional parameters are the
    // hazard; keeping the new one at the end is the cheap guard.
    plan_source: Option<&str>,
) {
    let Some(dir) = a2app_cards_dir() else {
        log::warn!("a2app: cannot save card '{name}' — no HOME/cards dir");
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let ext = if substrate == "runhtml" { "html" } else { "splash" };
    let path = dir.join(format!("{name}.{ext}"));
    // Also publish the PLAIN-DATA form of the same plan, for backends that render
    // Splash without makepad's widget registry (see app/splash-native). A makepad card
    // says `SolidView{…}`; a registry-free renderer needs `{t: "col", …}`. Both come
    // from the ONE plan the model emitted, so this is a second lowering rather than a
    // translation — and publishing it here means such a backend renders the model's real
    // output instead of a hand-written stand-in.
    if let Some(plan_json) = plan_source {
        match crate::app::plan::nodes::try_plain(plan_json) {
            Ok(plain) => {
                // The app's own media directory: writable by this app without any
                // permission, and world-READABLE, which is what a second renderer needs.
                // `/data/local/tmp` looked simpler and is not writable by an app at all —
                // SELinux blocks it whatever the mode bits say, which is why the first
                // attempt failed with EACCES on a 777 directory.
                let handoff = std::path::Path::new(
                    "/storage/emulated/0/Android/media/dev.makepad.octos_app/cards",
                );
                if let Err(e) = std::fs::create_dir_all(handoff) {
                    log::warn!("a2app: cannot create handoff dir: {e}");
                } else {
                    let p = handoff.join(format!("{name}.splash"));
                    match std::fs::write(&p, &plain) {
                        Ok(()) => log::info!(
                            "a2app: published plain-data card ({} bytes) → {}",
                            plain.len(),
                            p.display()
                        ),
                        Err(e) => log::warn!("a2app: plain-data publish failed: {e}"),
                    }
                }
            }
            // Say WHY. "no plain-data lowering" alone sent me guessing at the cause
            // when the real answer was in the text I had not printed.
            Err(e) => log::warn!("a2app: plain-data lowering skipped — {e}"),
        }
    }
    match std::fs::write(&path, body) {
        Ok(()) => log::info!("a2app: saved card '{name}' ({substrate}, {} bytes) → {}", body.len(), path.display()),
        Err(e) => log::warn!("a2app: save card '{name}' failed: {e}"),
    }
    let record = serde_json::json!({
        "name": name,
        "substrate": substrate,
        "domain": domain,
        "session_id": session_id,
        "prompt": prompt,
        "bytes": body.len(),
        "saved_at": chrono::Utc::now().to_rfc3339(),
    });
    let meta_path = dir.join(format!("{name}.meta.json"));
    if let Err(e) = std::fs::write(&meta_path, serde_json::to_vec_pretty(&record).unwrap()) {
        log::warn!("a2app: save card meta '{name}' failed: {e}");
    }
    let mut line = serde_json::to_string(&record).unwrap();
    line.push('\n');
    use std::io::Write as _;
    match std::fs::OpenOptions::new().create(true).append(true).open(dir.join("index.jsonl")) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(line.as_bytes()) {
                log::warn!("a2app: append card index '{name}' failed: {e}");
            }
        }
        Err(e) => log::warn!("a2app: open card index failed: {e}"),
    }
}

/// Mid-stream save: a card is complete enough to render the moment its
/// closing fence arrives, so persist it then — a stalled or cancelled turn
/// must still leave a traceable artifact (observed on-device: a youtube card
/// fully rendered while its turn never finalized). Runs on every delta;
/// deduped per turn via `data.saved_stream_cards`. The turn-complete save
/// remains the final revision.
fn save_completed_stream_cards(
    data: &mut ChatData,
    domain: Option<String>,
    session: Option<String>,
) {
    let text = data.streaming_text.as_str();
    if !text.contains("```") {
        return;
    }
    let prompt = data
        .messages
        .iter()
        .rev()
        .find(|m| m.role == ChatRole::User)
        .map(|m| m.text.clone());
    if let Some(body) = card_splash_body(text) {
        // Never persist a forbidden card (same rule as the turn-complete
        // path): scan ALL blocks, not just the first.
        let forbidden = extract_all_runsplash_bodies(text)
            .into_iter()
            .find_map(runsplash_body_forbidden)
            .is_some();
        if !forbidden {
            if let Some(name) = extract_card_name(&body) {
                if data.saved_stream_cards.insert(format!("runsplash:{name}")) {
                    save_card_artifact(
                        &name,
                        "runsplash",
                        &body,
                        domain.as_deref(),
                        prompt.as_deref(),
                        session.as_deref(),
                        extract_runplan_body(text),
                    );
                }
            }
        }
    }
    if let Some(html) = extract_runhtml_body(text) {
        if let Some(name) = extract_html_card_name(html) {
            if data.saved_stream_cards.insert(format!("runhtml:{name}")) {
                save_card_artifact(
                    &name,
                    "runhtml",
                    html,
                    domain.as_deref(),
                    prompt.as_deref(),
                    session.as_deref(),
                    // A hand-written HTML card has no plan, so no plain-data sibling.
                    None,
                );
            }
        }
    }
}

/// Load saved cards as `(name, dsl)`, newest-modified first, capped at `max`.
fn load_a2app_cards(max: usize) -> Vec<(String, String)> {
    let Some(dir) = a2app_cards_dir() else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut entries: Vec<(std::time::SystemTime, String, String)> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("splash") {
            continue;
        }
        let name = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
        let mtime = e
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if let Ok(dsl) = std::fs::read_to_string(&p) {
            if !name.is_empty() {
                entries.push((mtime, name, dsl));
            }
        }
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.into_iter().take(max).map(|(_, n, d)| (n, d)).collect()
}

/// Prepare a *raw* Splash body for a specific card: drop the `// name:`
/// directive, substitute its state values, neutralize bare `View{}`, and tag
/// its notify calls with the card id.
fn substitute_card_state(body: &str, item_id: usize, state: &CardState) -> String {
    let named = strip_card_name_line(body);
    // Expand Card{ use: … } composition FIRST, so an embedded child's inlined
    // body (and any {{props.k}} bound to a parent {{state.x}}) is present before
    // state substitution resolves it. No-op for cards with no embeds.
    let mut inst = 0u32;
    let composed = expand_card_embeds(&named, 0, &mut inst);
    let subst = substitute_state_keys(&composed, state);
    let safe = neutralize_bare_view(&subst);
    // Pin a `height: Fill` root to a fixed height BEFORE the image fit, so the
    // background image (`force_fullbleed_image_fit`) pins to the same height.
    let rooted = pin_fullbleed_root_height(&safe);
    let fitted = force_fullbleed_image_fit(&rooted);
    tag_notify_calls(&fitted, item_id)
}

/// Whole-message variant: substitute `{{state.*}}` and tag notify calls ONLY
/// inside ```runsplash fenced blocks (the generated live UI). Ordinary prose or
/// other code fences are left verbatim — they're the model's own text, not live
/// state, and rewriting them was a bug (`{{state.count}}` in an explanation
/// became `0`). No-op for normal messages: no runsplash block ⇒ nothing to do.
fn resolve_a2app_card(cx: &mut Cx, text: &str, item_id: usize, state: &CardState) -> String {
    // An L0 ledger becomes a rendered card first, so everything below this line
    // — the state substitution, `tag_notify_calls`, the render cache — sees the
    // widget DSL it already understands and needs no knowledge of L0.
    let resolved = app::l0_card::resolve_l0_blocks(cx, text, item_id);
    let text: &str = &resolved;

    // A PLAN becomes its card here, for the same reason an L0 ledger does one
    // line above: everything below this point understands only widget DSL.
    //
    // Without this a plan was lowered for *persistence* and never for display —
    // `card_splash_body` fed `save_card_artifact`, while the check below saw no
    // `runsplash` and returned the message unchanged, so the user was shown the
    // raw plan JSON as a code block. Measured on device: a correct Shanghai
    // weather plan, every section right, rendered as text.
    let planned;
    let text: &str = if text.contains("```runsplash") {
        text
    } else if let Some(body) = card_splash_body(text) {
        planned = format!("```runsplash\n{body}\n```");
        &planned
    } else {
        return text.to_string();
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("```runsplash") {
        // Copy up to and including the opening fence line verbatim.
        let after_marker = open + "```runsplash".len();
        let line_end = match rest[after_marker..].find('\n') {
            Some(nl) => after_marker + nl + 1,
            None => rest.len(),
        };
        out.push_str(&rest[..line_end]);
        let body_and_rest = &rest[line_end..];
        // Body runs to the closing fence; process only within it. The closing
        // ``` is copied verbatim by the next iteration's prefix (or the trailing
        // push below).
        match body_and_rest.find("```") {
            Some(close) => {
                let sub = substitute_card_state(&body_and_rest[..close], item_id, state);
                out.push_str(&sub);
                // Keep the closing ``` on its own line. strip_card_name_line's
                // lines()+join("\n") drops the body's trailing newline, which
                // would glue the fence onto the DSL's last brace ("}```"). That
                // is not a valid CommonMark closing fence, so pulldown-cmark
                // leaves the code block open to EOF — the Splash eval only
                // tolerates the trailing "```" by luck. Re-add the newline.
                if !sub.ends_with('\n') {
                    out.push('\n');
                }
                rest = &body_and_rest[close..];
            }
            None => {
                out.push_str(&substitute_card_state(body_and_rest, item_id, state));
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// While a reply is still streaming, hold back an UNCLOSED ```runsplash
/// block: the downstream remend pass auto-closes open fences, which would
/// dispatch every partial body to the Splash widget — a full script-VM eval
/// per repaint (observed ~60 evals for one card) and a jittering half-built
/// layout. Instead, cut the text at the open fence and show a small building
/// note; the card renders exactly once when the closing fence arrives.
fn defer_unclosed_runsplash(text: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let Some(start) = text.rfind("```runsplash") else {
        return Cow::Borrowed(text);
    };
    let after = &text[start + "```runsplash".len()..];
    let closed = match after.find('\n') {
        // Fence body present — closed iff a terminating ``` follows.
        Some(nl) => after[nl + 1..].contains("```"),
        // Mid-fence-line — certainly not closed yet.
        None => false,
    };
    if closed {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(format!("{}\u{1F6E0} Building app UI\u{2026}", &text[..start]))
    }
}

/// A semantic plan is not user-facing source code. While its fence is still
/// streaming, hide the partial JSON just like we hide a partial Splash program;
/// once closed, [`materialize_runplan_for_display`] replaces it with the card.
fn defer_unclosed_runplan(text: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let Some(start) = text.rfind("```runplan") else {
        return Cow::Borrowed(text);
    };
    let after = &text[start + "```runplan".len()..];
    let closed = match after.find('\n') {
        Some(nl) => after[nl + 1..].contains("```"),
        None => false,
    };
    if closed {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(format!("{}\u{1F6E0} Building app UI\u{2026}", &text[..start]))
    }
}

/// Replace a completed semantic-plan fence with the trusted Splash DSL lowered
/// by the runtime. The raw plan is still used by the save path before this
/// display-only conversion, so metadata and the plain-data sibling retain their
/// single semantic source of truth.
fn materialize_runplan_for_display(text: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let Some(start) = text.find("```runplan") else {
        return Cow::Borrowed(text);
    };
    let after_marker = start + "```runplan".len();
    let Some(line_end) = text[after_marker..].find('\n') else {
        return Cow::Borrowed(text);
    };
    let body_start = after_marker + line_end + 1;
    let Some(close) = text[body_start..].find("```") else {
        return Cow::Borrowed(text);
    };
    let body_end = body_start + close;
    let Ok(dsl) = crate::app::plan::lower_plan(text[body_start..body_end].trim_end()) else {
        return Cow::Borrowed(text);
    };
    let fence_end = body_end + 3;
    let mut out = String::with_capacity(text.len() + dsl.len());
    out.push_str(&text[..start]);
    out.push_str("```runsplash\n");
    out.push_str(&dsl);
    if !dsl.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```");
    out.push_str(&text[fence_end..]);
    Cow::Owned(out)
}

/// Strip Splash `//` line and `/* */` block comments and ALL whitespace,
/// producing a scan-only form. Used by the security gate so `net . http_request`,
/// `net./*x*/http_request`, and an aliased `n . http_request` all collapse to a
/// contiguous token a substring check can catch. Byte-wise (the patterns we
/// scan for are ASCII; multibyte string content only needs to not FORM one).
fn normalize_splash_for_scan(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        let c = b[i] as char;
        if !c.is_ascii_whitespace() {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Security gate: a generated card may bind live data ONLY through the `sys.*`
/// helpers and `http_resource` (GET-only image URLs). The low-level `net.*`
/// API (`net.http_request` + sockets) can POST/PUT/DELETE to arbitrary hosts —
/// an exfil / SSRF vector if a hallucinated or prompt-injected card reaches the
/// live renderer. Cards never legitimately call it, so forbid the METHOD names
/// (`.http_request`, `.socket`) on ANY receiver — that catches module aliasing
/// (`let n = net; n.http_request(...)`) too — plus the `net.HttpMethod` enum.
/// Scans the comment/whitespace-normalized body (see normalize_splash_for_scan).
///
/// A LINT, not the boundary. The boundary it used to ask for now exists: card
/// isolates are built with `script_mod_sandboxed`, so `fs`, `run`, `net` and
/// `cx.quit` are never registered and the names simply do not resolve
/// (`aichat/widgets/src/widget_async.rs`, `alloc_splash_vm`).
///
/// This is kept because a card tripping it is worth surfacing early with a
/// readable message rather than as a nil deref at eval time — and because a
/// denylist that has stopped being load-bearing is cheap to keep and expensive
/// to have removed if a future host wires the full surface back in by accident.
/// Its previous note read "NOT a hard boundary … the real fix is VM-level
/// capability gating"; that fix has landed.
fn runsplash_body_forbidden(body: &str) -> Option<&'static str> {
    let n = normalize_splash_for_scan(body).to_ascii_lowercase();
    if n.contains(".http_request")
        || n.contains(".httprequest")
        || n.contains("net.httpmethod")
        || n.contains(".socket_")
        || n.contains(".socketconnect")
    {
        return Some("card uses the low-level net API (only sys.* + http_resource are allowed)");
    }
    None
}

/// Neutralize EVERY forbidden ```runsplash block in a message (not just the
/// first — the display/store paths render all of them). Each unsafe block is
/// replaced by a plain notice; safe blocks and surrounding prose are kept
/// verbatim. Returns `Owned` iff something was blocked (the caller can use that
/// as the "message contained an unsafe card" signal). Applied on BOTH the
/// live-render path AND at store time, so a completed/hydrated message can
/// never re-surface a live forbidden fence.
fn neutralize_forbidden_cards(text: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    if !text.contains("```runsplash") {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut changed = false;
    while let Some(open) = rest.find("```runsplash") {
        let after_marker = open + "```runsplash".len();
        let line_end = match rest[after_marker..].find('\n') {
            Some(nl) => after_marker + nl + 1,
            None => rest.len(),
        };
        let body_and_rest = &rest[line_end..];
        match body_and_rest.find("```") {
            Some(close) => {
                let body = &body_and_rest[..close];
                if let Some(reason) = runsplash_body_forbidden(body) {
                    log::warn!("blocked unsafe card: {reason}");
                    out.push_str(&rest[..open]);
                    out.push_str(&format!("\u{26A0} A card was blocked: {reason}.\n"));
                    changed = true;
                    // skip past the closing fence
                    let close_abs = line_end + close;
                    let after_close = &rest[close_abs..];
                    let fence_end = after_close
                        .find('\n')
                        .map(|nl| close_abs + nl + 1)
                        .unwrap_or(rest.len());
                    rest = &rest[fence_end..];
                } else {
                    // keep this block verbatim; advance past its closing fence
                    let close_abs = line_end + close + "```".len();
                    out.push_str(&rest[..close_abs]);
                    rest = &rest[close_abs..];
                }
            }
            None => {
                // Unclosed trailing block — keep as-is (defer logic handles it).
                break;
            }
        }
    }
    out.push_str(rest);
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// Bodies of ALL ```runsplash blocks in a message (the security gate must scan
/// every one, not just the first — a safe first + unsafe second block would
/// otherwise slip through).
fn extract_all_runsplash_bodies(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("```runsplash") {
        let after = &rest[open + "```runsplash".len()..];
        let Some(body_start) = after.find('\n').map(|nl| nl + 1) else {
            break;
        };
        let body = &after[body_start..];
        match body.find("```") {
            Some(end) => {
                out.push(body[..end].trim_end());
                rest = &body[end + 3..];
            }
            None => break,
        }
    }
    out
}

/// Pull the body of the first ```runsplash fenced block out of a message so
/// it can be fed straight to a `Splash` widget. Returns the raw Splash script
/// (still containing any `{{state.*}}` placeholders).
/// Pull the body of the first ```runplan fenced block (the semantic-plan
/// substrate — typed JSON the runtime lowers into a card).
fn extract_runplan_body(text: &str) -> Option<&str> {
    if let Some(body) = fenced_body(text, "```runplan") {
        return Some(body);
    }
    // Fall back to ANY fenced block that lowers as a plan.
    //
    // The prompt demands `runplan`, and a model that emits a correct plan inside
    // a ```json fence instead had its whole answer rendered as a code block —
    // measured on device: a valid Shanghai weather plan, every section right,
    // shown to the user as raw JSON. The fence is formatting; the plan is the
    // intent, and rejecting the answer over the former helps nobody.
    //
    // Tolerant, not credulous: the body is accepted only if `lower_plan`
    // actually builds a card from it, so prose that merely contains JSON, or a
    // plan that is malformed, still falls through to being shown as text.
    for fence in ["```json", "```plan", "```"] {
        let Some(body) = fenced_body(text, fence) else {
            continue;
        };
        if crate::app::plan::lower_plan(body).is_ok() {
            return Some(body);
        }
    }
    None
}

/// The body of the first block opened by `fence`, or `None`.
fn fenced_body<'a>(text: &'a str, fence: &str) -> Option<&'a str> {
    let start = text.find(fence)?;
    let after = &text[start + fence.len()..];
    let body_start = after.find('\n')? + 1;
    let body = &after[body_start..];
    let end = body.find("```")?;
    Some(body[..end].trim_end())
}

/// The Splash body for a message: a ```runsplash block as the model wrote it, or a
/// ```runplan block LOWERED to one.
///
/// This is the seam where intent becomes realization. A plan carries only what the
/// model can be trusted with — which place, which condition, which sections — and
/// `plan::lower_plan` supplies everything else: the coordinates (geocoded, never
/// typed), the week's temperature extent, the weekday names, the font chain, the
/// layout invariants. A malformed plan is REJECTED with a message naming the field,
/// which is a far better repair target than one bad line in 16 KB of free-form DSL.
fn card_splash_body(text: &str) -> Option<String> {
    if let Some(b) = extract_runsplash_body(text) {
        return Some(b.to_string());
    }
    let plan = extract_runplan_body(text)?;
    // A plan still streaming is INCOMPLETE, not wrong. `extract_runplan_body`
    // needs a closing fence, but the tolerant any-fence fallback can match a
    // block whose JSON is still arriving, and turning that into a refusal card
    // flashes "This card was refused" mid-generation. Observed on the news
    // query: `plan is not valid JSON: EOF while parsing a list`.
    if serde_json::from_str::<serde_json::Value>(plan).is_err() {
        return None;
    }
    match crate::app::plan::lower_plan(plan) {
        Ok(dsl) => Some(dsl),
        Err(e) => {
            // Show the refusal, exactly as the L0 path does. Returning None here
            // dropped the message to ordinary prose, so a rejected plan and a
            // model that simply chose to answer in words looked identical on
            // screen -- the reason reachable only through `adb logcat`. Still
            // never a partial card: this is a whole card that says no.
            // Logged, not shown — same rule as the L0 path. A refusal is the
            // generator's problem and the reasons are unusable by whoever is
            // holding the phone.
            log::warn!("runplan rejected, not rendered: {e}");
            Some(crate::app::l0_card::error_card())
        }
    }
}

fn extract_runsplash_body(text: &str) -> Option<&str> {
    let start = text.find("```runsplash")?;
    let after = &text[start + "```runsplash".len()..];
    let body_start = after.find('\n')? + 1;
    let body = &after[body_start..];
    let end = body.find("```")?;
    Some(body[..end].trim_end())
}

/// Pull the body of the first ```runhtml fenced block out of a message (the
/// webview substrate — a complete HTML document).
fn extract_runhtml_body(text: &str) -> Option<&str> {
    let start = text.find("```runhtml")?;
    let after = &text[start + "```runhtml".len()..];
    let body_start = after.find('\n')? + 1;
    let body = &after[body_start..];
    let end = body.find("```")?;
    Some(body[..end].trim_end())
}

/// Extract the `<!-- name: <slug> -->` directive the web contract requires on
/// the first line of a runhtml card. Same slug rules as `extract_card_name`.
fn extract_html_card_name(body: &str) -> Option<String> {
    for line in body.lines().take(15) {
        let t = line.trim();
        let rest = t
            .strip_prefix("<!-- name:")
            .or_else(|| t.strip_prefix("<!--name:"));
        if let Some(rest) = rest {
            let rest = rest.trim().trim_end_matches("-->").trim();
            let slug: String = rest
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
                .collect();
            let slug = slug.trim_matches('-').to_string();
            if !slug.is_empty() {
                return Some(slug.chars().take(48).collect());
            }
        }
        // The directive lives at the very top; stop once real markup starts
        // (doctype / html / head openers are allowed through).
        let upper = t.to_ascii_uppercase();
        if t.starts_with('<')
            && !t.starts_with("<!--")
            && !upper.starts_with("<!DOCTYPE")
            && !upper.starts_with("<HTML")
        {
            break;
        }
    }
    None
}

/// Short A2App directive for follow-up requests in a session that already has
/// the Splash manual in its history (see `App::splash_primed`). Avoids
/// re-sending the ~85KB manual every turn.
fn app_splash_followup(request: &str) -> String {
    format!(
        "Respond with EXACTLY ONE ```runsplash fenced block (Makepad Splash \
syntax, no prose, no other fences), following the Splash manual already \
provided earlier in this conversation. Same rules: no imports, no \
Root/Window wrapper. FIRST line inside the block = `// name: <slug>` (reuse the \
same name when refining one of YOUR SAVED CARDS below). Each card has its OWN \
state: read `{{{{state.<key>}}}}`; \
change it with `agent.notify(\"inc\"/\"dec\"/\"reset\", {{key: \"count\"}})` for \
numbers or `agent.notify(\"set\", {{key, value}})` for strings. Internet images: \
`Image{{ src: http_resource(\"https://…\") fit: ImageFit.Smallest }}`; refreshable \
= cache-buster `?sig={{{{state.count}}}}` + a button that does `inc`. CRITICAL: begin DIRECTLY with \
a single root container widget (e.g. `RoundedView{{`) — NO top-level `let X = \
…` component definitions (inline/repeat instead); a leading `let` fails to \
render.\n\nUser request: {request}",
    )
}

app_main!(App);

/// Resolve a font file path for `role`, cfg-selected per platform. On Android we
/// read the on-device system fonts (keeps the APK lean — no bundled fonts); on
/// desktop we read the fonts from the crate's `desktop-fonts/` dir so CJK /
/// emoji / symbol text still renders. That dir is deliberately NOT under
/// `resources/` — cargo-makepad bundles the whole `resources/` tree into the
/// APK, so keeping desktop fonts out of it is what keeps the Android APK lean.
/// Used via `file_resource(#(fpath("role")))` in the theme font overrides
/// (file_resource evaluates its arg at runtime).
/// Roles: "mono_latin", "sans_latin"/"symbols" (default), "cjk", "emoji".
#[cfg(target_os = "android")]
pub(crate) fn fpath(role: &str) -> String {
    match role {
        "mono_latin" => "/system/fonts/DroidSansMono.ttf",
        "cjk" => "/system/fonts/NotoSansCJK-Regular.ttc",
        "emoji" => "/system/fonts/NotoColorEmoji.ttf",
        _ => "/system/fonts/Roboto-Regular.ttf",
    }
    .to_string()
}

#[cfg(not(target_os = "android"))]
pub(crate) fn fpath(role: &str) -> String {
    let file = match role {
        "mono_latin" => "LiberationMono-Regular.ttf",
        "cjk" => "LXGWWenKaiMono-Regular.ttf",
        "emoji" => "NotoColorEmoji.ttf",
        _ => "NotoSans-Regular.ttf",
    };
    format!("{}/desktop-fonts/{}", env!("CARGO_MANIFEST_DIR"), file)
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.CodeView
    use mod.widgets.DiagramView
    use mod.text.*
    use mod.res.*
    use mod.draw.*

    // Override theme fonts. Two purposes:
    //   1. font_code — CJK-capable monospace (LXGW Mono) so `` `inline` ``
    //      and CodeView render Chinese correctly.
    //   2. font_regular — add a symbols-capable latin (NotoSans) so Unicode
    //      blocks outside IBM Plex Sans's repertoire (arrows U+2190-U+21FF,
    //      math operators, misc technical) render as glyphs instead of tofu.
    //
    // Note: Makepad's Markdown widget bakes `theme.font_*` at expansion time,
    // so these theme-level overrides are necessary but not sufficient —
    // per-instance overrides on each Markdown instance are also applied below.
    mod.themes.dark = mod.themes.dark{
        font_code: TextStyle{
            font_size: theme.font_size_code
            font_family: FontFamily{
                latin := FontMember{res: file_resource(#(fpath("mono_latin"))) asc: 0.0 desc: 0.0}
                chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                symbols := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
            }
            line_spacing: 1.35
        }
        font_regular: mod.themes.dark.font_regular{
            font_family: FontFamily{
                latin := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                symbols := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
            }
        }
    }

    let ai_ink = #x06130F
    let ai_panel = #x0A3A30
    let ai_panel_deep = #x06251F
    let ai_cream = #xF3E3C7
    let ai_cream_dim = #xE0D2BACC
    let ai_cyan = #x72E4FF
    let ai_cyan_soft = #x72E4FF77
    let ai_gold = #xF6BE63
    let ai_gold_soft = #xF6BE6388

    let chat_scene_bg = Gradient{x1: 0 y1: 0 x2: 1 y2: 1
        Stop{offset: 0 color: #x071018 opacity: 0.56}
        Stop{offset: 0.44 color: #x121923 opacity: 0.52}
        Stop{offset: 0.72 color: #x18202B opacity: 0.48}
        Stop{offset: 1 color: #x201722 opacity: 0.52}
    }

    let chat_scene_cyan = RadGradient{cx: 0.14 cy: 0.16 r: 0.44
        Stop{offset: 0 color: #x72E4FF opacity: 0.70}
        Stop{offset: 0.44 color: #x2E84FF opacity: 0.20}
        Stop{offset: 1 color: #x2E84FF opacity: 0.0}
    }

    let chat_scene_gold = RadGradient{cx: 0.88 cy: 0.14 r: 0.36
        Stop{offset: 0 color: #xFFD18A opacity: 0.52}
        Stop{offset: 0.50 color: #xFF8F3A opacity: 0.15}
        Stop{offset: 1 color: #xFF8F3A opacity: 0.0}
    }

    let chat_scene_violet = RadGradient{cx: 0.64 cy: 0.88 r: 0.48
        Stop{offset: 0 color: #xDCA5FF opacity: 0.48}
        Stop{offset: 0.54 color: #x806DFF opacity: 0.14}
        Stop{offset: 1 color: #x806DFF opacity: 0.0}
    }

    let chat_scene_mint = RadGradient{cx: 0.28 cy: 0.76 r: 0.38
        Stop{offset: 0 color: #x8AFFD1 opacity: 0.42}
        Stop{offset: 0.48 color: #x2BD7B7 opacity: 0.12}
        Stop{offset: 1 color: #x2BD7B7 opacity: 0.0}
    }

    let ChatSceneVector = Vector{
        width: Fill
        height: Fill
        viewbox: vec4(0 0 1200 820)

        Rect{x: 0 y: 0 w: 1200 h: 820 fill: chat_scene_bg}
        Circle{cx: 160 cy: 112 r: 350 fill: chat_scene_cyan}
        Circle{cx: 1080 cy: 112 r: 290 fill: chat_scene_gold}
        Circle{cx: 768 cy: 790 r: 390 fill: chat_scene_violet}
        Circle{cx: 320 cy: 650 r: 300 fill: chat_scene_mint}

        Rect{x: 24 y: 28 w: 1152 h: 760 rx: 38 ry: 38 fill: #x07101822}
        Rect{x: 24 y: 28 w: 1152 h: 760 rx: 38 ry: 38 fill: false stroke: #xFFFFFF1A stroke_width: 1.2}
        Rect{x: 28 y: 32 w: 1144 h: 752 rx: 36 ry: 36 fill: false stroke: #x72E4FF20 stroke_width: 1.0}
        Rect{x: 42 y: 44 w: 1116 h: 724 rx: 32 ry: 32 fill: false stroke: #xFFD18A10 stroke_width: 0.8}

        Path{d: "M -80 190 C 170 72 330 120 520 70 S 905 20 1280 110" fill: false stroke: #x72E4FF22 stroke_width: 2.6 stroke_linecap: "round"}
        Path{d: "M -60 610 C 160 500 348 548 548 480 S 900 380 1260 475" fill: false stroke: #xDCA5FF1E stroke_width: 2.2 stroke_linecap: "round"}
        Path{d: "M 1120 -40 C 960 156 900 286 730 374 S 470 528 248 878" fill: false stroke: #xFFD18A1A stroke_width: 2.0 stroke_linecap: "round"}

        Rect{x: 92 y: 74 w: 320 h: 118 rx: 34 ry: 34 fill: #xFFFFFF05}
        Rect{x: 850 y: 84 w: 244 h: 88 rx: 30 ry: 30 fill: #xFFFFFF06}
        Rect{x: 470 y: 612 w: 330 h: 118 rx: 34 ry: 34 fill: #xFFFFFF05}
    }

    let ToolbarLabel = Label {
        draw_text.color: ai_cream_dim
        draw_text.text_style.font_size: 11
    }

    let ToolbarGlass = GlassPanel {
        height: 38
        flow: Right
        align: Align{y: 0.5}
        spacing: 8
        padding: Inset{left: 12 right: 12 top: 0 bottom: 0}
        draw_bg +: {
            tint_color: #x06231C
            tint_alpha: 0.88
            border_color: #x72E4FF
            border_alpha: 0.24
            border_width: 1.0
            corner_radius: 14.0
            halo_strength: 0.0
            halo_radius: 0.0
            highlight_strength: 0.10
            highlight_band_height: 18.0
            noise_strength: 0.003
        }
    }

    let PillButton = ButtonFlat {
        height: 34
        padding: Inset{left: 14 right: 14 top: 0 bottom: 0}
        draw_text +: {
            color: ai_cream
            text_style +: { font_size: 11 }
        }
        draw_bg +: {
            color: #x08251EB8
            color_hover: #x123B31DD
            border_color: #xEAD8B82D
            border_size: 1.0
            border_radius: 10.0
        }
    }

    let IconButton = ButtonFlat {
        width: 36
        height: 36
        padding: 0
        draw_text +: {
            color: ai_cream
            text_style +: { font_size: 15 }
        }
        draw_bg +: {
            color: #x08251EB0
            color_hover: #x154337DD
            border_color: #xEAD8B82A
            border_size: 1.0
            border_radius: 10.0
        }
    }

    let SendButton = ButtonFlatIcon {
        width: 36
        height: 36
        padding: 0
        icon_walk: Walk{ width: 20, height: 20 }
        draw_icon +: {
            color: ai_gold
            svg: crate_resource("self:resources/icons/send.svg")
        }
        // Flat icon button — no filled circle behind the send glyph.
        draw_bg +: {
            color: #00000000
            color_hover: #xEAD8B814
            border_size: 0.0
            border_radius: 8.0
        }
    }

    let GlassSlider = SliderMinimal {
        width: 170
        height: 28
        text: ""
        min: 0.72
        max: 0.98
        step: 0.01
        default: 0.90
        precision: 2
        label_walk: Walk{width: 0 height: 0}
        text_input: TextInput{
            width: 0
            height: 0
            is_read_only: true
        }
        draw_bg +: {
            hover: instance(0.0)
            focus: instance(0.0)
            drag: instance(0.0)
            disabled: instance(0.0)
            border_size: 0.0
            offset_y: 11.0
            handle_size: 20.0
            color: #x9CC9C24A
            color_hover: #x9CC9C266
            color_focus: #x9CC9C266
            color_drag: #x9CC9C280
            color_2: #x0A241EAA
            color_2_hover: #x0E3028CC
            color_2_focus: #x0E3028CC
            color_2_drag: #x123C32DD
            val_color: ai_gold
            val_color_hover: #xFFD98B
            val_color_focus: #xFFD98B
            val_color_drag: #xFFE2A3
            handle_color: ai_gold
            handle_color_hover: #xFFF0D2
            handle_color_focus: #xFFF0D2
            handle_color_drag: #xFFF0D2
            border_color: #x72E4FF44
            border_color_2: #x00000055
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let track_y = self.rect_size.y * 0.5 - 2.0
                let track_h = 4.0
                let handle_x = clamp(
                    self.slide_pos * self.rect_size.x,
                    8.0,
                    self.rect_size.x - 8.0
                )
                let handle_r = 8.0 + self.hover * 1.0

                sdf.box(0.0, track_y, self.rect_size.x, track_h, 2.0)
                sdf.fill(#x6EA99E66)

                sdf.box(0.0, track_y, handle_x, track_h, 2.0)
                sdf.fill(self.val_color.mix(self.val_color_hover, self.hover))

                sdf.circle(handle_x, self.rect_size.y * 0.5, handle_r)
                sdf.fill_keep(self.handle_color.mix(self.handle_color_hover, self.hover))
                sdf.stroke(#xFFF0D288, 1.0)

                return sdf.result
            }
        }
    }

    let MermaidSvgView = #(MermaidSvgView::register_widget(vm)) {
        width: Fill
        height: Fit
        // Animated flow dot shader: SDF circle + halo. Per-edge color
        // (incl. pulse alpha in `.w`) is written from Rust.
        draw_flow_dot +: {
            color: #xe2e8f0
            pixel: fn() {
                let r = length(self.pos - vec2(0.5, 0.5))
                let core = 1.0 - smoothstep(0.30, 0.38, r)
                let halo = (1.0 - smoothstep(0.38, 0.50, r)) * 0.55
                let a = clamp(core + halo, 0.0, 1.0) * self.color.w
                return Pal.premul(vec4(self.color.xyz, a))
            }
        }
        draw_text +: {
            color: #xe2e8f0
            text_style: theme.font_code{
                font_size: 12
                font_family: FontFamily{
                    latin := FontMember{res: file_resource(#(fpath("mono_latin"))) asc: 0.0 desc: 0.0}
                    chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                    emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
                }
            }
        }
    }

    let ChatList = #(ChatList::register_widget(vm)) {
        width: Fill
        height: Fill

        list := PortalList {
            width: Fill
            height: Fill
            flow: Down
            // Weather app: this list scrolls the single (taller-than-screen) newest
            // card. The fork's PortalList now clamps first_id into the active range every
            // draw (draw_align_list.retain + first_id clamp), so the top-clamp always
            // engages and neither a drag NOR a fling can scroll the card off the top into
            // blank space — it rubber-bands back. `selectable` off so a text drag scrolls
            // instead of selecting (the per-answer copy icon covers extraction on mobile).
            drag_scrolling: true
            // Fling momentum: the default scaling (0.005) barely moves for the velocity
            // our touch sampling reports, so a flick crawls. Boost it + raise the cap so
            // one flick glides across most of the card. (The fork clamp keeps it from
            // escaping the top no matter how hard the fling.)
            flick_scroll_scaling: 0.015
            flick_scroll_maximum: 150.0
            // NO auto/smooth tail: this list shows one tall card that must rest at its
            // TOP (the hero temperature), iOS-Weather style, and scroll DOWN to details.
            // Tailing pulls it to the bottom (grid) and — with smooth_tail — springs any
            // scroll-up back down, making the hero unreachable. The newest card is shown
            // at its top by the explicit pin (set_first_id_and_scroll(newest, 0.0)) below.
            auto_tail: false
            smooth_tail: false
            selectable: false
            // Hide the right-edge scrollbar (drag-to-scroll is the gesture).
            scroll_bar: mod.widgets.ScrollBar { bar_size: 0.0 }

            User := RoundedView {
                width: Fill
                height: Fit
                // Full page width (was left:50 chat-bubble indent).
                margin: Inset{top: 4 bottom: 4 left: 8 right: 8}
                padding: Inset{left: 12 top: 8 right: 12 bottom: 8}
                flow: Down
                show_bg: true
                draw_bg +: {
                    color: #x0B2A22E6
                    radius: 12.0
                }

                selectable := Markdown {
                    width: Fill
                    height: Fit
                    // Off on mobile: per-widget text selection fought the
                    // list's drag-to-scroll (a swipe popped Android's
                    // Copy/Cut toolbar mid-scroll). Copy icon covers this.
                    selectable: false
                    use_code_block_widget: true
                    use_math_widget: true
                    body: ""
                    // Per-instance override for `` `inline code` ``. The
                    // Markdown widget bakes `theme.font_code` at expansion
                    // time, so a later `mod.themes.dark{...}` override
                    // doesn't reach it. Without this override, CJK inside
                    // backticks renders as tofu (no glyph) because Liberation
                    // Mono is Latin-only.
                    text_style_fixed: theme.font_code{
                        font_family: FontFamily{
                            latin := FontMember{res: file_resource(#(fpath("mono_latin"))) asc: 0.0 desc: 0.0}
                            chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                            symbols := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                            emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
                        }
                    }
                    // Prose font family with symbols fallback — fixes "tofu"
                    // for Unicode arrows / math / misc technical symbols
                    // (observed trigger: `1→5`, `≤`, `≥`, `α` in prose).
                    text_style_normal: theme.font_regular{
                        font_family: FontFamily{
                            latin := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                            chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                            symbols := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                            emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
                        }
                    }
                    code_block := ScrollXView {
                        width: Fill
                        height: Fit
                        flow: Right
                        code_view := CodeView {
                            keep_cursor_at_end: false
                            editor +: {
                                height: Fit
                                draw_bg +: { color: #x031510EE }
                            }
                        }
                    }
                    splash_block := View {
                        width: Fill
                        height: Fit
                        splash_view := Splash {
                            width: Fill
                            height: Fit
                        }
                    }
                    web_block := View {
                        width: Fill
                        height: 1900
                        web_view := WebCard {
                            width: Fill
                            height: Fill
                        }
                    }
                    // Diagram block — rendered by makepad-diagram-kit's
                    // DiagramView. The inner `diagram_view` id matches what
                    // the markdown widget's `ids!(diagram_view).set_text`
                    // dispatch expects.
                    diagram_block := ScrollXView {
                        width: Fill
                        height: Fit
                        flow: Right
                        diagram_view := DiagramView {
                            width: Fit
                            height: Fit
                        }
                    }
                    mermaid_block := ScrollXView {
                        width: Fill
                        height: Fit
                        flow: Right
                        mermaid_view := MermaidSvgView {
                            width: Fit
                            height: Fit
                        }
                    }
                    inline_math := MathView {
                        // MathView lays out at font_size*1.75; body is ~10,
                        // so 5.7 keeps inline math the same height as text.
                        font_size: 5.7
                    }
                    display_math := MathView {
                        font_size: 6.3
                    }
                }

                // (Per-message close button removed — user directive.)
                View {
                    width: Fill
                    height: Fit
                    align: Align{x: 1.0}
                }
            }

            Assistant := RoundedView {
                width: Fill
                height: Fit
                // Edge-to-edge: no bubble margin/padding/background so the A2App
                // card fills the entire screen (was margin 8 / padding 12 with a
                // dark bubble bg — that framed the card and broke full-screen).
                margin: Inset{top: 0 bottom: 0 left: 0 right: 0}
                padding: Inset{left: 0 top: 0 right: 0 bottom: 0}
                flow: Down
                // OPAQUE BLACK backing for the whole card item. A full-screen
                // A2App card is a translucent scrim over a photo; wherever the
                // photo doesn't cover (an offset above the image, a not-yet-
                // loaded texture), the scrim would otherwise reveal the
                // uninitialized Android surface as BRIGHT RED. An opaque black
                // bubble guarantees those regions read black, not red.
                show_bg: true
                draw_bg +: {
                    color: #x000000FF
                    radius: 0.0
                }

                RubberView {
                    width: Fill
                    height: Fit
                    smoothing: 0.3

                    selectable := Markdown {
                        width: Fill
                        height: Fit
                        // Off on mobile — see User bubble note (drag scrolls,
                        // copy icon extracts).
                        selectable: false
                        use_code_block_widget: true
                        use_math_widget: true
                        body: ""
                        // Per-instance override — same as User's Markdown
                        // above. Fixes `` `中文` `` inline-code tofu.
                        text_style_fixed: theme.font_code{
                            font_family: FontFamily{
                                latin := FontMember{res: file_resource(#(fpath("mono_latin"))) asc: 0.0 desc: 0.0}
                                chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                                emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
                            }
                        }
                        draw_text +: {
                            get_color: fn() {
                                let fade_chars = 50.0
                                let dist_from_end = self.total_chars - self.char_index
                                let t = clamp(dist_from_end / fade_chars, 0.0, 1.0)
                                let alpha = pow(t, 0.5)
                                return vec4(self.color.rgb, self.color.a * alpha)
                            }
                        }
                        code_block := ScrollXView {
                            width: Fill
                            height: Fit
                            flow: Right
                            code_view := CodeView {
                                keep_cursor_at_end: true
                                editor +: {
                                    height: Fit
                                    draw_bg +: { color: #x031510EE }
                                    // Local font override: CodeView is defined in the
                                    // makepad-code-editor crate and bakes `theme.font_code`
                                    // at its own expansion time, so later `mod.themes.dark`
                                    // overrides don't reach it. Override per-instance.
                                    draw_text +: {
                                        text_style: theme.font_code{
                                            font_family: FontFamily{
                                                latin := FontMember{res: file_resource(#(fpath("mono_latin"))) asc: 0.0 desc: 0.0}
                                                chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                                                emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
                                            }
                                        }
                                    }
                                    draw_gutter +: {
                                        text_style: theme.font_code{
                                            font_family: FontFamily{
                                                latin := FontMember{res: file_resource(#(fpath("mono_latin"))) asc: 0.0 desc: 0.0}
                                                chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                                                emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // TEXTURE-CACHED so the tall (~1200dp) weather card scrolls
                        // smoothly. PortalList bakes scroll into each item's absolute
                        // position and re-walks every visible item every frame; an
                        // un-cached card therefore re-shapes ~55 CJK/emoji labels per
                        // scroll frame (~45ms → ~22fps). CachedView renders the card
                        // ONCE into an offscreen texture and, on a position-only change
                        // (scroll), re-blits that bitmap at the new rect (~60fps).
                        //
                        // The card is TALLER than the viewport, so earlier attempts baked
                        // BLACK into the off-screen part (a re-render while scrolled — e.g.
                        // when a map image lands — inherited the PortalList's viewport clip).
                        // Fixed in the fork: View::draw_walk's Texture arm now closes its
                        // offscreen turtle with `end_texture_turtle_with_area`, an un-clipped
                        // pass turtle that clips only to the card's OWN bounds, so the FULL
                        // card always lands in the texture (see aichat/draw/src/turtle.rs).
                        // LOCAL DEBUG: CachedView's offscreen pass never sizes on the
                        // emulator (setup_render_pass rect < 0.5 → skipped, paint_dirty
                        // already cleared → never retried). Un-cache to prove the theory.
                        splash_block := View{
                            flow: Overlay
                            width: Fill
                            height: Fit
                            // OPAQUE BLACK backing. The offscreen pass clears to transparent
                            // black, so any pixel the card leaves unpainted (e.g. an Image
                            // letterbox) would otherwise composite the chat background through
                            // the blit. CachedView itself can't use show_bg (its draw_bg drives
                            // the texture sampler), so the backing lives as a CHILD SolidView
                            // drawn first (behind), guaranteeing clean black letterboxing.
                            splash_backing := SolidView{
                                flow: Overlay
                                width: Fill
                                height: Fit
                                draw_bg.color: #000000FF
                                splash_view := Splash {
                                    flow: Overlay
                                    width: Fill
                                    height: Fit
                                }
                            }
                        }
                        web_block := View{
                            width: Fill
                            height: 1900
                            web_view := WebCard{
                                width: Fill
                                height: Fill
                            }
                        }
                        // Diagram block — see User-side comment.
                        diagram_block := ScrollXView{
                            flow: Right
                            new_batch: true
                            width: Fill
                            height: Fit
                            diagram_view := DiagramView {
                                width: Fit
                                height: Fit
                            }
                        }
                        mermaid_block := ScrollXView{
                            flow: Right
                            new_batch: true
                            width: Fill
                            height: Fit
                            mermaid_view := MermaidSvgView {
                                width: Fit
                                height: Fit
                            }
                        }
                        inline_math := MathView {
                            // Match body text height (font_size*1.75 ≈ body).
                            font_size: 5.7
                        }
                        display_math := MathView {
                            font_size: 6.3
                        }
                    }
                }

                // Answer action row: copy + share, drawn natively from the
                // supplied SVGs via each button's DrawSvg icon slot.
                // `draw_icon.color` overrides the SVG `currentColor`. Both are
                // gated off until the answer completes (draw loop hides them
                // on the in-flight item). Flat transparent button bg.
                actions_row := View {
                    width: Fill
                    height: Fit
                    flow: Right
                    align: Align{x: 0.0 y: 0.5}
                    spacing: 2
                    copy_button := ButtonFlatIcon {
                        width: 34
                        height: 27
                        margin: Inset{top: 6 left: 2}
                        icon_walk: Walk{ width: 19, height: 19 }
                        draw_icon +: {
                            color: #xB6C6BE
                            svg: crate_resource("self:resources/icons/copy.svg")
                        }
                        draw_bg +: {
                            color: #00000000
                            color_hover: #xEAD8B814
                            border_size: 0.0
                            border_radius: 8.0
                        }
                    }
                    share_button := ButtonFlatIcon {
                        width: 34
                        height: 27
                        margin: Inset{top: 6}
                        icon_walk: Walk{ width: 19, height: 19 }
                        draw_icon +: {
                            color: #xB6C6BE
                            svg: crate_resource("self:resources/icons/share.svg")
                        }
                        draw_bg +: {
                            color: #00000000
                            color_hover: #xEAD8B814
                            border_size: 0.0
                            border_radius: 8.0
                        }
                    }
                }
            }
        }
    }

    // SessionList — sidebar pane backed by `octos_app_store::SessionMap`.
    // Replaces W02's static `nav_recent` placeholder; see
    // `app/src/app/sessions.rs`. Pattern lifted from
    // `aichat/examples/aichat/src/main.rs:343` (ChatList DSL) and `:1774-1881`
    // (Widget impl); item template models the row design in
    // `04-IA-AND-NAVIGATION.md` § Sidebar with `octos-web/src/components/session-list.tsx`'s
    // hover-x affordance.
    let SessionList = #(crate::app::sessions::SessionList::register_widget(vm)) {
        width: Fill
        height: Fit

        list := PortalList {
            width: Fill
            height: Fill
            flow: Down
            drag_scrolling: true
            auto_tail: false
            selectable: true

            SessionItem := RoundedView {
                width: Fill
                height: Fit
                flow: Right
                margin: Inset{top: 2 bottom: 2 left: 0 right: 0}
                padding: Inset{left: 6 top: 6 right: 6 bottom: 6}
                spacing: 6
                align: Align{y: 0.5}
                show_bg: true
                draw_bg +: {
                    color: #x0A2A2200
                    color_hover: #xEAD8B814
                    radius: 8.0
                }

                // Streaming / active-task dot. Hidden by Rust when neither
                // flag is set; see `octos_app_store::sessions::is_session_active`.
                streaming_dot := Label {
                    width: Fit
                    height: Fit
                    text: "●"
                    margin: Inset{right: 2}
                    draw_text.color: #x72E4FF
                    draw_text.text_style.font_size: 10
                }

                // Selection caret — Rust toggles visibility when this row's
                // id matches `APP_STATE.current_session`.
                selected_marker := Label {
                    width: Fit
                    height: Fit
                    text: "▸"
                    margin: Inset{right: 2}
                    draw_text.color: #xF6BE63
                    draw_text.text_style.font_size: 10
                }

                // The row's click target doubles as its title: Buttons render
                // only their OWN text (child Labels nested inside a Button
                // are never drawn — Button::draw_walk paints bg/icon/text and
                // stops), so `row_click.text` carries the session title,
                // set from `SessionList::draw_walk`.
                row_click := ButtonFlat {
                    width: Fill
                    height: Fit
                    align: Align{x: 0.0 y: 0.5}
                    padding: Inset{left: 2 top: 4 right: 2 bottom: 4}
                    text: ""
                    draw_text +: {
                        color: #xF3E3C7
                        text_style +: { font_size: 12 }
                    }
                    draw_bg +: {
                        color: #00000000
                        color_hover: #xEAD8B810
                        border_size: 0.0
                        border_radius: 6.0
                    }
                }

                delete_button := ButtonFlat {
                    width: Fit
                    height: Fit
                    padding: Inset{top: 2 bottom: 2 left: 6 right: 6}
                    margin: Inset{left: 2}
                    text: "x"
                    draw_text +: {
                        color: #xCDBF9F66
                        text_style +: { font_size: 10 }
                    }
                    draw_bg +: {
                        color: #00000000
                        color_hover: #xEAD8B822
                        border_size: 0.0
                        border_radius: 6.0
                    }
                }
            }
        }
    }

    // W04 / M2 — DockRow prototype. Pulled to script_mod top level so the
    // `row_0..row_7 := DockRow {}` slots inside TaskDock's expanded body can
    // reference it. Defining `DockRow := View { ... }` *inside* TaskDock's
    // body created an instance child named `DockRow`, not a reusable
    // prototype, so the eight `row_N := DockRow {}` lookups crashed at live
    // eval with `variable DockRow not found in scope`. Mirrors the
    // `let RiskBadge = ...` pattern in `app/src/app/approvals.rs`.
    let DockRow = View {
        width: Fill
        height: Fit
        flow: Right
        spacing: 8
        align: Align{y: 0.5}
        padding: Inset{left: 4 top: 2 right: 4 bottom: 2}

        row_icon := Label {
            width: 18
            text: "🔧"
            draw_text.color: #xF6BE63
            draw_text.text_style.font_size: 12
        }
        row_name := Label {
            width: Fill
            text: ""
            draw_text.color: ai_cream
            draw_text.text_style.font_size: 11
        }
        row_status := Label {
            width: Fit
            text: ""
            draw_text.color: #x72E4FF
            draw_text.text_style.font_size: 10
        }
        row_detail := Label {
            width: Fit
            text: ""
            visible: false
            draw_text.color: #xCDBF9F88
            draw_text.text_style.font_size: 10
            margin: Inset{left: 6}
        }
    }

    // W04 / M2 — TaskDock under the chat composer. Reads `APP_STATE.tool_calls`
    // and `APP_STATE.tasks` on each draw; the OctosUiAgent drains
    // `tool/*` and `task/*` notifications into the store
    // (`app/src/backend/octos_ui.rs::fold_into_store`). The Rust impl is in
    // `app/src/app/task_dock.rs`; this DSL block declares the visual layout.
    let TaskDock = #(crate::app::task_dock::TaskDock::register_widget(vm)) {
        width: Fill
        height: Fit
        flow: Down
        margin: Inset{left: 92 right: 92 top: 4 bottom: 0}
        spacing: 4

        header_row := View {
            width: Fill
            height: Fit
            flow: Right
            align: Align{y: 0.5}
            spacing: 6

            chevron := Label {
                width: Fit
                height: Fit
                text: "▸"
                draw_text.color: #xF6BE63
                draw_text.text_style.font_size: 11
            }

            // Pill behind the chevron + label. Click anywhere toggles the
            // expanded body (Rust handles the action).
            header_pill := ButtonFlat {
                width: Fill
                height: 26
                align: Align{x: 0.0 y: 0.5}
                padding: Inset{left: 10 right: 10}
                text: "🔧 0 tools · 0 tasks · 0% running"
                draw_text +: {
                    color: ai_cream
                    text_style +: { font_size: 11 }
                }
                draw_bg +: {
                    color: #x0A2E26C8
                    color_hover: #x123E32DD
                    border_color: #x72E4FF44
                    border_size: 1.0
                    border_radius: 12.0
                }
            }
        }

        // Expanded-state body. Visibility flipped from Rust on toggle. The
        // outer `RubberView` smoothes the height transition on expand /
        // collapse — same trick aichat uses for the streaming-markdown
        // assistant body (`aichat:480`, smoothing 0.3).
        body := RubberView {
            width: Fill
            height: Fit
            smoothing: 0.3
            visible: false
            margin: Inset{top: 4}
            padding: Inset{left: 10 top: 8 right: 10 bottom: 8}
            spacing: 4
            show_bg: true
            draw_bg +: {
                color: #x062821CC
                radius: 10.0
            }

            row_0 := DockRow {}
            row_1 := DockRow {}
            row_2 := DockRow {}
            row_3 := DockRow {}
            row_4 := DockRow {}
            row_5 := DockRow {}
            row_6 := DockRow {}
            row_7 := DockRow {}

            overflow := Label {
                width: Fill
                height: Fit
                text: ""
                visible: false
                margin: Inset{top: 4}
                draw_text.color: #xCDBF9FAA
                draw_text.text_style.font_size: 10
            }
        }
    }

    // W07 / M3 — Studio / Slides / Sites producer screens (DSL inline,
    // Rust impl at `app/src/app/producers.rs`). Mirrors the SessionList
    // / TaskDock pattern. The chat pane in each triptych embeds the
    // local `ChatList` binding directly, satisfying W07's "the chat
    // thread inside each producer MUST be the same `ChatList` widget".

    let ProducerHeading = Label {
        width: Fill height: Fit margin: Inset{top: 0 bottom: 4 left: 2 right: 2}
        draw_text.color: #xCDBF9FA0 draw_text.text_style.font_size: 11
    }

    let GenerationCard = #(crate::app::producers::GenerationCardWidget::register_widget(vm)) {
        width: Fill height: Fit flow: Down spacing: 4 show_bg: true
        margin: Inset{top: 3 bottom: 3 left: 4 right: 4}
        padding: Inset{left: 10 top: 8 right: 10 bottom: 8}
        draw_bg +: { color: #x0A2A22DD radius: 10.0 }

        gen_header := View {
            width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 6
            gen_kind_label := Label {
                width: Fit text: ""
                draw_text.color: #x72E4FF draw_text.text_style.font_size: 10
            }
            View { width: Fill height: 1 }
            gen_open_button := ButtonFlat {
                width: Fit height: 22 text: "Open"
                padding: Inset{left: 8 right: 8}
                draw_text +: {
                    color: #xF3E3C7
                    text_style +: { font_size: 10 }
                }
                draw_bg +: {
                    color: #x08251EC8 color_hover: #x123B31DD
                    border_color: #xEAD8B83A border_size: 1.0 border_radius: 8.0
                }
            }
        }
        gen_title_label := Label {
            width: Fill height: Fit text: ""
            draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 12
        }
    }

    // Shared body for the three producer screens. Used via `..ProducerBody{}`
    // spread in each `mod.widgets.{Studio,Slides,Sites}Screen` below.
    let ProducerBody = View {
        width: Fill height: Fill flow: Down spacing: 8

        producer_header := View {
            width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 6
            producer_title := Label {
                width: Fit text: ""
                draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 16
            }
            producer_subtitle := Label {
                width: Fit text: ""
                margin: Inset{left: 8}
                draw_text.color: #xCDBF9F88 draw_text.text_style.font_size: 11
            }
        }

        producer_body := View {
            width: Fill height: Fill flow: Right spacing: 12

            source_pane := View {
                width: 320 height: Fill flow: Down spacing: 6
                ProducerHeading { text: "Sources" }
                source_input := TextInput {
                    width: Fill height: 56 empty_text: "URL, pasted text, or PDF reference"
                    draw_bg +: {
                        color: #x06241DCC color_hover: #x0A2D24DD color_focus: #x0F362DEE
                        color_empty: #x06241DCC border_color: #x72E4FF44
                        border_size: 1.0 border_radius: 10.0
                    }
                    draw_text +: {
                        color: #xF3E3C7 color_empty: #xF3E3C766
                        text_style +: { font_size: 12 }
                    }
                }
                add_source_button := ButtonFlat {
                    width: Fill height: 32 text: "+ Add Source"
                    padding: Inset{left: 12 right: 12}
                    draw_text +: { color: #xF3E3C7 text_style +: { font_size: 11 } }
                    draw_bg +: {
                        color: #x08251EC8 color_hover: #x123B31DD
                        border_color: #xEAD8B83A border_size: 1.0 border_radius: 10.0
                    }
                }
                source_divider := SolidView {
                    width: Fill height: 1 margin: Inset{top: 4 bottom: 4}
                    draw_bg.color: #xEAD8B81C
                }
                source_list_heading := ProducerHeading { text: "Added" }
                source_list := PortalList {
                    width: Fill height: Fill flow: Down
                    drag_scrolling: true auto_tail: false
                    SourceRow := RoundedView {
                        width: Fill height: Fit
                        margin: Inset{top: 2 bottom: 2 left: 0 right: 0}
                        padding: Inset{left: 8 top: 5 right: 8 bottom: 5}
                        show_bg: true
                        draw_bg +: { color: #x06231CCC radius: 6.0 }
                        source_text_label := Label {
                            width: Fill text: ""
                            draw_text.color: #xCDBF9FCC
                            draw_text.text_style.font_size: 11
                        }
                    }
                }
                source_empty := Label {
                    width: Fill height: Fit
                    text: "No sources yet."
                    visible: true
                    margin: Inset{top: 6}
                    draw_text.color: #xCDBF9F77
                    draw_text.text_style.font_size: 11
                }
            }

            // Per W07 brief: reuse the W03 `ChatList` widget directly.
            // The chat thread is per-project — switching projects swaps
            // `APP_STATE.current_session` so this re-mounts cleanly.
            chat_pane := View {
                width: Fill height: Fill flow: Down spacing: 4
                ProducerHeading { text: "Chat" }
                producer_chat_list := ChatList {}
            }

            output_pane := View {
                width: 360 height: Fill flow: Down spacing: 6
                ProducerHeading { text: "Generations" }
                output_list := PortalList {
                    width: Fill height: Fill flow: Down
                    drag_scrolling: true auto_tail: false
                    GenRow := GenerationCard {}
                }
                output_empty := View {
                    width: Fill height: Fit flow: Down align: Align{x: 0.5 y: 0.5}
                    margin: Inset{top: 24} visible: true
                    Label {
                        text: "Generation history will appear here"
                        draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 12
                    }
                    Label {
                        text: "(server producer tools land in the next slice)"
                        draw_text.color: #xCDBF9F77 draw_text.text_style.font_size: 10
                        margin: Inset{top: 4}
                    }
                }
            }
        }
    }

    // StudioScreen / SlidesScreen / SitesScreen templates removed —
    // unsupported in this build (their widgets remain in `producers.rs`).

    startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                show_caption_bar: false
                // Opaque black, NOT transparent: with a transparent clear, any
                // pixel no opaque widget covers shows the uninitialized Android
                // surface, which reads as BRIGHT RED on this device — seen as
                // red bands wherever a generated card didn't fully cover.
                // window.transparent MUST be false on Android: a transparent
                // window forces the EGL clear alpha to 0, so the opaque
                // clear_color below is ignored and the status-bar / notch
                // safe-area strip (no opaque widget covers it) shows the
                // uninitialized surface as a RED band. Transparency was only
                // needed for the macOS backdrop blur, which is disabled.
                pass.clear_color: #000000FF
                window.transparent: false
                // window.backdrop: WindowBackdrop.Blur — disabled until
                // platform bug fixed in macos_window.rs:532 (addSubview
                // positioned arg must be NSWindowBelow/-1 or NSWindowAbove/1,
                // not 0). See issues/aichat-liquid-glass-backdrop-platform-bug.md
                window.macos: MacosWindowConfig{chrome: MacosWindowChrome.Borderless resizable: true}
                window.inner_size: vec2(900, 700)
                window.title: " "
                body +: {
                    flow: Overlay
                    padding: 3
                    spacing: 0
                    draw_bg.color: #00000000

                    app_shell := GlassPanel {
                        width: Fill
                        height: Fill
                        new_batch: true
                        flow: Right
                        // Edge-to-edge: no frame inset so the A2App card fills
                        // the whole screen.
                        padding: Inset{left: 0 top: 0 right: 0 bottom: 0}
                        spacing: 0
                        draw_bg +: {
                            tint_color: #x0D4035
                            tint_alpha: 0.66
                            border_color: ai_cyan
                            border_alpha: 0.38
                            border_width: 1.0
                            corner_radius: 10.0
                            halo_color: ai_cyan
                            halo_strength: 0.0
                            halo_radius: 0.0
                            highlight_strength: 0.28
                            highlight_band_height: 58.0
                            chroma_strength: 0.0
                            noise_strength: 0.004
                        }

                    sidebar := GlassPanel {
                        width: 298
                        height: Fill
                        // Removed from the product: the session list pane is
                        // desktop shell furniture, and on device it covered
                        // the AMA surface whenever the window was not narrow
                        // (rotate to landscape and half the screen was list).
                        // The pane and its toggle stay in the tree for the
                        // desktop shell work to revive, but nothing shows it.
                        visible: false
                        new_batch: true
                        flow: Down
                        padding: Inset{left: 14 top: 14 right: 14 bottom: 14}
                        spacing: 10
                        draw_bg +: {
                            tint_color: #x0A3A30
                            tint_alpha: 0.78
                            border_color: #xEAD8B8
                            border_alpha: 0.20
                            border_width: 0.0
                            corner_radius: 0.0
                            halo_strength: 0.0
                            halo_radius: 0.0
                            highlight_strength: 0.16
                            highlight_band_height: 54.0
                            chroma_strength: 0.0
                            noise_strength: 0.004
                        }

                        sidebar_header := View {
                            width: Fill
                            height: Fit
                            flow: Down
                            spacing: 8
                            margin: Inset{top: 4 bottom: 18}

                            View {
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: 10
                                align: Align{y: 0.5}

                                Label {
                                    text: "AI"
                                    draw_text.color: ai_cyan
                                    draw_text.text_style.font_size: 14
                                }

                                Label {
                                    text: "Octos"
                                    draw_text.color: ai_cream
                                    draw_text.text_style.font_size: 15
                                }
                            }

                            Label {
                                text: "Diagram workspace"
                                draw_text.color: ai_cream_dim
                                draw_text.text_style.font_size: 11
                            }
                        }

                        nav_new := ButtonFlat {
                            width: Fill
                            height: 38
                            text: "+  新对话"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 14 right: 12}
                            draw_text +: {
                                color: ai_cream
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #x0B6B67AA
                                color_hover: #x108E88CC
                                border_color: #x72E4FF66
                                border_size: 1.0
                                border_radius: 10.0
                            }
                        }

                        nav_search := ButtonFlat {
                            width: Fill
                            height: 27
                            text: "⌕  搜索"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xE4D4B6
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        nav_plugins := ButtonFlat {
                            width: Fill
                            height: 27
                            text: "⌘  插件"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xE4D4B6
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        nav_automation := ButtonFlat {
                            width: Fill
                            height: 27
                            text: ">  自动化"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xE4D4B6
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        // W04 / M2 — Content nav button. Replaces the
                        // inactive `nav_project` placeholder per
                        // `04-IA-AND-NAVIGATION.md` § Top-level shell
                        // ("Content" sidebar item). Click dispatches
                        // through App::handle_actions to flip
                        // `APP_STATE.navigation` to `CurrentScreen::Content`.
                        nav_content := ButtonFlat {
                            width: Fill
                            height: 27
                            text: "📚  内容"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xE4D4B6
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        // Coding / Studio / Slides / Sites navs removed —
                        // not supported in this build (user directive). The
                        // screens' widget modules stay registered for when
                        // the server-side tools land.

                        Label {
                            text: "对话"
                            margin: Inset{top: 28 bottom: 2 left: 0 right: 0}
                            draw_text.color: #xCDBF9FA0
                            draw_text.text_style.font_size: 12
                        }

                        // W04 — live session list. Empty until a server is
                        // connected; `App::handle_startup` calls
                        // `crate::app::sessions::hydrate_sessions` once the
                        // RestClient is ready. Click selects, x deletes.
                        session_list := SessionList {
                            width: Fill
                            height: Fill
                        }

                        settings_button := ButtonFlat {
                            width: Fill
                            height: 32
                            text: "*  设置"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xF3E3C7
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        // W08 — Sign-out link. On click `App::handle_actions`
                        // wipes the keychain entry for `(host, profile_id)`,
                        // clears the in-memory auth slice, and flips the
                        // login_overlay back on.
                        sign_out_button := ButtonFlat {
                            width: Fill
                            height: 28
                            text: "↪  退出登录"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xCDBF9F88
                                text_style +: { font_size: 11 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }
                    }

                    SolidView {
                        width: 1
                        height: Fill
                        draw_bg.color: #xEAD8B81E
                    }

                    main_area := GlassPanel {
                        width: Fill
                        height: Fill
                        new_batch: true
                        flow: Down
                        // Edge-to-edge full-screen card: zero padding.
                        padding: Inset{left: 0 top: 0 right: 0 bottom: 0}
                        spacing: 0
                        draw_bg +: {
                            tint_color: #x0B3B31
                            tint_alpha: 0.70
                            border_color: #xEAD8B8
                            border_alpha: 0.16
                            border_width: 0.0
                            corner_radius: 0.0
                            halo_strength: 0.0
                            halo_radius: 0.0
                            highlight_strength: 0.16
                            highlight_band_height: 56.0
                            chroma_strength: 0.0
                            noise_strength: 0.004
                        }

                        // Layer 3 (W08) — the multi-app switcher moved INTO the
                        // native composer pill (＋ new app, ⟳ switch). The screen
                        // is otherwise just the full-screen a2app card — no top
                        // chrome (see `handle_actions` NativeComposerNewApp/Switch).

                        top_bar := View {
                            width: Fill
                            height: 40
                            flow: Right
                            align: Align{y: 0.5}
                            // Minimalist full-screen A2App: no header chrome.
                            visible: false

                            // Phone: the sidebar auto-collapses after nav
                            // clicks on narrow windows; this brings it back.
                            nav_toggle := ButtonFlat {
                                width: 34
                                height: 27
                                text: "☰"
                                margin: Inset{right: 8}
                                align: Align{x: 0.5 y: 0.5}
                                draw_text +: {
                                    color: #xE4D4B6
                                    text_style +: { font_size: 14 }
                                }
                                draw_bg +: {
                                    color: #00000000
                                    color_hover: #xEAD8B814
                                    border_size: 0.0
                                    border_radius: 8.0
                                }
                            }

                            Label {
                                text: "Octos"
                                draw_text.color: ai_cream
                                draw_text.text_style.font_size: 14
                            }

                            // W04 follow-up #3 — connection state dot.
                            // Colour is updated from `App::update_connection_indicator`
                            // by re-evaluating the label's color: green = Live,
                            // amber = Reconnecting, red = Offline / Failed.
                            connection_dot := Label {
                                text: "●"
                                margin: Inset{left: 8 right: 4}
                                draw_text.color: #x6F8F6F
                                draw_text.text_style.font_size: 12
                            }
                            connection_state_label := Label {
                                text: ""
                                draw_text.color: ai_cream_dim
                                draw_text.text_style.font_size: 11
                            }

                            // Live context-window usage — updated every turn
                            // from `context/normalization` (App::update_context_indicator).
                            // Shows how full the model's context is, so the
                            // server-side compaction that keeps it bounded is
                            // visible rather than invisible.
                            context_chip := Label {
                                text: ""
                                margin: Inset{left: 10}
                                draw_text.color: #x8FB8A6
                                draw_text.text_style.font_size: 11
                            }

                            View { width: Fill height: 1 }

                            ToolbarGlass {
                                // Slimmed for phone viewports (was 286 with a
                                // "Profile" caption — clipped at 384pt).
                                width: 150

                                // Renamed from `backend_dropdown` per W02 §
                                // "Top bar contents" — same widget shape, but
                                // populated with the user's Octos profiles
                                // (W08 will swap in real labels). Stub label
                                // ships in M1 so the dropdown isn't empty.
                                backend_dropdown := DropDown {
                                    width: Fill
                                    height: 27
                                    popup_menu_position: PopupMenuPosition.BelowInput
                                    labels: ["(no profile)"]
                                    popup_menu: PopupMenuFlat{
                                        width: 170
                                        padding: Inset{left: 4 right: 4 top: 4 bottom: 4}
                                        draw_bg +: {
                                            color: #x06231CF2
                                            border_color: #x72E4FF38
                                            border_size: 1.0
                                            border_radius: 12.0
                                        }
                                        menu_item: PopupMenuItem{
                                            height: 26
                                            padding: Inset{left: 18 right: 10 top: 0 bottom: 0}
                                            draw_text +: {
                                                color: ai_cream
                                                color_hover: #xFFF0D2
                                                color_active: ai_cream
                                                text_style +: { font_size: 11 }
                                            }
                                            draw_bg +: {
                                                color: #x00000000
                                                color_hover: #x123B31DD
                                                color_active: #xEAD8B82D
                                                border_color: #x00000000
                                                border_color_hover: #x72E4FF22
                                                border_color_active: #x72E4FF44
                                                border_size: 1.0
                                                border_radius: 6.0
                                                mark_color_active: ai_gold
                                            }
                                        }
                                    }
                                    draw_text +: {
                                        color: ai_cream
                                        text_style +: { font_size: 11 }
                                    }
                                    draw_bg +: {
                                        color: #x08251ED8
                                        color_hover: #x12382FEE
                                        border_color: #xEAD8B832
                                        border_size: 1.0
                                        border_radius: 10.0
                                        arrow_color: ai_cream
                                    }
                                }
                            }

                            glass_toolbar := ToolbarGlass {
                                width: 318
                                margin: Inset{left: 12}

                                ToolbarLabel {
                                    text: "Glass"
                                    width: 54
                                }

                                opacity_slider := GlassSlider {}

                                opacity_value := Label {
                                    width: 42
                                    text: "90%"
                                    margin: Inset{left: 4}
                                    draw_text.color: ai_cream_dim
                                    draw_text.text_style.font_size: 11
                                }
                            }
                        }

                        // W04 / M2 — `chat_screen` wrapper. Holds the chat
                        // thread + approvals + composer + task dock as one
                        // visibility unit so the sibling `content_screen`
                        // can swap in when `CurrentScreen::Content` is
                        // active. App::handle_actions toggles `set_visible`
                        // in lockstep (mirrors the W08 login_overlay
                        // pattern, app/src/main.rs:1433).
                        chat_screen := View {
                            width: Fill
                            height: Fill
                            // Down flow: card fills the space, composer docks at
                            // the bottom. A true Overlay float broke touch routing
                            // over a FULL-SCREEN card (the PortalList swallowed
                            // taps meant for the floating pill), so the composer
                            // docks below the card instead — it still auto-hides
                            // to the reveal pill, and docking avoids covering the
                            // card's bottom text.
                            flow: Down
                            spacing: 0
                            // OPAQUE BLACK backing for the whole chat area. The
                            // full-screen card is pinned lower than the viewport
                            // top (a collapsed prior message still reserves a
                            // slot); without an opaque backing that gap samples
                            // the uninitialized compositor surface as BRIGHT RED.
                            // Black guarantees any uncovered strip reads black.
                            show_bg: true
                            draw_bg +: {
                                color: #x000000FF
                            }

                        chat_shell := View {
                            width: Fill
                            height: Fill
                            flow: Overlay

                            empty_state := View {
                                width: Fill
                                height: Fill
                                flow: Down
                                align: Align{x: 0.5 y: 0.46}
                                spacing: 18

                                Label {
                                    text: "我们该做什么？"
                                    draw_text.color: #xF3E3C7
                                    draw_text.text_style.font_size: 27
                                }

                                Label {
                                    // Fill width lets the text wrap instead of
                                    // hard-clipping at the screen edge (seen on
                                    // the 480px watch); stays one line where it
                                    // already fits.
                                    width: Fill
                                    align: Align{x: 0.5}
                                    text: "输入自然语言，生成可交互的 Makepad diagram。"
                                    draw_text.color: #xCDBF9FAA
                                    draw_text.text_style.font_size: 12
                                }
                            }

                            chat_list := ChatList {}

                            // The waiting curtain: "the model is working on it",
                            // full screen. It replaced an 84px pill docked above
                            // the composer, which on a full-bleed card was easy
                            // to miss entirely. LAST child of this Overlay so it
                            // paints over the card.
                            //
                            // The opaque backing is belt-and-braces. Measured on
                            // device: while a turn runs, the previous card is not
                            // drawn at all (the streaming item stays hidden until
                            // it completes), so what sits behind the blobs is the
                            // app background — every sampled pixel of a photo card
                            // was gone. The backing is here so that if a partial
                            // card ever does draw, it cannot peek between blobs.
                            thinking_curtain := View {
                                width: Fill
                                height: Fill
                                visible: false
                                show_bg: true
                                draw_bg +: {
                                    color: #x000000F5
                                }
                                octo := OctoThinking {}
                            }
                        }

                        // W05 — typed approval cards. The pane hides itself
                        // when `APP_STATE.approvals` is empty (see
                        // `app/src/app/approvals.rs::draw_walk`); when
                        // approvals are pending it pins above the composer.
                        approvals_pane := ApprovalsPane {}

                        // toast_row lives inside composer_row (bottom stack) so
                        // toasts sit just above the floating composer — not at
                        // the top of the Overlay flow. The thinking indicator is
                        // NOT here: it is `thinking_curtain`, full screen over
                        // the card.

                        composer_row := View {
                            width: Fill
                            height: Fit
                            flow: Down
                            align: Align{x: 0.5}

                            // Toast strip — one auto-dismissing pill for
                            // compaction / memory-saved / warning messages
                            // (App::sync_toasts drives it from APP_STATE.toasts).
                            toast_row := View {
                                width: Fill
                                height: Fit
                                visible: false
                                align: Align{x: 0.5}
                                toast_pill := RoundedView {
                                    width: Fit
                                    height: Fit
                                    margin: Inset{top: 2 bottom: 4}
                                    padding: Inset{left: 14 top: 8 right: 14 bottom: 8}
                                    show_bg: true
                                    draw_bg +: {
                                        color: #x0C3A2FF2
                                        radius: 10.0
                                    }
                                    toast_label := Label {
                                        width: Fit
                                        height: Fit
                                        text: ""
                                        draw_text.color: #xDCEAE0
                                        draw_text.text_style.font_size: 11
                                    }
                                }
                            }

                            // Collapsed state: a slim translucent pill that
                            // reveals the composer again (it auto-hides after a
                            // card renders). Only one of pill/composer is visible
                            // at a time; they stack at the bottom of this flow.
                            reveal_pill := PillButton {
                                text: "+"
                                width: 52
                                height: 27
                                visible: false
                                margin: Inset{bottom: 12}
                                draw_text +: {
                                    color: ai_cream
                                    text_style +: { font_size: 18 }
                                }
                                draw_bg +: {
                                    color: #x0B4035B0
                                    color_hover: #x123B31D0
                                    border_color: #x72E4FF44
                                    border_size: 1.0
                                    border_radius: 15.0
                                }
                            }

                            composer := GlassPanel {
                                // No min-width: a 620pt floor pushed the
                                // composer (and its Send button) off-screen
                                // on portrait phones (~384pt viewport).
                                width: Fill{max: 1040}
                                height: Fit
                                new_batch: true
                                flow: Down
                                margin: Inset{left: 12 right: 12}
                                padding: Inset{left: 14 top: 5 right: 12 bottom: 5}
                                spacing: 2
                                draw_bg +: {
                                    tint_color: #x0B4035
                                    // Floats over the card — keep it translucent
                                    // (liquid glass) so the card shows through.
                                    tint_alpha: 0.50
                                    border_color: ai_cyan
                                    border_alpha: 0.42
                                    border_width: 1.0
                                    corner_radius: 11.0
                                    halo_color: ai_cyan
                                    halo_strength: 0.05
                                    halo_radius: 3.0
                                    highlight_strength: 0.24
                                    highlight_band_height: 28.0
                                    chroma_strength: 0.0
                                    noise_strength: 0.003
                                }

                                input := TextInput {
                                    width: Fill
                                    height: 32
                                    // Soft keyboards: show a Send action key
                                    // (ImeAction::Send submits via the same
                                    // path as the ↑ button). Without this the
                                    // on-screen Enter did nothing visible.
                                    return_key_type: Send
                                    empty_text: "问任何事…"
                                    draw_bg +: {
                                        color: #00000000
                                        color_hover: #00000000
                                        color_focus: #00000000
                                        border_size: 0.0
                                        border_radius: 0.0
                                    }
                                    // Per-instance font override — TextInput bakes
                                    // `theme.font_regular` at DSL-expansion time, same
                                    // issue as Markdown/CodeView. Without this the
                                    // input box shows tofu for CJK and U+2192 arrows.
                                    draw_text +: {
                                        color: ai_cream
                                        color_empty: ai_cream_dim
                                        text_style: theme.font_regular{
                                            line_spacing: theme.font_wdgt_line_spacing
                                            font_size: 13
                                            font_family: FontFamily{
                                                latin := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                                                chinese := FontMember{res: file_resource(#(fpath("cjk"))) asc: 0.0 desc: 0.0}
                                                symbols := FontMember{res: file_resource(#(fpath("sans_latin"))) asc: 0.0 desc: 0.0}
                                                emoji := FontMember{res: file_resource(#(fpath("emoji"))) asc: 0.0 desc: 0.0}
                                            }
                                        }
                                    }
                                }

                                composer_actions := View {
                                    width: Fill
                                    height: Fit
                                    flow: Right
                                    align: Align{y: 0.5}
                                    spacing: 6

                                    attach_button := IconButton { text: "+" width: 30 height: 27 }

                                    // @ mention, ⌘ tools and 默认权限 stubs
                                    // dropped: all are M1 placeholders and
                                    // the row must fit a 384pt phone
                                    // viewport.

                                    // Thinking + A2App toggles removed — this app
                                    // is now an always-on A2App card generator
                                    // (splash_mode is forced true at startup).

                                    View { width: Fill height: 1 }

                                    cancel_button := ButtonFlat {
                                        text: "Cancel"
                                        width: 64
                                        height: 27
                                        visible: false
                                        draw_text +: {
                                            color: #xF2F4F8
                                            text_style +: { font_size: 11 }
                                        }
                                        draw_bg +: {
                                            color: #x4B332FCC
                                            color_hover: #x64413ADD
                                            border_color: #xEAD8B818
                                            border_size: 1.0
                                            border_radius: 10.0
                                        }
                                    }

                                    clear_button := ButtonFlatIcon {
                                        width: 34
                                        height: 27
                                        icon_walk: Walk{ width: 19, height: 19 }
                                        draw_icon +: {
                                            color: #xB6C6BE
                                            svg: crate_resource("self:resources/icons/clear.svg")
                                        }
                                        draw_bg +: {
                                            color: #00000000
                                            color_hover: #xEAD8B814
                                            border_size: 0.0
                                            border_radius: 8.0
                                        }
                                    }

                                    send_button := SendButton {
                                        width: 30
                                        height: 27
                                    }
                                }
                            }
                        }

                        // W04 / M2 — TaskDock placed below the composer per
                        // 04-IA-AND-NAVIGATION.md § ChatScreen ASCII layout.
                        // Idle state collapses to zero height (`set_visible`
                        // off when both tool_calls and tasks are empty for the
                        // current session). Smoothing animation lifted from
                        // `aichat:480` (RubberView wrapping the assistant
                        // message body).
                        task_dock := TaskDock {}
                        }

                        // W04 / M2 — Content browser screen. Sibling to
                        // `chat_screen`; only one of the two is visible at
                        // a time. App::handle_actions toggles
                        // `set_visible` based on `APP_STATE.navigation`.
                        // Hidden by default — the boot path keeps Chat as
                        // the active screen.
                        content_screen := ContentBrowser {
                            visible: false
                        }

                        // Coding / Studio / Slides / Sites screens removed —
                        // unsupported in this build (user directive).

                        status_label := Label {
                            width: Fill
                            height: Fit
                            text: "Initializing..."
                            margin: Inset{left: 12 right: 12 top: 0 bottom: 0}
                            draw_text.text_style.font_size: 10
                            draw_text.color: #xE2D2B9AA
                            // Minimalist full-screen A2App: no footer chrome.
                            visible: false
                        }
                    }
                    }

                    // W08 — LoginScreen overlay. Lives at the body level
                    // (sibling to `app_shell`) so its hit-region covers
                    // everything when visible. App-side boot / login flow
                    // toggles `app_shell.visible` and `login_overlay.visible`
                    // in lockstep so only one of the two is interactive at a
                    // time. Default: hidden — `App::after_new_from_script`
                    // flips it on if no token is in the keychain.
                    login_overlay := LoginScreen {
                        visible: false
                    }

                    // W04 / M2 — File-viewer overlay (sibling to
                    // `app_shell` so it covers the whole window when a
                    // file is opened). Toggled by App::handle_actions on
                    // ContentAction::Open. Mirrors `login_overlay`.
                    viewer_overlay := ViewerOverlay {}

                    // There WAS a desktop window-resize grip here — three
                    // diagonal strokes meant for the bottom-right corner. It
                    // drew at the window's TOP-LEFT instead, over every card,
                    // on every platform: `Vector::draw_walk` rebuilds its Walk
                    // from abs_pos/margin/width/height and never reads
                    // `self.layout` (aichat/widgets/src/vector.rs), so
                    // `align: Align{x: 1.0 y: 1.0}` was a silent no-op and in a
                    // `flow: Overlay` parent the margins don't shift a child
                    // either. Removed rather than re-aligned: the phone has no
                    // window to resize.
                }
            }
        }
    }
}

// Global chat state accessible to ChatList widget
pub static CHAT_DATA: std::sync::RwLock<ChatData> = std::sync::RwLock::new(ChatData {
    messages: Vec::new(),
    streaming_text: String::new(),
    authoritative_text: String::new(),
    thinking_text: String::new(),
    is_streaming: false,
    a2app_state: std::collections::BTreeMap::new(),
    saved_stream_cards: std::collections::BTreeSet::new(),
});

/// Bumped whenever `CHAT_DATA` is bulk-replaced (app switch restore, wipe) —
/// NOT on normal append/stream. `ChatList` watches this and drops its
/// `rendered_cache` when it changes, so a restored card re-parses instead of
/// redrawing a torn-down (blank) markdown widget. Layer 3 (W08).
pub static CHAT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Recursively copy `src` into `dst` (creating dirs), overwriting files.
/// Used by the boot provisioning hook to deploy an octos-home (GLM profile +
/// a2app memory tree) from a world-readable staging dir (`/data/local/tmp`,
/// which `adb push` can write) into the app-private octos-home — the only way
/// to provision a non-rooted, non-debuggable device. Returns files copied.
fn deploy_provision(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<usize> {
    let mut n = 0;
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            n += deploy_provision(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
            n += 1;
        }
    }
    Ok(n)
}

// Slider position range (NOT alpha — alpha is derived per-layer).
const DEFAULT_GLASS_OPACITY: f64 = 0.90;
const MIN_GLASS_OPACITY: f64 = 0.10;
const MAX_GLASS_OPACITY: f64 = 1.00;

#[derive(Debug, Clone, Copy, PartialEq)]
struct GlassOpacity {
    app: f32,
    sidebar: f32,
    main: f32,
    composer: f32,
}

// Map slider [0.10..1.00] to actual panel alpha. The earlier mapping only
// moved alpha slightly, so the "Glass" control felt inert on a transparent
// window. Keep layer ordering, but make the low/high ends visually obvious.
fn glass_opacity_values(slider: f64) -> GlassOpacity {
    let t = ((slider.clamp(MIN_GLASS_OPACITY, MAX_GLASS_OPACITY) - MIN_GLASS_OPACITY)
        / (MAX_GLASS_OPACITY - MIN_GLASS_OPACITY)) as f32;
    let shell = 0.28 + t * 0.64;
    GlassOpacity {
        app: shell,
        main: (shell + 0.05).min(0.99),
        sidebar: (shell + 0.08).min(0.99),
        composer: (shell + 0.11).min(0.99),
    }
}

fn should_start_window_drag(abs: DVec2, size: DVec2) -> bool {
    const RESIZE_EDGE_MARGIN: f64 = 10.0;
    const DRAG_STRIP_HEIGHT: f64 = 52.0;
    const RIGHT_TOOLBAR_WIDTH: f64 = 260.0;

    abs.y > RESIZE_EDGE_MARGIN
        && abs.y < DRAG_STRIP_HEIGHT
        && abs.x > RESIZE_EDGE_MARGIN
        && abs.x < size.x - RESIZE_EDGE_MARGIN
        && abs.x < size.x - RIGHT_TOOLBAR_WIDTH
}

// Diagram-fence safety scanner moved to `app/diagram_safety.rs` — same
// behaviour, just lifted out of main.rs for readability. The functions are
// re-exported below so the streaming pipeline (chat list redraw +
// `handle_event` on `TurnComplete`) doesn't need to qualify the path.
// `assistant_message_is_safe_for_history` is only referenced from the
// regression tests in `mod tests`; allow `unused` so a non-test build
// doesn't warn.
#[allow(unused_imports)]
use crate::app::diagram_safety::{
    assistant_message_is_safe_for_history, assistant_message_is_safe_to_store,
    unwrap_outer_markdown_fence,
};
// W04 — `SessionList` widget + REST hydrate plumbing. The widget type is
// referenced from the `let SessionList = …` register block in script_mod
// above via the fully-qualified `crate::app::sessions::SessionList` path,
// so no `use` for it here. The `SessionListAction` variants are folded in
// `App::handle_actions`.
use crate::app::sessions::{self as sessions_mod, SessionListAction, APP_STATE};
// W04 / M2 — content browser + viewers actions. Action variants land via
// `Cx::post_action` and are folded in `App::handle_actions`. State globals
// (`CONTENT_STATE`, `VIEWER_STATE`) mirror the `APP_STATE` pattern.
use crate::app::content_browser::{
    self as content_mod, ContentAction, ContentFilter, CONTENT_STATE,
};
use crate::app::viewers::{
    self as viewers_mod, OpenViewer, ViewerAction, VIEWER_STATE,
};
use octos_app_store::navigation::{CurrentScreen, NavigationEvent};
use octos_app_transport::rest::MyContentQuery;

/// Map a `recorded_decision` string from a server `-32011 APPROVAL_NOT_PENDING`
/// error payload back to an `ApprovalDecision`. The wire form is
/// `serde_json` snake_case (`"approve"` / `"deny"`); see octos-core
/// `ui_protocol.rs:564-569`.
fn parse_recorded_decision(s: &str) -> Option<octos_core::ui_protocol::ApprovalDecision> {
    use octos_core::ui_protocol::ApprovalDecision;
    match s {
        "approve" => Some(ApprovalDecision::Approve),
        "deny" => Some(ApprovalDecision::Deny),
        _ => None,
    }
}

// (W02 strip) — `CHAT_SAVE_PATH` (`aichat_history.json`),
// `stateless_history_messages` and the `SavedHistory` / `SavedMessage`
// SerJson types lived here. They're gone: Octos sessions are stateful
// server-side, so we don't replay history into a stateless backend, and the
// flat-file JSON cache is replaced by per-session SQLite + REST hydrate
// (W04). See `01-ARCHITECTURE.md` § "Persistence".

#[derive(Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
}

#[derive(Script, ScriptHook, Widget)]
pub struct MermaidSvgView {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_svg: DrawSvg,
    #[live]
    draw_text: DrawText,
    #[live]
    draw_flow_dot: DrawColor,
    #[rust]
    doc: SvgDocument,
    #[rust]
    content_w: f64,
    #[rust]
    content_h: f64,
    #[rust]
    last_src_hash: u64,
    #[rust]
    pending_src_hash: u64,
    #[rust]
    cached_text_cmds: Vec<SvgTextCmd>,
    #[rust]
    cached_edges: Vec<SvgEdge>,
    #[rust(1.0f64)]
    zoom: f64,
    #[rust]
    pan: DVec2,
    #[rust]
    drag_start_abs: Option<DVec2>,
    #[rust]
    drag_start_pan: DVec2,
    #[rust]
    last_rect: Rect,
    #[rust]
    anim_t: f32,
    #[rust]
    next_frame: NextFrame,
}

impl MermaidSvgView {
    pub fn set_svg_str(&mut self, cx: &mut Cx, svg: &str) {
        use std::hash::{DefaultHasher, Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        svg.hash(&mut hasher);
        let hash = hasher.finish();
        if hash == self.last_src_hash && !self.doc.root.is_empty() {
            return;
        }

        self.last_src_hash = hash;
        self.doc = parse_svg(svg);
        self.cached_text_cmds = collect_text_cmds(&self.doc);
        self.cached_edges = collect_edges(&self.doc);
        self.draw_svg.cache_valid = false;
        self.draw_svg.set_doc_bounds(&self.doc);
        if let Some(vb) = self.doc.viewbox.as_ref() {
            self.draw_svg.content_bounds = (vb.x, vb.y, vb.x + vb.width, vb.y + vb.height);
            self.content_w = vb.width as f64;
            self.content_h = vb.height as f64;
            self.draw_svg.content_size = dvec2(self.content_w, self.content_h);
        }
        self.redraw(cx);
    }

    pub fn set_mermaid_src(&mut self, cx: &mut Cx, src: &str) {
        use std::hash::{DefaultHasher, Hash, Hasher};

        let cleaned: String = src.chars().filter(|c| *c != '▋').collect();
        let trimmed = cleaned.trim();
        if trimmed.is_empty() || trimmed.len() < 8 {
            return;
        }

        let mut hasher = DefaultHasher::new();
        trimmed.hash(&mut hasher);
        let hash = hasher.finish();
        if hash == self.last_src_hash && !self.doc.root.is_empty() {
            return;
        }

        // Streaming debounce: render only when the same source arrives twice
        // in a row. During active token streaming the body changes every
        // frame; after a pause or close it stabilizes and renders once.
        if hash != self.pending_src_hash {
            self.pending_src_hash = hash;
            return;
        }

        match streaming_markdown_kit::render_mermaid_to_svg(trimmed) {
            Ok(svg) => {
                self.set_svg_str(cx, &svg);
                self.last_src_hash = hash;
            }
            Err(err) => {
                log!("mermaid render error: {:?}", err);
            }
        }
    }
}

impl Widget for MermaidSvgView {
    fn set_text(&mut self, cx: &mut Cx, v: &str) {
        self.set_mermaid_src(cx, v);
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if self.next_frame.is_event(event).is_some() {
            self.anim_t = (self.anim_t + 0.003).rem_euclid(1.0);
            self.next_frame = cx.new_next_frame();
            self.redraw(cx);
        }

        match event.hits_with_capture_overload(cx, self.draw_svg.area(), true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                if fe.tap_count >= 2 {
                    self.zoom = 1.0;
                    self.pan = DVec2::default();
                    self.drag_start_abs = None;
                    self.redraw(cx);
                } else {
                    self.drag_start_abs = Some(fe.abs);
                    self.drag_start_pan = self.pan;
                    cx.set_cursor(MouseCursor::Grabbing);
                }
            }
            Hit::FingerMove(fe) => {
                if let Some(start) = self.drag_start_abs {
                    self.pan = self.drag_start_pan + (fe.abs - start);
                    self.redraw(cx);
                }
            }
            Hit::FingerUp(_) => {
                if self.drag_start_abs.is_some() {
                    self.drag_start_abs = None;
                    cx.set_cursor(MouseCursor::Grab);
                }
            }
            Hit::FingerHoverIn(_) => cx.set_cursor(MouseCursor::Grab),
            Hit::FingerScroll(fs) => {
                if !fs.modifiers.is_primary() {
                    return;
                }
                let dy = if fs.scroll.y.abs() > f64::EPSILON {
                    fs.scroll.y
                } else {
                    fs.scroll.x
                };
                let factor = (1.0 - dy * 0.005).clamp(0.5, 2.0);
                let old_zoom = self.zoom.max(0.01);
                let new_zoom = (old_zoom * factor).clamp(0.2, 8.0);
                let local = fs.abs - self.last_rect.pos - self.pan;
                let content_local = local / old_zoom;
                self.pan = fs.abs - self.last_rect.pos - content_local * new_zoom;
                self.zoom = new_zoom;
                self.redraw(cx);
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        if self.doc.root.is_empty() {
            return DrawStep::done();
        }
        let sw = self.draw_svg.content_size.x;
        let sh = self.draw_svg.content_size.y;
        if sw <= 0.0 || sh <= 0.0 {
            return DrawStep::done();
        }
        let walk = Walk {
            abs_pos: walk.abs_pos,
            margin: walk.margin,
            width: match walk.width {
                Size::Fit { .. } => Size::Fixed(sw),
                other => other,
            },
            height: match walk.height {
                Size::Fit { .. } => Size::Fixed(sh),
                other => other,
            },
            metrics: walk.metrics,
        };
        let rect = cx.walk_turtle(walk);
        self.last_rect = rect;

        let zoom = if self.zoom > 0.01 { self.zoom } else { 1.0 };
        let effective_rect = Rect {
            pos: rect.pos + self.pan,
            size: rect.size * zoom,
        };

        self.draw_svg.svg_doc = Some(std::mem::take(&mut self.doc));
        self.draw_svg.has_animations = false;
        self.draw_svg.render_to_rect(cx, &effective_rect, 0.0);
        self.doc = self.draw_svg.svg_doc.take().unwrap_or_default();

        let text_cmds = std::mem::take(&mut self.cached_text_cmds);
        self.render_text_cmds(cx, &effective_rect, &text_cmds);
        self.cached_text_cmds = text_cmds;

        let edges = std::mem::take(&mut self.cached_edges);
        self.render_flow_dots(cx, &effective_rect, &edges);
        let has_edges = !edges.is_empty();
        self.cached_edges = edges;

        if has_edges {
            self.next_frame = cx.new_next_frame();
        }
        DrawStep::done()
    }
}

impl MermaidSvgView {
    fn render_text_cmds(&mut self, cx: &mut Cx2d, rect: &Rect, cmds: &[SvgTextCmd]) {
        if cmds.is_empty() {
            return;
        }
        let (min_x, min_y, max_x, max_y) = self.draw_svg.content_bounds;
        let content_w = (max_x - min_x) as f64;
        let content_h = (max_y - min_y) as f64;
        if content_w <= 0.0 || content_h <= 0.0 {
            return;
        }
        let scale = (rect.size.x / content_w).min(rect.size.y / content_h);
        let render_w = content_w * scale;
        let render_h = content_h * scale;
        let origin_x = rect.pos.x + (rect.size.x - render_w) * 0.5;
        let origin_y = rect.pos.y + (rect.size.y - render_h) * 0.5;
        const PX_TO_PT: f64 = 0.75;

        for cmd in cmds {
            if cmd.text.trim().is_empty() {
                continue;
            }
            let world_font_size = (cmd.font_size as f64 * scale * PX_TO_PT).max(1.0);
            self.draw_text.text_style.font_size = world_font_size as f32;
            self.draw_text.color = vec4(
                cmd.color.0,
                cmd.color.1,
                cmd.color.2,
                cmd.color.3.max(0.0),
            );

            let lines: Vec<&str> = cmd.text.split('\n').collect();
            let line_step_screen = world_font_size * 1.2;
            let base_cy = origin_y + (cmd.y as f64 - min_y as f64) * scale;
            let base_cx_screen = origin_x + (cmd.x as f64 - min_x as f64) * scale;

            for (line_index, line) in lines.iter().enumerate() {
                if line.is_empty() {
                    continue;
                }
                let estimated_width: f64 = line
                    .chars()
                    .map(|ch| {
                        let advance = if (ch as u32) >= 0x2E80 { 1.0 } else { 0.55 };
                        advance * world_font_size
                    })
                    .sum();
                let anchor_shift = match cmd.text_anchor {
                    SvgTextAnchor::Start => 0.0,
                    SvgTextAnchor::Middle => -0.5,
                    SvgTextAnchor::End => -1.0,
                } * estimated_width;

                let px = base_cx_screen + anchor_shift;
                let cy = base_cy + line_step_screen * line_index as f64;
                let py = cy - world_font_size * 0.7;
                self.draw_text.draw_abs(cx, dvec2(px, py), line);
            }
        }
    }

    fn render_flow_dots(&mut self, cx: &mut Cx2d, rect: &Rect, edges: &[SvgEdge]) {
        if edges.is_empty() {
            return;
        }
        let (min_x, min_y, max_x, max_y) = self.draw_svg.content_bounds;
        let content_w = (max_x - min_x) as f64;
        let content_h = (max_y - min_y) as f64;
        if content_w <= 0.0 || content_h <= 0.0 {
            return;
        }
        let scale = (rect.size.x / content_w).min(rect.size.y / content_h);
        let render_w = content_w * scale;
        let render_h = content_h * scale;
        let origin_x = rect.pos.x + (rect.size.x - render_w) * 0.5;
        let origin_y = rect.pos.y + (rect.size.y - render_h) * 0.5;
        let dot_size = 10.0_f64;
        let pulse =
            0.55 + 0.45 * (self.anim_t * std::f32::consts::TAU * 1.5).sin().abs();

        for (edge_index, edge) in edges.iter().enumerate() {
            if edge.points.len() < 2 {
                continue;
            }
            let phase = (self.anim_t + edge_index as f32 * 0.17).rem_euclid(1.0);
            let max_index = edge.points.len() - 1;
            let float_index = phase * max_index as f32;
            let point_index = float_index as usize;
            let next_index = (point_index + 1).min(max_index);
            let frac = float_index - point_index as f32;
            let p0 = edge.points[point_index];
            let p1 = edge.points[next_index];
            let wx = p0.0 + (p1.0 - p0.0) * frac;
            let wy = p0.1 + (p1.1 - p0.1) * frac;

            let sx = origin_x + (wx as f64 - min_x as f64) * scale;
            let sy = origin_y + (wy as f64 - min_y as f64) * scale;

            self.draw_flow_dot.color = vec4(
                edge.color.0,
                edge.color.1,
                edge.color.2,
                edge.color.3 * pulse,
            );
            self.draw_flow_dot.draw_abs(
                cx,
                Rect {
                    pos: dvec2(sx - dot_size * 0.5, sy - dot_size * 0.5),
                    size: dvec2(dot_size, dot_size),
                },
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ChatRole {
    User,
    Assistant,
}

pub struct ChatData {
    pub messages: Vec<ChatMessage>,
    pub streaming_text: String,
    /// The kernel's durably-stored text for the in-flight turn, when it has
    /// sent one (`message/persisted`). Preferred over `streaming_text` at turn
    /// end: a `message/delta` lost in transit leaves `streaming_text` short by
    /// that chunk with its edges spliced mid-token, which turns a valid card
    /// DSL into one that cannot parse. Empty when the backend sends no
    /// authoritative copy, in which case the accumulation stands.
    pub authoritative_text: String,
    pub thinking_text: String,
    pub is_streaming: bool,
    /// Per-card A2App/Splash state: card (message index) → `CardState`. Each
    /// rendered card owns an isolated map so independent cards never share
    /// state; `{{state.<key>}}` substitutes that card's value. Mutated by
    /// `agent.notify` events tagged with the card's id (see `tag_notify_calls`).
    pub a2app_state: std::collections::BTreeMap<usize, CardState>,
    /// Cards already persisted mid-stream (fence-close save) for the CURRENT
    /// turn — dedupes the per-delta scan; cleared on the next prompt submit.
    /// The turn-complete save still writes the final revision.
    /// (`BTreeSet` because `HashSet::new` is not const for the static init.)
    pub saved_stream_cards: std::collections::BTreeSet<String>,
}

impl ChatData {
    /// TODO: W04 — replace this no-op with a SQLite per-session cache write.
    /// aichat's flat `aichat_history.json` is gone (see
    /// `01-ARCHITECTURE.md` § "Persistence" — REST snapshot is the source of
    /// truth, the local cache is just a startup-warmer). Calls keep working
    /// so the streaming pipeline doesn't have to special-case anything.
    pub fn save_to_disk(&self) {
        // intentionally empty
    }

    /// TODO: W04 — hydrate from the per-session SQLite cache + REST
    /// snapshot. M1 returns an empty Vec so the empty state shows on boot.
    pub fn load_from_disk() -> Vec<ChatMessage> {
        Vec::new()
    }
}

/// One open app in the client = one octos session. Layer 3 (W08 Phase 2): the
/// client is a window manager over N of these, and `App::foreground` indexes
/// the visible one. Path B (hydrate-on-switch): only the foreground app's
/// conversation lives in the global `CHAT_DATA`; switching foreground calls
/// `resume_session` → `session/hydrate` to reload that session's history.
/// Background apps live on the server ledger — we keep only this light record
/// plus an unread badge. Streaming `AgentEvent`s carry a `prompt_id` (not a
/// session id), so `current_prompt` is how a delta is routed to its owning app.
#[derive(Clone)]
pub struct AppRecord {
    pub session_id: SessionId,
    pub title: String,
    /// The request this app's current card is FOR, kept for the repair turn.
    ///
    /// A repair prompt that only lists diagnostics invites the model to
    /// re-emit the EXEMPLAR — whose weather card declares `city: ""` (device
    /// location). Measured on device: "weather in osaka" refused five times,
    /// repaired, and rendered San Jose, while the same request accepted
    /// first-try rendered Osaka. The intent has to travel with the repair.
    pub last_request: Option<String>,
    /// The app domain this session is specialised for ("weather"/"stock"/"news").
    /// The AMA's routing decision names a domain; we activate the app agent whose
    /// `domain` matches. `None` for a generic app (Layer-3 "open another app").
    pub domain: Option<String>,
    /// In-flight turn for THIS app (`None` when idle).
    pub current_prompt: Option<PromptId>,
    /// A background app's turn produced output the user hasn't seen yet. The
    /// foreground guard sets this instead of writing `CHAT_DATA`; cleared when
    /// the app is brought to the foreground.
    pub has_updates: bool,
    /// Saved conversation for this app while it's backgrounded (Path A-lite).
    /// The foreground app's live conversation lives in the global `CHAT_DATA`;
    /// on switch we snapshot `CHAT_DATA` into the outgoing app and restore the
    /// incoming one. Instant and fully offline (no server round-trip), which
    /// matters because the on-device server hydrate needs connectivity. Empty
    /// for an app that has never been foregrounded with content.
    pub saved_messages: Vec<ChatMessage>,
    pub saved_a2app: std::collections::BTreeMap<usize, CardState>,
    /// One automatic lint-repair turn has been spent for the CURRENT routed
    /// intent (reset on the next `route_to_app`). Caps the validate→repair
    /// loop at a single retry so a stubborn model can't ping-pong forever.
    pub repair_attempted: bool,
    /// L0 CHECKER refusals get their own, larger budget (`L0_REPAIR_BUDGET`,
    /// also per-intent, reset with `repair_attempted`). A refused L0 card is
    /// a BLANK SCREEN, not a cosmetic miss, and two live runs showed one
    /// retry is not enough for a syntax-class mistake — the model needs the
    /// teaching diagnostic more than once. Each retry feeds the checker's
    /// own diagnostics back, exactly like the first.
    pub l0_repair_attempts: u8,
}

/// How many automatic repair turns an L0 checker refusal may spend per
/// routed intent. Lint and security repairs keep their single shot
/// (`repair_attempted`); this budget is only for cards the CHECKER refused,
/// where the alternative is a quiet fallback card instead of the answer.
pub const L0_REPAIR_BUDGET: u8 = 3;

impl AppRecord {
    fn new(session_id: SessionId, title: impl Into<String>) -> Self {
        Self {
            session_id,
            title: title.into(),
            last_request: None,
            domain: None,
            current_prompt: None,
            has_updates: false,
            saved_messages: Vec::new(),
            saved_a2app: std::collections::BTreeMap::new(),
            repair_attempted: false,
            l0_repair_attempts: 0,
        }
    }
    /// A domain-specialised app agent (weather/stock/news), for AMA routing.
    fn with_domain(session_id: SessionId, title: impl Into<String>, domain: &str) -> Self {
        let mut r = Self::new(session_id, title);
        r.domain = Some(domain.to_string());
        r
    }
}

// ChatList widget wrapping PortalList for chat message display.
#[derive(Script, ScriptHook, Widget)]
pub struct ChatList {
    #[deref]
    view: View,
    #[rust]
    animating_msg: Option<usize>,
    /// Newest-card id the list was last scroll-pinned to. We pin the card to the
    /// top ONCE when it appears (id changes), not every draw, so the user's
    /// drag-scroll position persists between frames.
    #[rust]
    pinned_id: Option<usize>,
    /// Cache of the last card render, keyed by (item_id, raw message, card state).
    /// Resolving + re-parsing the card DSL every draw — INCLUDING every scroll
    /// frame — is the dominant per-frame cost (~30ms: re-runs the sys.* helpers,
    /// the whole string-rewrite pipeline, and re-parses ~55 labels). The card is
    /// static during a scroll, so we skip all of it when the inputs are unchanged
    /// and just re-draw the already-parsed widget. This is what makes scrolling
    /// smooth instead of ~30fps.
    /// …and the data-fetch epoch, because an L0 card bakes its values IN.
    ///
    /// A `sys.*` call in an L0 card is evaluated when the ledger is resolved,
    /// not when the widget draws — so the tree carries whatever the fetch had
    /// returned at that moment, which for a cold card is a placeholder. The old
    /// path emitted the call into the widget DSL and the Splash widget re-ran it
    /// on each epoch bump; this one has to re-resolve instead.
    ///
    /// Without the epoch here, every value on the stock list stayed an em dash
    /// forever: the text and the state never change when data arrives, so the
    /// cache hit and the placeholder was final.
    #[rust]
    rendered_cache: Option<(usize, String, CardState, u64)>,
    /// Last-seen `CHAT_GENERATION`. When the App bulk-replaces `CHAT_DATA`
    /// (app switch / wipe) it bumps the counter; we drop `rendered_cache` so
    /// the restored card re-parses instead of redrawing a stale/blank widget.
    #[rust]
    last_gen: u64,
    /// Last-seen data-fetch epoch, and a pump for it.
    ///
    /// An L0 card's `sys.*` values are baked in when the ledger is resolved, so
    /// a cold card carries placeholders — and nothing brings it back. The Splash
    /// widget re-evaluates on an epoch bump because it runs its own frame pump;
    /// this list draws on interaction, so without a nudge the stock card shows
    /// em dashes forever while the data sits fetched and unused.
    #[rust]
    last_fetch_epoch: u64,
    /// The rendered card follows a live position AND ticks its own values, so an
    /// epoch bump must not re-resolve it. See the epoch poll.
    #[rust]
    driving_card: bool,
    #[rust]
    epoch_poll: Timer,
    /// The frame that draws the values a fetch just delivered.
    ///
    /// Clearing the cache and calling `redraw` is not enough: this app renders
    /// ON DEMAND, so a dirty area sits dirty until something asks for a frame.
    /// The stock card cleared its cache the moment `sys.movers` landed and then
    /// drew nothing — every price stayed an em dash over live data already in
    /// memory. The Splash widget arms the same pump for any body that binds a
    /// live source.
    #[rust]
    epoch_frame: NextFrame,
}

impl Widget for ChatList {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Layer 3 — invalidate the card cache when CHAT_DATA was bulk-replaced.
        let gen = CHAT_GENERATION.load(std::sync::atomic::Ordering::Relaxed);
        if gen != self.last_gen {
            self.last_gen = gen;
            self.rendered_cache = None;
            self.pinned_id = None;
            self.animating_msg = None;
        }
        let data = CHAT_DATA.read().unwrap();

        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                let msg_count = data.messages.len();
                let items_len = msg_count + data.is_streaming as usize;
                // Weather app shows ONLY the newest card, and THIS list scrolls it (the
                // card is taller than the screen). Put ONLY the newest item in the range
                // so first_id == range_start == the card and the layout's top-clamp
                // engages. Fling momentum is disabled on the list (flick_scroll_scaling 0):
                // a fling carries first_id past range_start, the clamp is skipped, and the
                // card sails off-screen for good. Drag-only scrolling stays clamped — the
                // layout re-pins first_scroll to the top the moment the drag stops.
                let newest = items_len.saturating_sub(1);
                list.set_item_range(cx, newest, items_len);
                // NO tailing for the card: tail_range makes the list scroll down by the
                // card's overflow (~417dp) every draw to keep its BOTTOM (the detail grid)
                // in view, so the hero temperature at the top is unreachable. Other code
                // paths (send/refresh) call set_tail_range(true); re-assert false here every
                // draw so the card rests at its top and scrolls DOWN to details, iOS-style.
                list.set_tail_range(false);
                // Pin to the card's top ONLY the first frame it appears (id changes) so
                // the user's drag-scroll position survives the every-frame redraws.
                if items_len > 0 && self.pinned_id != Some(newest) {
                    list.set_first_id_and_scroll(newest, 0.0);
                    self.pinned_id = Some(newest);
                }

                while let Some(item_id) = list.next_visible_item(cx) {
                    // Weather app: show ONLY the newest card full-screen (the
                    // streaming item while generating, else the last message).
                    // Collapse every earlier item to zero height — a scrollable
                    // stack of full-screen cards scrolled unstably.
                    if item_id + 1 < items_len {
                        let item_widget = list.item(cx, item_id, id!(User));
                        item_widget.set_visible(cx, false);
                        item_widget.draw_all_unscoped(cx);
                        continue;
                    }
                    if data.is_streaming && item_id == msg_count {
                        let just_started = self.animating_msg != Some(item_id);
                        if just_started {
                            self.animating_msg = Some(item_id);
                        }

                        let (item_widget, _) = list.item_with_existed(cx, item_id, id!(Assistant));
                        // Copy/share icons only appear once the answer is
                        // complete — hide them on the in-flight streaming item.
                        item_widget
                            .button(cx, ids!(copy_button))
                            .set_visible(cx, false);
                        item_widget
                            .button(cx, ids!(share_button))
                            .set_visible(cx, false);
                        let streaming_body;
                        // Reasoning/thinking is intentionally NOT surfaced in the
                        // chat bubble (user preference) — the swimming-octopus
                        // indicator conveys "working". Show only a minimal
                        // placeholder until the answer's first token arrives.
                        let text: &str = if data.streaming_text.is_empty() {
                            "…"
                        } else {
                            let opts = SanitizeOptions {
                                trim_unclosed_fence: false,
                                ..SanitizeOptions::default()
                            };
                            // Remend keeps fenced blocks, tables and math
                            // self-consistent mid-stream so the Markdown
                            // widget doesn't re-layout a half-closed block
                            // on every token. An open `runsplash` fence is
                            // deferred first — see `defer_unclosed_runsplash`.
                            // Gate order: hold back an unclosed block, THEN
                            // neutralize EVERY closed-but-forbidden one before it
                            // reaches the Splash renderer (net-write exfil).
                            let deferred_plan = defer_unclosed_runplan(&data.streaming_text);
                            let materialized_plan =
                                materialize_runplan_for_display(&deferred_plan);
                            let deferred = defer_unclosed_runsplash(&materialized_plan);
                            let safe = neutralize_forbidden_cards(&deferred);
                            streaming_body = streaming_display_with_latex_autowrap_remend(
                                &safe,
                                opts,
                            );
                            &streaming_body
                        };
                        let mut markdown = item_widget.markdown(cx, ids!(selectable));
                        // Unwrap outer ```markdown wrapper in streaming
                        // content: some LLMs emit the wrapper as the very
                        // first tokens, so we'd otherwise render a growing
                        // code block for the whole stream.
                        let unwrapped_stream = unwrap_outer_markdown_fence(text);
                        let empty_state = CardState::new();
                        let card_state = data.a2app_state.get(&item_id).unwrap_or(&empty_state);
                        let resolved_stream =
                            resolve_a2app_card(cx, unwrapped_stream, item_id, card_state);
                        markdown.set_text(cx, &resolved_stream);
                        if just_started {
                            markdown.reset_all_streaming_animations();
                        } else {
                            markdown.start_streaming_animation();
                        }
                        item_widget.draw_all_unscoped(cx);
                        continue;
                    }

                    if let Some(msg) = data.messages.get(item_id) {
                        // Full-screen splash app: don't echo the user's prompt —
                        // only the generated card is shown. Collapse the user
                        // item to zero height instead of rendering the bubble.
                        if matches!(msg.role, ChatRole::User) {
                            let item_widget = list.item(cx, item_id, id!(User));
                            item_widget.set_visible(cx, false);
                            item_widget.draw_all_unscoped(cx);
                            continue;
                        }
                        let is_animating = self.animating_msg == Some(item_id);
                        let template = match msg.role {
                            ChatRole::User => id!(User),
                            ChatRole::Assistant => id!(Assistant),
                        };
                        let item_widget = list.item(cx, item_id, template);
                        // Completed message — show the copy/share icons (PortalList
                        // pools items; this one may have been the hidden streaming
                        // item last frame). But NOT on an A2App card: copy/share act
                        // on the raw message text, which for a card is runsplash DSL,
                        // so the affordance is meaningless — hide both. User messages
                        // have neither button, so these are no-ops there.
                        let is_splash_card = msg.text.contains("```runsplash")
                            || msg.text.contains("```runplan");
                        item_widget
                            .button(cx, ids!(copy_button))
                            .set_visible(cx, !is_splash_card);
                        item_widget
                            .button(cx, ids!(share_button))
                            .set_visible(cx, !is_splash_card);
                        let mut markdown = item_widget.markdown(cx, ids!(selectable));
                        let empty_state = CardState::new();
                        let card_state = data.a2app_state.get(&item_id).unwrap_or(&empty_state);
                        // Only re-resolve + re-parse the card when its inputs actually
                        // change (new message or card state) — NOT every draw. Skipping
                        // this on scroll frames (nothing changed) is what keeps scrolling
                        // smooth; otherwise the whole DSL is re-parsed ~30ms every frame.
                        // A DRIVING card ignores the epoch here, for the same reason
                        // the poll no longer clears the cache for one: its live values
                        // set themselves from `fn tick()`, so re-resolving would
                        // compute the text it already shows — at the cost of
                        // re-parsing the document and rebuilding the `MapView` inside
                        // it, which is a measured 327 ms of frozen map.
                        //
                        // Clearing the cache and comparing the epoch are TWO gates.
                        // Skipping only the first left this one re-rendering anyway,
                        // and the re-resolve count went up rather than down.
                        // A driving card must not rebuild for DATA — that tears down
                        // the map, and it also snaps the swipe sheet shut, since a
                        // rebuilt tree starts from `visible: false`. Freezing the key
                        // is what does that.
                        //
                        // It must still rebuild for a TAP, and it does, by a different
                        // route: the tap handler bumps `CHAT_GENERATION`, and
                        // `draw_walk` above drops this cache whenever that moves. So
                        // the sentinel is safe here — verified on device by reverting
                        // it and watching `End` still switch screens.
                        //
                        // Worth writing down, because this comment previously blamed
                        // the sentinel for `End` being inert and that was wrong. The
                        // cause was in the profile: a `cycle` resolved its current
                        // value store → initial while the RENDERER resolves store →
                        // data → initial, so a host-seeded `screen: "drive"` was
                        // cycled from the declared `.plan` and advanced to `.drive` —
                        // the screen it was already on. Fixed in `splash-ui-l0`.
                        let epoch = if self.driving_card {
                            0
                        } else if L0_TYPING_PENDING.load(std::sync::atomic::Ordering::Relaxed) {
                            // FROZEN AT THE CACHED VALUE, not at a sentinel. A first
                            // version froze to 0 and 0 never matched the epoch the
                            // cache was written with — so while typing was pending
                            // every draw MISSED and re-resolved, which multiplied
                            // the rebuilds the flag exists to suppress.
                            self.rendered_cache.as_ref().map(|c| c.3).unwrap_or(0)
                        } else {
                            cx.script_data_fetch_epoch()
                        };
                        let unchanged = matches!(
                            &self.rendered_cache,
                            Some((cid, ctext, cstate, cepoch))
                                if *cid == item_id
                                    && ctext == &msg.text
                                    && cstate == card_state
                                    && *cepoch == epoch
                        );
                        if !unchanged {
                            let resolve_began = std::time::Instant::now();
                            // Which of the two gates opened. A driving card that
                            // re-resolves on data is the stutter; one that does not
                            // re-resolve on a tap is `End` doing nothing. Both were
                            // diagnosed from screenshots before this said which.
                            log!(
                                "[l0] resolve item {item_id}: driving={} key {:?} -> {epoch}",
                                self.driving_card,
                                self.rendered_cache.as_ref().map(|c| c.3)
                            );
                            // wrap_bare_latex wraps `\cmd{…}` with `$…$` so MathView can
                            // render them.
                            let unwrapped = unwrap_outer_markdown_fence(&msg.text);
                            // dev's runplan materialisation, then ours: a
                            // semantic plan becomes card markup BEFORE the L0
                            // resolve, and `resolve_a2app_card` keeps the `cx`
                            // the L0 path needs.
                            let materialized = materialize_runplan_for_display(unwrapped);
                            let rendered = wrap_bare_latex(&materialized);
                            let rendered = resolve_a2app_card(cx, &rendered, item_id, card_state);
                            // A card that FOLLOWS a position and ticks its own values
                            // must not be re-resolved on an epoch bump — see the
                            // epoch poll in `handle_event`. Both halves are required:
                            // the follow map is what makes a rebuild visible, and the
                            // tick is what makes skipping it correct.
                            // Locked in only once the card HAS ITS ROUTE.
                            //
                            // Suppressing the re-resolve is what stops the stutter, and
                            // doing it too early stops the card finishing: rendered
                            // before the route arrives, its polyline is empty, and the
                            // re-resolve was the only thing that would ever fill it in.
                            // Measured — a flat map with no ribbon, no puck, and a
                            // banner reading just "left", frozen that way for good.
                            //
                            // So all three must hold: it follows a live position, it
                            // ticks its own values, and the route it draws is already
                            // there. Until then it re-resolves like any other card.
                            let has_route = rendered.contains("nav_polyline: \"")
                                && !rendered.contains("nav_polyline: \"\"");
                            self.driving_card = rendered.contains("nav_mode: \"follow")
                                && rendered.contains("fn tick()")
                                && has_route;
                            log!(
                                "[l0] resolve took {} ms",
                                resolve_began.elapsed().as_millis()
                            );
                            markdown.set_text(cx, &rendered);
                            // The epoch AFTER resolving: a `sys.*` call during
                            // resolution can itself start a fetch and bump it,
                            // and caching the pre-resolve value would re-resolve
                            // every frame forever.
                            self.rendered_cache = Some((
                                item_id,
                                msg.text.clone(),
                                card_state.clone(),
                                // Matching the sentinel above, so a driving card's
                                // cache entry keeps comparing equal.
                                if self.driving_card {
                                    0
                                } else {
                                    cx.script_data_fetch_epoch()
                                },
                            ));
                        }
                        if is_animating {
                            markdown.stop_streaming_animation();
                        }
                        item_widget.draw_all_unscoped(cx);
                        // LOCAL DEBUG: where did this item actually land?
                        if std::env::var_os("MAKEPAD_GL_DRAW_TRACE").is_some() {
                            let r = item_widget.area().rect(cx);
                            log::info!(
                                "[CHATLIST] item {item_id} rect pos=({:.1},{:.1}) size=({:.1},{:.1})",
                                r.pos.x, r.pos.y, r.size.x, r.size.y
                            );
                        }
                        if is_animating && markdown.is_streaming_animation_done() {
                            self.animating_msg = None;
                        }
                    }
                }
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        // A fetch that landed since the last draw must reach the screen. Polled
        // rather than pushed because the fetch completes on another thread and
        // this widget has no hook into it; one second is well under what a user
        // reads as "stuck" and far above what would cost anything.
        // `Timer::default()` is empty, so the first event starts the loop and
        // every firing re-arms it.
        if self.epoch_poll.is_event(event).is_some() || self.epoch_poll.is_empty() {
            let epoch = cx.script_data_fetch_epoch();
            if epoch != self.last_fetch_epoch {
                self.last_fetch_epoch = epoch;
                // A DRIVING card is not re-resolved, and this is the rule the app
                // being replaced states in capitals: never introduce anything that
                // forces a rebuild while driving.
                //
                // Re-resolving re-parses the whole document, which tears down and
                // rebuilds the `MapView` inside it. Measured on a OnePlus 6: frame
                // hitches and card re-resolves correlate 1:1, up to 327 ms of frozen
                // map — the stutter, and no amount of camera smoothing hides it
                // because what stops is the UI thread.
                //
                // It is safe to skip precisely BECAUSE such a card carries a
                // `fn tick()`: its live values set themselves on named widgets every
                // frame, so a re-resolve would compute the same text it already has.
                // A card without one still re-resolves — that is how a list or a
                // forecast gets the values a fetch just delivered.
                //
                // Structure still rebuilds: a tap changes card state, and that
                // invalidates this cache by a different key.
                if !self.driving_card {
                    self.rendered_cache = None;
                }
                self.epoch_frame = cx.new_next_frame();
                self.view.redraw(cx);
            }
            self.epoch_poll = cx.start_timeout(1.0);
        }

        if let Event::Actions(actions) = event {
            let list = self.view.portal_list(cx, ids!(list));
            if list.any_items_with_actions(actions) {
                for (item_id, item) in list.items_with_actions(actions) {
                    let copy_btn = item.button(cx, ids!(copy_button));
                    if copy_btn.clicked(actions) {
                        let data = CHAT_DATA.read().unwrap();
                        if let Some(msg) = data.messages.get(item_id) {
                            cx.copy_to_clipboard(&msg.text);
                        }
                    }
                    // Share opens the OS share sheet (Android ACTION_SEND).
                    let share_btn = item.button(cx, ids!(share_button));
                    if share_btn.clicked(actions) {
                        let data = CHAT_DATA.read().unwrap();
                        if let Some(msg) = data.messages.get(item_id) {
                            cx.share_text(&msg.text);
                        }
                    }
                }
            }
        }
    }
}

// (W02 strip) — aichat's `BackendType` enum + `ALL_BACKENDS` constant + the
// inline `BackendType::system_prompt` (which baked the entire splash.md and
// diagram-kit JSON manual into the binary) lived here. They're gone: Octos
// serves all LLMs server-side and supplies system prompts per profile, so
// the client doesn't pick a backend or carry a prompt. See
// `05-AICHAT-REUSE-MAP.md` "Stuff we drop or replace" and
// `OCTOS_PLACEHOLDER_SYSTEM_PROMPT` near the top of this file. The original
// block lived at `aichat/examples/aichat/src/main.rs:1883–2072`.

/// Whether a keystroke's debounced rebuild is pending — see `l0_typing_rebuild`.
static L0_TYPING_PENDING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

std::thread_local! {
    /// Live touch Start points (uid -> (x, y, time)) for the L0 swipe gesture —
    /// the raw touch stream carries no start position at Stop, and the Script
    /// derive on App admits no field of this shape.
    static L0_TOUCH_STARTS: std::cell::RefCell<std::collections::HashMap<u64, (f64, f64, f64)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    /// When the last horizontal swipe fired. The tap overlays release on
    /// finger-up wherever it lands — a drag does not cancel them — so the
    /// same motion that pages a card also "taps" whatever row it crossed
    /// (measured: a manage-mode swipe opened the quote it swept over). A tap
    /// notify arriving within this window of a swipe is the swipe, not a tap.
    static L0_SWIPE_AT: std::cell::Cell<Option<std::time::Instant>> =
        std::cell::Cell::new(None);
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    /// Auto-dismiss timer for the toast strip (compaction / memory-saved /
    /// warnings). Empty when no toast is showing.
    #[rust]
    toast_timer: Timer,
    /// Trailing debounce for keystroke-driven L0 re-renders. Each `on_change`
    /// dispatch applies its state change immediately (the field echoes locally)
    /// and re-arms this; the card re-resolves once, ~a third of a second after
    /// the LAST keystroke, instead of ~120 ms of re-parse per character with the
    /// results rows re-laying the sheet out under the user's finger.
    ///
    /// `L0_TYPING_PENDING` rides with it: the epoch half of ChatList's cache key
    /// would otherwise re-resolve the card every time a partial query's search
    /// response LANDED — measured, three rebuilds per keystroke — so while the
    /// debounce is pending the epoch is frozen too, and the pause pays for one
    /// rebuild that reads whatever has arrived by then.
    #[rust]
    l0_typing_rebuild: Timer,
    /// After a card renders, a brief repaint burst so the newest card's remote
    /// background image adopts its decoded texture from the ImageCache (the
    /// Image widget self-heals on draw, but the app is otherwise idle after the
    /// card lands, so nothing would trigger that draw). Parks after a few ticks.
    #[rust]
    settle_timer: Timer,
    #[rust]
    settle_ticks: u32,
    /// ~10 Hz repaint driver while a turn streams. Deltas only accumulate
    /// text + set `stream_dirty`; this interval turns them into redraws so a
    /// fast token stream doesn't re-parse/redraw the thread per token.
    #[rust]
    stream_tick: Timer,
    /// Set by delta handlers; cleared when the tick repaints.
    #[rust]
    stream_dirty: bool,
    /// Layer 3 — a background app's badge/title changed during this event
    /// drain; the switcher strip is re-synced once after the drain (rather than
    /// per streaming delta). Cleared by the flush.
    #[rust]
    tabs_dirty: bool,
    /// "A2App" composer toggle: when on, the next message is wrapped with the
    /// Splash UI-generation prompt so the LLM returns a `runsplash` block that
    /// renders as live UI.
    #[rust]
    splash_mode: bool,
    /// Whether the Splash manual has already been sent into the current
    /// session. octos sessions are stateful server-side, so the ~85KB manual
    /// is primed once (first A2App message); later A2App messages send only a
    /// short instruction, avoiding re-sending it every turn. Reset on new chat.
    #[rust]
    splash_primed: bool,
    /// Whether the floating composer is expanded. It auto-collapses to the
    /// reveal pill after a card renders (full-screen viewing), and expands
    /// again when the pill is tapped. Initialized true in `handle_startup`.
    #[rust]
    composer_shown: bool,
    /// One-shot guard for the `OCTOS_SEED_PROMPT` build-time seed (see
    /// `handle_actions`). Only used when that env var is set at compile time.
    #[rust]
    seed_prompt_sent: bool,
    /// One-shot guard for the `OCTOS_SEED_CARD` build-time card injection.
    #[rust]
    seed_card_shown: bool,
    /// Frames the seed card has waited for the youtube live-id resolver.
    #[rust]
    seed_card_waits: usize,
    /// Pending (destination, origin) to seed into the nav card's state once its
    /// message exists. Only set by the `OCTOS_SEED_CARD=nav` build-time seed.
    #[rust]
    seed_nav_state: Option<(String, Option<String>)>,
    /// Single OctosUiAgent instance — replaces aichat's `Box<dyn Agent>`
    /// dynamic dispatch over LLM backends. Lazily constructed on first use.
    #[rust]
    agent: Option<Box<dyn Agent>>,
    /// Open apps, each backed by an octos session. Empty until the first
    /// session opens (`clear_chat` at boot pushes the first). Layer 3 / W08.
    #[rust]
    apps: Vec<AppRecord>,
    /// Index into `apps` of the visible (foreground) app. Only meaningful when
    /// `apps` is non-empty; the `fg*` accessors return `None`/no-op otherwise.
    #[rust]
    foreground: usize,
    /// AMA (Activity Management Agent) session — the routing brain, running
    /// CONCURRENTLY with the app agents. Every user intent is broadcast to both
    /// the AMA and the app agents; the AMA classifies which app should own the
    /// screen. MVP: it renders nothing (its stream is logged, not shown).
    #[rust]
    ama_session: Option<SessionId>,
    /// The AMA's in-flight classification turn (so its stream is routed to the
    /// AMA log, never to the visible CHAT_DATA).
    #[rust]
    ama_prompt: Option<PromptId>,
    /// DEV-GOAL HARNESS (self-evolving app dev). A hidden master session on the
    /// phone's own kernel; its stream is collected here and never rendered. The
    /// host bridge is three world-writable files under /data/local/tmp — goal
    /// in, card out, findings in — so the model on the DEVICE does the
    /// developing and the host only ferries bytes.
    #[rust]
    dev_session: Option<SessionId>,
    #[rust]
    dev_prompt: Option<PromptId>,
    #[rust]
    dev_text: String,
    #[rust]
    dev_round: u32,
    /// A cancelled AMA routing prompt whose late deltas must be DROPPED, not
    /// streamed into the foreground card. Cancel clears `ama_prompt`
    /// synchronously, but the server interrupt is async — a delta already in
    /// flight would otherwise no longer match `ama_prompt`, fall through the
    /// foreground guard, and leak as card text. Cleared when its TurnComplete
    /// finally arrives.
    #[rust]
    cancelled_ama: Option<PromptId>,
    /// Accumulates the AMA's streamed routing decision for logging.
    #[rust]
    ama_text: String,
    /// The user intent captured at submit, held while the AMA classifies it. On
    /// the AMA's TurnComplete we dispatch this to the routed domain agent (that
    /// agent then generates its card and takes the screen). None when idle.
    #[rust]
    pending_intent: Option<String>,
    /// Currently-selected Octos profile id (X-Profile-Id on the wire).
    /// `None` until W08 hydrates the profile list. Used by `update_status`.
    #[rust]
    current_profile: Option<ProfileId>,
    /// `(profile_id, display_label)` pairs for the top-bar dropdown.
    /// Empty in M1 — W08 calls `set_labels` once `/api/my/profile` lands.
    #[rust]
    available_profiles: Vec<(ProfileId, String)>,

    // ---- W08 — login flow state -------------------------------------------
    //
    // These are flat instead of an enum because the LoginScreen DSL keeps
    // the three step containers and toggles their `visible` flag, mirroring
    // the four-state machine in `workstreams/W08-auth-tenancy.md`
    // § "LoginScreen flow" (`Idle` / `SendingCode` / `AwaitingCode` /
    // `Verifying`). Verbose enum mapping isn't worth the indirection here.

    /// Once `Continue` (Step 1) succeeds we cache the parsed URL + profile
    /// id here so the email / verify steps can build a `RestClient` without
    /// re-reading `~/.config/octos-app/server.json`.
    #[rust]
    login_server_url: Option<url::Url>,
    /// Mirror of `ProfileId` from server config; threaded into the keychain
    /// service-name on a successful verify.
    #[rust]
    login_profile_id: Option<ProfileId>,
    /// Stashed across the Step 2 → Step 3 transition so `Verify` can resend
    /// the same email the OTP was issued against.
    #[rust]
    login_pending_email: Option<String>,

    /// W05 — handle exposed by `OctosUiAgent::approval_handle`, captured at
    /// agent-construction time so `App::handle_actions` can issue
    /// `approval/respond` without downcasting `Box<dyn Agent>`.
    /// Cheap-clone (`Sender<OutboundCommand>` + `tokio::runtime::Handle`).
    #[rust]
    approval_handle: Option<crate::backend::octos_ui::ApprovalHandle>,
    /// One-shot `task/output/read` handle for the coding task drill-down.
    #[rust]
    task_output_handle: Option<crate::backend::octos_ui::TaskOutputHandle>,
}

impl App {
    // ---- Layer 3 (W08 Phase 2) — foreground-app accessors -----------------
    //
    // These replace the old single `session_id` / `current_prompt` fields.
    // `apps[foreground]` is the source of truth for the visible app; the
    // helpers keep the ~dozen call sites terse and make "which app owns this
    // event" explicit (streaming events carry a `prompt_id`, not a session id).

    /// The visible app, if any (`None` before the first session opens).
    fn fg(&self) -> Option<&AppRecord> {
        self.apps.get(self.foreground)
    }
    fn fg_mut(&mut self) -> Option<&mut AppRecord> {
        let i = self.foreground;
        self.apps.get_mut(i)
    }
    /// Foreground session id (replaces the old single `session_id` field).
    fn fg_session(&self) -> Option<SessionId> {
        self.fg().map(|a| a.session_id)
    }
    /// Take the foreground app's in-flight prompt (used by cancel).
    fn fg_prompt_take(&mut self) -> Option<PromptId> {
        self.fg_mut().and_then(|a| a.current_prompt.take())
    }
    /// Set the foreground app's in-flight prompt (replaces `current_prompt =`).
    fn set_fg_prompt(&mut self, p: Option<PromptId>) {
        if let Some(a) = self.fg_mut() {
            a.current_prompt = p;
        }
    }
    /// Index of the app whose in-flight turn is `prompt_id`, if any tracks it.
    /// `None` means orphan (cancelled/stale) — callers treat that as foreground
    /// to preserve the pre-Layer-3 single-app fallback behavior.
    fn app_of_prompt(&self, prompt_id: PromptId) -> Option<usize> {
        self.apps
            .iter()
            .position(|a| a.current_prompt == Some(prompt_id))
    }
    /// Bring the app holding `sid` to the foreground, opening a light record if
    /// this session isn't an app yet. Clears its unread badge. Path B: the
    /// caller then hydrates `CHAT_DATA` from this session's server history.
    fn focus_session(&mut self, sid: SessionId, title: impl Into<String>) {
        match self.apps.iter().position(|a| a.session_id == sid) {
            Some(i) => self.foreground = i,
            None => {
                self.apps.push(AppRecord::new(sid, title));
                self.foreground = self.apps.len() - 1;
            }
        }
        if let Some(a) = self.fg_mut() {
            a.has_updates = false;
        }
    }

    /// AMA "decision → activation": the AMA classified the held `pending_intent`
    /// into `app_id` (a domain). Activate the app agent whose `domain` matches —
    /// foreground it and dispatch the domain-specialised generation prompt to it,
    /// so THAT agent generates its card and takes the screen. An unknown domain
    /// (e.g. "none") renders nothing.
    fn route_to_app(&mut self, cx: &mut Cx, app_id: &str, decision: &str) {
        let Some(intent) = self.pending_intent.take() else {
            return;
        };
        let Some(idx) = self
            .apps
            .iter()
            .position(|a| a.domain.as_deref() == Some(app_id))
        else {
            log::info!("AMA → route: {app_id:?} (no app agent for this domain) | {decision}");
            CHAT_DATA.write().unwrap().is_streaming = false;
            self.ui.redraw(cx);
            return;
        };
        log::info!("AMA → activate '{app_id}' app agent (idx {idx}) | {decision}");
        // Remember what this card is FOR, so a repair turn can restate it.
        self.apps[idx].last_request = Some(intent.clone());
        // This domain agent takes the screen.
        self.foreground = idx;
        // A non-web app taking the screen must not leave a web card's native
        // WebView overlay floating above its Splash card.
        if app_id != "web" && app_id != "youtube" {
            cx.system_browser(web_card_browser_id()).detach();
        }
        // youtube WAS served here as a complete hand-authored HTML app, with
        // its live ids patched in at serve time. It is an L0 card now (see
        // L0_APPS): the card SEARCHES rather than carrying ids the model
        // remembered — which is what the patching existed to paper over —
        // and hands a player url to sys.link for playback, so the WebView is
        // still what plays the video and no longer what draws the app.
        // NAV IS GENERATED, not served.
        //
        // It was direct-served: the client emitted the 664-line L2 trip planner
        // verbatim because "the on-device model under-generates / truncates this
        // ~14 KB card". That rationale was about the CARD'S SIZE, and the card is
        // not that size any more — `apps/nav/exemplar.card` is the same screen in
        // 92 lines of L0, which is a request a capable model answers.
        //
        // So nav now falls through to the ordinary generation path with every
        // other app. What it still needs is the TRIP: the AMA already parsed the
        // origin and destination out of the request onto its decision line, and
        // that has to reach the model or the card generates with empty search
        // boxes and the user types the place they just said.
        let intent = if app_id == "nav" {
            let (orig, dest) = parse_nav_places(decision);
            let dest = dest.or_else(|| extract_nav_destination(&intent));
            match (orig, dest) {
                (Some(o), Some(d)) => format!(
                    "{intent}\n\nThe trip is FROM \"{o}\" TO \"{d}\". Put those in the card's \
                     own state as the initial origin and destination queries, so it opens on that \
                     route instead of on an empty search box."
                ),
                (None, Some(d)) => format!(
                    "{intent}\n\nThe destination is \"{d}\". Put it in the card's own state as \
                     the initial destination query, so it opens on that route instead of on an \
                     empty search box. The origin is the device's position."
                ),
                _ => intent,
            }
        } else {
            intent
        };
        // New foreground → drop ChatList's render cache so the card re-parses.
        CHAT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Dispatch the domain-specialised generation prompt to the chosen agent.
        // Every app (stock included) is generated by its app agent from the
        // requirements spec in its injected memory — nothing is baked into the
        // client, and there are no exemplars: the agent assembles the app
        // from the spec + widget patterns (stock is ONE combined list+detail
        // card navigating client-side via `set`/`selected`, no per-tap LLM
        // round-trip).
        let sid = self.apps[idx].session_id;
        let prompt = app_splash_router_for(app_id, &intent);
        let pid = self.agent.as_mut().unwrap().send_prompt(cx, sid, &prompt);
        self.apps[idx].current_prompt = Some(pid);
        // Fresh intent → fresh repair budgets (see card_lint / L0_REPAIR_BUDGET).
        self.apps[idx].repair_attempted = false;
        self.apps[idx].l0_repair_attempts = 0;
        self.sync_app_tabs(cx);
        self.ui.redraw(cx);
    }

    /// AMA "compose → activation" (the dynamic-composition path): the AMA found
    /// NO existing app for the held intent, authored a brand-new app spec into
    /// the injected memory tree (`apps/<app_id>/app.md`), and answered
    /// `compose <app_id> — <reason>`. The client's part is only plumbing:
    /// create a NEW peer app-agent session for that id — a FRESH session gets
    /// the memory tree (now containing the new spec) injected on open, so the
    /// new agent generates the new app with clean, dedicated context — then
    /// route the still-held intent to it exactly like a boot-time domain agent.
    /// Extract `(is_compose, app_id)` from the AMA's raw reply. Contract:
    /// `<appid> — <reason>` / `compose <id> — <reason>` / `none`. Robust to the
    /// model narrating first and running the decision onto the same line with
    /// no newline (a line/first-token heuristic then grabs a narration word).
    /// Anchor on the em-dash separator: the id is the last token BEFORE the
    /// last `—`, and it's a compose if the token before that is "compose".
    /// Falls back to the last non-empty line's first token (covers a bare
    /// `none` / `weather` with no em-dash).
    fn parse_ama_decision(text: &str) -> (bool, String) {
        // Normalize an id token: keep [a-z0-9-], strip leading/trailing hyphens
        // ("weather-news-" -> "weather-news"), lowercase, cap length.
        let clean = |tok: &str| -> String {
            let kept: String = tok
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            kept.trim_matches('-').to_ascii_lowercase().chars().take(40).collect()
        };
        // 1. Compose keyword: "compose <id>". A composed id is ALWAYS multi-part
        //    (`<a>-<b>`), so require a '-' in the token — that rejects a reason
        //    that merely mentions "compose a plan" ("a" has no dash).
        let lower = text.to_ascii_lowercase();
        if let Some(pos) = lower.rfind("compose ") {
            let after = &text[pos + "compose ".len()..];
            let tok = after
                .split(|c: char| c.is_whitespace() || c == '\u{2014}')
                .next()
                .unwrap_or("");
            let id = clean(tok);
            if id.contains('-') {
                return (true, id);
            }
        }
        // 2. `<id> — <reason>`: the FIRST em-dash is the separator (a reason may
        //    itself contain em-dashes, which defeats rfind). Take the token
        //    right before it.
        if let Some(dash) = text.find('\u{2014}') {
            let before = text[..dash].trim_end();
            let id = before
                .rsplit(|c: char| c.is_whitespace())
                .next()
                .map(clean)
                .unwrap_or_default();
            if !id.is_empty() {
                return (false, id);
            }
        }
        // 3. Bare decision, no em-dash: last non-empty line, first token.
        let line = text
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("");
        let mut toks = line.split(|c: char| c.is_whitespace());
        let first = toks.next().map(clean).unwrap_or_default();
        if first == "compose" {
            return (true, toks.next().map(clean).unwrap_or_default());
        }
        (false, first)
    }

    fn compose_app(&mut self, cx: &mut Cx, app_id: &str, decision: &str) {
        // Idempotent: if a peer agent for this domain already exists (the AMA
        // re-composed an app from earlier in this run), just activate it.
        if self.apps.iter().any(|a| a.domain.as_deref() == Some(app_id)) {
            self.route_to_app(cx, app_id, decision);
            return;
        }
        // Guard against a HALLUCINATED app id: the AMA may name (or "compose")
        // a domain whose spec doesn't exist on disk — the fresh peer would then
        // be told to follow a nonexistent `apps/<id>/app.md` and produce
        // nothing useful, silently with no lint (no rules to load). Require the
        // spec to be present before spinning one up; otherwise fall back to the
        // held intent's default so the user still gets a card.
        if Self::app_spec_exists(app_id) {
            // fall through and create the peer
        } else {
            log::warn!(
                "AMA named unknown app '{app_id}' (no apps/{app_id}/app.md) | {decision} — \
                 falling back to weather"
            );
            self.route_to_app(cx, "weather", "unknown app fallback");
            return;
        }
        let Some(agent) = self.agent.as_mut() else {
            return;
        };
        log::info!("AMA → compose '{app_id}' (new peer agent) | {decision}");
        // Mirror the boot path (`clear_chat`) exactly: same SessionConfig, same
        // client-side `create_session` — it allocates the SessionId and fires
        // `session/open`; the generation prompt queues behind it on the stdio
        // pipe, so routing immediately after is safe.
        let config = SessionConfig {
            system_prompt: Some(OCTOS_PLACEHOLDER_SYSTEM_PROMPT.to_string()),
            ..Default::default()
        };
        let sid = agent.create_session(cx, config);
        // Boot titling convention, derived from the id:
        // "weather-activity" → "Weather Activity".
        let title = app_id
            .split('-')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut chars = s.chars();
                match chars.next() {
                    Some(f) => f.to_ascii_uppercase().to_string() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        self.apps.push(AppRecord::with_domain(sid, title, app_id));
        self.sync_app_tabs(cx);
        // `route_to_app` finds the record just pushed, foregrounds it, and
        // consumes `pending_intent` — the intent was deliberately left pending
        // until this point so the new agent receives it.
        self.route_to_app(cx, app_id, decision);
    }

    /// Construct an `OctosUiAgent` from the current process environment.
    /// W08 will plumb the bearer + profile through `octos-app-store::auth`
    /// and the keychain; for now we read placeholders so the binary boots
    /// without a server. Returns the boxed `Agent` so `App::agent` can stay
    /// `Option<Box<dyn Agent>>` and the streaming pipeline keeps working.
    ///
    /// Replaces aichat's per-backend `create_agent` match arm.
    /// Returns the boxed agent + the W05 approval handle (captured before
    /// the box hides the concrete type).
    fn create_octos_agent(
        transport_config: TransportConfig,
    ) -> (
        Box<dyn Agent>,
        crate::backend::octos_ui::ApprovalHandle,
        crate::backend::octos_ui::TaskOutputHandle,
    ) {
        let agent = OctosUiAgent::new(transport_config);
        let approval_handle = agent.approval_handle();
        let task_output_handle = agent.task_output_handle();
        (Box::new(agent) as Box<dyn Agent>, approval_handle, task_output_handle)
    }

    /// (Re)build the REST client + `OctosUiAgent` from the on-disk
    /// config/token state. Runs at boot and again after a successful login,
    /// so the WS transport picks up a fresh bearer without an app restart
    /// (the replaced agent drops its runtime + socket).
    ///
    /// W04 — the REST session hydrate fires before the agent steals the
    /// config. Empty bearer means we expect a 401; the failure path is
    /// silent in M1. W04 follow-up #5 — `/api/version` probe runs
    /// off-thread so we don't stall the caller.
    fn connect_transport(&mut self, cx: &mut Cx) {
        let transport_config = Self::placeholder_transport_config();
        log::info!(
            "connect transport: base_url={} profile_id={}",
            transport_config.base_url, transport_config.profile_id.0
        );
        // M12 D-5 — `GET /api/sessions` is retired server-side; the sidebar
        // hydrates over the WS (`session/list`) once `session/open` lands
        // (see `OctosUiAgent`'s `CapabilityNegotiated` arm). Only the public
        // version probe stays on REST.
        Self::probe_version(Self::build_rest_client(&transport_config));
        // Reflect the signed-in identity in the top bar: the Profile pill
        // previously shipped its "(no profile)" stub forever.
        let pid_str = transport_config.profile_id.0.clone();
        if !pid_str.is_empty() {
            self.available_profiles =
                vec![(ProfileId::from(pid_str.clone()), pid_str.clone())];
            self.current_profile = Some(ProfileId::from(pid_str.clone()));
            let dd = self.ui.drop_down(cx, ids!(backend_dropdown));
            dd.set_labels(cx, vec![pid_str]);
            dd.set_selected_item(cx, 0);
        }
        self.update_status(cx);
        let (agent, approval_handle, task_output_handle) =
            Self::create_octos_agent(transport_config);
        self.agent = Some(agent);
        self.approval_handle = Some(approval_handle);
        self.task_output_handle = Some(task_output_handle);
    }

    /// Build a `RestClient` from a `TransportConfig`. Used by W04 to hydrate
    /// the session list and to issue `DELETE /api/sessions/{id}`. Cheap —
    /// `reqwest::Client::new()` is `Arc`-shaped internally.
    fn build_rest_client(cfg: &TransportConfig) -> octos_app_transport::rest::RestClient {
        octos_app_transport::rest::RestClient::new(
            reqwest::Client::new(),
            cfg.base_url.clone(),
            cfg.bearer.clone(),
            cfg.profile_id.clone(),
        )
    }

    /// W04 follow-up #5 — fire `GET /api/version` once at boot. Logs the
    /// version + service, warns if the version doesn't start with `0.` /
    /// `1.` (so a mis-pointed server surfaces in the logs without
    /// blocking the boot path), and warns if `service != "octos"`.
    /// Off-thread; failures are silent (the live smoke can hit servers
    /// that don't serve `/api/version` yet).
    fn probe_version(client: octos_app_transport::rest::RestClient) {
        let _ = std::thread::Builder::new()
            .name("octos-version-probe".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::warn!("version probe: spawn tokio runtime: {e}");
                        return;
                    }
                };
                match rt.block_on(async { client.version_probe().await }) {
                    Ok(probe) => {
                        let version = probe.version_string();
                        let service = probe.service().map(str::to_owned);
                        log::info!(
                            "version probe: version={} service={}",
                            version.as_deref().unwrap_or("<unknown>"),
                            service.as_deref().unwrap_or("<unknown>"),
                        );
                        if let Some(v) = version.as_deref() {
                            if !v.starts_with("0.") && !v.starts_with("1.") {
                                log::warn!(
                                    "version probe: server reported {v}; expected 0.x or 1.x"
                                );
                            }
                        }
                        if let Some(s) = service.as_deref() {
                            if s != "octos" {
                                log::warn!(
                                    "version probe: service={s}; expected \"octos\" — wrong server?"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("version probe failed: {e}");
                    }
                }
            });
    }

    /// Build a `TransportConfig` for the boot REST hydrate + the WS agent.
    ///
    /// Resolution precedence (matches `boot_is_authed`):
    ///
    /// 1. **`~/.config/octos-app/server.json`** if present — `server_url`
    ///    becomes the REST `base_url`, `profile_id` is the `X-Profile-Id`.
    /// 2. **Bearer**: `OCTOS_APP_TOKEN` env var first (dev shortcut), else
    ///    `keychain::load_token(host, profile_id)` from the OS keychain.
    ///    Empty bearer is fine — REST will respond 401 and the failure path
    ///    is silent in M1.
    /// 3. **Fallback** (no server.json): the legacy `OCTOS_BASE_URL` /
    ///    `OCTOS_BEARER` / `OCTOS_PROFILE_ID` env vars + `https://localhost:8080`
    ///    so headless CI / `cargo run` without any config still boots.
    ///
    /// Renaming this away from the W01-era `placeholder_transport_config` is
    /// deferred — call sites also live in `handle_actions` and a rename
    /// would balloon the diff. The doc comment carries the new semantics.
    fn placeholder_transport_config() -> TransportConfig {
        // 1. server.json — happy path on a configured machine.
        if let Some(cfg) = crate::app::login::load_server_config() {
            if let Ok(base_url) = url::Url::parse(&cfg.server_url) {
                let profile_id = TransportProfileId::new(cfg.profile_id.clone());
                let bearer = Self::resolve_bearer(&base_url, &cfg.profile_id);
                return TransportConfig {
                    base_url,
                    bearer,
                    profile_id,
                    cursor: None,
                    cursor_file: Self::cursor_file_path(),
                    requested_capabilities: Capabilities::requested(),
                    workspace_cwd: Self::current_workspace_cwd(),
                    stdio: Self::stdio_spawn(),
                };
            } else {
                log::warn!(
                    "server.json server_url failed to parse; falling back to OCTOS_BASE_URL env"
                );
            }
        }

        // 2. Env-only fallback (no server.json yet).
        let base_url = std::env::var("OCTOS_BASE_URL")
            .ok()
            .and_then(|s| url::Url::parse(&s).ok())
            .unwrap_or_else(|| {
                url::Url::parse("https://localhost:8080").expect("static URL is valid")
            });
        let bearer = SecretString::new(std::env::var("OCTOS_BEARER").unwrap_or_default());
        let stdio = Self::stdio_spawn();
        // The embedded kernel's local profile is `_main` (its on-disk profile
        // id, where the LLM provider config lives) — `session/open` naming
        // anything else (the old `default` fallback) is rejected with
        // "profile 'default' is not configured for this AppUI session".
        let profile_id = TransportProfileId::new(
            std::env::var("OCTOS_PROFILE_ID").unwrap_or_else(|_| {
                if stdio.is_some() {
                    "_main".to_string()
                } else {
                    "default".to_string()
                }
            }),
        );
        TransportConfig {
            base_url,
            bearer,
            profile_id,
            cursor: None,
            cursor_file: Self::cursor_file_path(),
            requested_capabilities: Capabilities::requested(),
            workspace_cwd: Self::current_workspace_cwd(),
            stdio,
        }
    }

    /// Where per-session replay cursors persist (W08) so they survive a transport
    /// re-spawn / app restart — under the app's HOME, next to the saved cards.
    /// `None` (no HOME) falls back to in-memory cursors.
    fn cursor_file_path() -> Option<std::path::PathBuf> {
        std::env::var("HOME")
            .ok()
            .map(|h| std::path::PathBuf::from(h).join("a2app-cursors.json"))
    }

    /// Build the stdio-transport spawn spec. On Android the app runs the
    /// bundled `octos` binary as `serve --stdio` instead of dialing a
    /// WebSocket: no `octos serve` daemon, no TCP port. `untrusted_app` can
    /// only exec from its nativeLibraryDir, so the binary must ship there as a
    /// `lib*.so`; we locate that dir from our own mapped `libmakepad.so`.
    /// `HOME` points at an app-private octos home whose
    /// `.config/octos/config.json` carries the provider + inline key — so the
    /// app process never holds the LLM secret. Returns `None` (⇒ WebSocket) on
    /// desktop, or on Android when the bundled binary is absent (safe
    /// fallback: the app still boots against a remote `octos serve`).
    /// Locate the embedded octos kernel binary: (1) the APK-bundled lib in our
    /// nativeLibraryDir, (2) a staged copy in the app's private files dir. (2)
    /// exists for /system/priv-app installs: PackageManager does NOT extract
    /// native libs for system apps, and the system partition (or the emulator's
    /// overlayfs scratch) is too small for the 80MB+ kernel — so a priv-app
    /// deployment ships libmakepad/libstd beside the APK and stages
    /// liboctos.so into the app's data dir out-of-band (see
    /// docs/SYSTEM-APP.md). The app itself is a full SYSTEM+PERSISTENT
    /// component either way.
    #[cfg(target_os = "android")]
    fn find_embedded_kernel(
        lib_dir: &std::path::Path,
        _home: &std::path::Path,
    ) -> Option<std::path::PathBuf> {
        // The APK's native-lib dir, and ONLY that. A copy under the app's own
        // octos-home cannot be exec'd on Android 10+: W^X forbids executing
        // from app-writable storage, so the spawn dies with
        // `avc: denied { execute_no_trans }` — measured on a OnePlus 6T, where
        // stdio found the file, spawned it, and SELinux killed it. Keeping that
        // candidate read as a supported side-load and was not one. (The ohos
        // arm below still lists it: untested there, so left alone.)
        [lib_dir.join("liboctos.so")]
            .into_iter()
            .find(|p| p.exists())
    }

    /// True when an embedded kernel is available — the app then talks to a
    /// trusted local process over stdio and needs NO HTTP auth (see the boot
    /// decision in `handle_startup`).
    #[cfg(target_os = "android")]
    fn has_embedded_kernel() -> bool {
        let home = std::path::PathBuf::from("/data/user/0/dev.makepad.octos_app/files/octos-home");
        Self::native_lib_dir()
            .map(|lib_dir| Self::find_embedded_kernel(&lib_dir, &home).is_some())
            .unwrap_or(false)
    }

    #[cfg(target_os = "android")]
    fn stdio_spawn() -> Option<StdioSpawn> {
        let lib_dir = Self::native_lib_dir()?;
        let home = std::path::PathBuf::from("/data/user/0/dev.makepad.octos_app/files/octos-home");
        let Some(program) = Self::find_embedded_kernel(&lib_dir, &home) else {
            log::warn!(
                "stdio: bundled octos not found under {}; using WebSocket transport",
                lib_dir.display()
            );
            return None;
        };
        // Ensure HOME exists BEFORE spawning: `Command::spawn` chdir's into
        // `cwd` before exec, so a missing octos-home makes the spawn fail with
        // ENOENT ("No such file or directory") even though the binary is fine —
        // and since the server never starts, it never creates octos-home, so the
        // failure is permanent once the dir is absent (e.g. after `pm clear`).
        // Creating it here makes the spawn robust regardless of data state.
        if let Err(e) = std::fs::create_dir_all(&home) {
            log::warn!("stdio: could not create HOME {}: {e}", home.display());
        }
        Self::ensure_kernel_memory_budget(&home);
        log::info!("stdio: octos={} HOME={}", program.display(), home.display());
        // OCTOS_SKILLS_PATH adds the a2app memory dir as a skill READ-ZONE
        // (config.rs plugin_dirs_from_project → skill_read_zones), so the
        // splash-gen sub-agent's read_file can reach it by absolute path even
        // though file tools are otherwise fenced to the per-session workspace.
        let a2app = home.join("a2app").to_string_lossy().into_owned();
        let mut env = vec![
            ("HOME".to_owned(), home.to_string_lossy().into_owned()),
            ("OCTOS_SKILLS_PATH".to_owned(), a2app),
            // TEMP diagnostics: surface the embedded server's INFO trace
            // (subagent token counts, stop_reason) to logcat via the
            // stderr→log::info bridge, to pin the serve-relay truncation.
            ("RUST_LOG".to_owned(), "info".to_owned()),
        ];
        // Route octos's LLM HTTPS through a proxy when the device itself has no
        // internet route — e.g. an `adb reverse` tunnel to the dev host, which
        // reaches api.z.ai. Set via launch intent extra `makepad.OCTOS_PROXY`
        // (→ env MAKEPAD_OCTOS_PROXY, e.g. "http://127.0.0.1:8899"). reqwest
        // honours HTTP(S)_PROXY and CONNECT-tunnels HTTPS through it.
        if let Ok(proxy) = std::env::var("MAKEPAD_OCTOS_PROXY") {
            let proxy = proxy.trim().to_owned();
            if !proxy.is_empty() {
                log::info!("stdio: octos LLM proxy = {proxy}");
                for k in ["HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy", "ALL_PROXY"] {
                    env.push((k.to_owned(), proxy.clone()));
                }
            }
        }
        Some(StdioSpawn {
            program,
            args: vec!["serve".to_owned(), "--stdio".to_owned()],
            env,
            cwd: Some(home),
        })
    }

    /// Ensure the KERNEL config (`octos-home/.config/octos/config.json`)
    /// carries a `memory.max_inject_tokens` big enough for the a2app card
    /// memory. octos's built-in default is 2500 tokens; the assembled
    /// `app-cards/` tree is ~23k and grows with every drop-in app, and an
    /// over-budget tree is truncated SILENTLY at inject time — the app agent
    /// then never sees the framework manual/exemplars, improvises binding
    /// syntax, and cards render with empty values. The knob moved out of the
    /// profile JSON (the old BUILDING-ANDROID.md sed targeted a `_main.json`
    /// key the current profile schema no longer has), so the app maintains it
    /// in the one place the current kernel reads it from: the kernel config
    /// file. Config file rather than spawn env on purpose — env propagation
    /// on Android is not reliable across process restarts/re-exec.
    /// Merge-only: every other key is preserved, an EXPLICIT existing value
    /// wins (operators can tune it), and an unparseable file is left alone
    /// (the kernel surfaces the parse error itself).
    #[cfg(target_os = "android")]
    fn ensure_kernel_memory_budget(home: &std::path::Path) {
        const INJECT_BUDGET_TOKENS: u64 = 40_000;
        let path = home.join(".config/octos/config.json");
        let mut root = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(v) if v.is_object() => v,
                _ => {
                    log::warn!(
                        "stdio: {} is not a JSON object; memory budget NOT ensured",
                        path.display()
                    );
                    return;
                }
            },
            Err(_) => serde_json::json!({}),
        };
        let mut changed = false;
        {
            let memory = root
                .as_object_mut()
                .unwrap()
                .entry("memory")
                .or_insert_with(|| serde_json::json!({}));
            match memory.as_object_mut() {
                // Upgrade an ABSENT or too-LOW budget. A device provisioned
                // under the old flow can carry an explicit `2500` (octos's
                // pre-app-cards default) — that silently truncates the ~23k
                // tree, so treat any numeric value below our floor the same as
                // absent. A value >= the floor (an operator's deliberate tune)
                // is respected; a non-numeric value is left alone.
                Some(memory)
                    if memory
                        .get("max_inject_tokens")
                        // as_f64 accepts both ints and JSON floats (2500.0) — a
                        // previously-provisioned float default was otherwise
                        // read as "unparseable, present" and left un-upgraded.
                        .and_then(|v| v.as_f64())
                        .map(|n| n < INJECT_BUDGET_TOKENS as f64)
                        .unwrap_or(!memory.contains_key("max_inject_tokens")) =>
                {
                    memory.insert(
                        "max_inject_tokens".into(),
                        serde_json::json!(INJECT_BUDGET_TOKENS),
                    );
                    changed = true;
                }
                Some(_) => {}
                None => log::warn!(
                    "stdio: kernel config `memory` is not an object; leaving it alone"
                ),
            }
        }
        // The AMA composer session is cwd-hinted into the app-cards memory
        // tree; without this knob the kernel relocates that session's
        // transcripts into the card tree (`appui.sessions_in_cwd` defaults
        // true). Same merge contract as the memory budget: absent-only, an
        // explicit operator value wins.
        {
            let appui = root
                .as_object_mut()
                .unwrap()
                .entry("appui")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(appui) = appui.as_object_mut() {
                if !appui.contains_key("sessions_in_cwd") {
                    appui.insert("sessions_in_cwd".into(), serde_json::json!(false));
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let bytes = match serde_json::to_vec_pretty(&root) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("stdio: serialize kernel config: {e}");
                return;
            }
        };
        match std::fs::write(&path, bytes) {
            Ok(()) => log::info!(
                "stdio: set memory.max_inject_tokens={INJECT_BUDGET_TOKENS} in {}",
                path.display()
            ),
            Err(e) => log::warn!("stdio: write {}: {e}", path.display()),
        }
    }

    /// Directory holding the app's packaged native libraries, found by scanning
    /// `/proc/self/maps` for our own mapped `libmakepad.so`. Android and
    /// OpenHarmony are both Linux-kernel platforms that mount `/proc`, and
    /// neither hands the app its lib dir directly (the bundle path carries an
    /// install-specific prefix), so this is identical on both.
    #[cfg(mobile)]
    fn native_lib_dir() -> Option<std::path::PathBuf> {
        let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
        for line in maps.lines() {
            let Some(slash) = line.find('/') else { continue };
            let path = &line[slash..];
            if path.ends_with("/libmakepad.so") {
                return std::path::Path::new(path).parent().map(|p| p.to_path_buf());
            }
        }
        None
    }

    /// The app-private read/write root inside the OpenHarmony sandbox.
    #[cfg(target_env = "ohos")]
    fn ohos_home() -> std::path::PathBuf {
        std::path::PathBuf::from("/data/storage/el2/base/files/octos-home")
    }

    /// Bundled kernel path, if present. Mirrors the Android layout: the binary
    /// ships as `liboctos.so` in the native lib dir, with a staged copy under
    /// HOME as the fallback.
    #[cfg(target_env = "ohos")]
    fn find_embedded_kernel(
        lib_dir: &std::path::Path,
        home: &std::path::Path,
    ) -> Option<std::path::PathBuf> {
        [lib_dir.join("liboctos.so"), home.join(".bin/liboctos.so")]
            .into_iter()
            .find(|p| p.exists())
    }

    /// Whether the bundled kernel is present AND actually executable here.
    ///
    /// Unlike Android — where exec from `nativeLibraryDir` is a documented,
    /// relied-upon capability — it is not established that an OpenHarmony HAP
    /// may exec out of its bundle libs dir. Claiming an embedded kernel we
    /// cannot launch would be worse than not having one: `stdio.is_some()` also
    /// selects the `_main` profile id, so a kernel that fails to spawn would
    /// leave the app talking to a remote server under a profile that server has
    /// never heard of. So probe with a real `--version` exec and believe the
    /// result rather than the file's mode bits.
    #[cfg(target_env = "ohos")]
    fn ohos_kernel_is_executable(program: &std::path::Path) -> bool {
        match std::process::Command::new(program).arg("--version").output() {
            Ok(out) if out.status.success() => true,
            Ok(out) => {
                log::warn!(
                    "stdio: {} exited {} on --version probe; using WebSocket transport",
                    program.display(),
                    out.status
                );
                false
            }
            Err(e) => {
                log::warn!(
                    "stdio: cannot exec {} ({e}); using WebSocket transport",
                    program.display()
                );
                false
            }
        }
    }

    #[cfg(target_env = "ohos")]
    fn has_embedded_kernel() -> bool {
        let home = Self::ohos_home();
        Self::native_lib_dir()
            .and_then(|lib_dir| Self::find_embedded_kernel(&lib_dir, &home))
            .is_some_and(|p| Self::ohos_kernel_is_executable(&p))
    }

    #[cfg(target_env = "ohos")]
    fn stdio_spawn() -> Option<StdioSpawn> {
        let lib_dir = Self::native_lib_dir()?;
        let home = Self::ohos_home();
        let program = Self::find_embedded_kernel(&lib_dir, &home).or_else(|| {
            log::warn!(
                "stdio: bundled octos not found under {}; using WebSocket transport",
                lib_dir.display()
            );
            None
        })?;
        if !Self::ohos_kernel_is_executable(&program) {
            return None;
        }
        // Create HOME before spawning: `Command::spawn` chdir's into `cwd`
        // before exec, so a missing dir fails the spawn with ENOENT even though
        // the binary is fine (same trap as the Android path).
        if let Err(e) = std::fs::create_dir_all(&home) {
            log::warn!("stdio: could not create HOME {}: {e}", home.display());
        }
        log::info!("stdio: octos={} HOME={}", program.display(), home.display());
        let a2app = home.join("a2app").to_string_lossy().into_owned();
        let env = vec![
            ("HOME".to_owned(), home.to_string_lossy().into_owned()),
            ("OCTOS_SKILLS_PATH".to_owned(), a2app),
            ("RUST_LOG".to_owned(), "info".to_owned()),
        ];
        Some(StdioSpawn {
            program,
            args: vec!["serve".to_owned(), "--stdio".to_owned()],
            env,
            cwd: Some(home),
        })
    }

    #[cfg(not(mobile))]
    fn stdio_spawn() -> Option<StdioSpawn> {
        // Desktop dev keeps the WebSocket transport (talk to `octos serve`).
        None
    }

    /// Locate the app's nativeLibraryDir by scanning `/proc/self/maps` for our
    /// own already-mapped `libmakepad.so` — avoids a JNI round-trip to
    /// `ApplicationInfo.nativeLibraryDir` (the path carries a per-install hash,
    /// so it can't be hard-coded).

    /// Does a routed/composed app id have a spec on disk yet? Checks the same
    /// two locations `card_lint::load_rules` reads. Used to reject hallucinated
    /// app ids before spawning a peer for them.
    #[cfg(target_os = "android")]
    fn app_spec_exists(app_id: &str) -> bool {
        // An L0 spec is COMPILED IN, so looking for it on disk asks the wrong
        // question. `L0_APPS` holds each app's requirements and exemplar via
        // `include_str!`; they cannot be missing at runtime.
        //
        // Checking only the device tree meant the guard rejected apps that were
        // certainly present: routing "where should I go" produced `AMA named
        // unknown app 'activity' (no apps/activity/app.md)` and fell back to
        // weather — which then received a request that is not about weather,
        // invented `pick`/`picked_lat`/`picked_lon`, and had the card refused.
        // The refusal was the checker doing its job; the defect was here.
        if L0_APPS.iter().any(|(d, _, _)| *d == app_id) {
            return true;
        }
        let Ok(home) = std::env::var("HOME") else {
            return false;
        };
        let base = std::path::Path::new(&home).join("octos-home");
        [
            base.join(".octos/profiles/_main/data/memory/app-cards/apps")
                .join(app_id)
                .join("app.md"),
            base.join("a2app/apps").join(app_id).join("app.md"),
        ]
        .iter()
        .any(|p| p.exists())
    }
    #[cfg(not(target_os = "android"))]
    fn app_spec_exists(_app_id: &str) -> bool {
        // Desktop has no on-device tree; don't block composition there.
        true
    }

    /// The AMA composer session's workspace: the app-cards **`apps/`** subdir,
    /// not the tree root. The kernel fences file writes to the session
    /// workspace, so pointing it one level down means the composer can only
    /// create/modify files under `apps/<id>/` — it CANNOT touch `framework.md`,
    /// `widgets/`, or `MEMORY.md` (poisoning those would corrupt EVERY app's
    /// injected context, not one app's). Blast-radius reduction, not full
    /// isolation: overwriting a sibling `apps/weather/app.md` still needs
    /// kernel-side create-only enforcement (tracked). Android-only; on desktop
    /// the tree lives server-side.
    fn app_cards_memory_dir() -> Option<String> {
        #[cfg(target_os = "android")]
        {
            let p = "/data/user/0/dev.makepad.octos_app/files/octos-home/.octos/profiles/_main/data/memory/app-cards/apps";
            // The dir must EXIST for the kernel's cwd validation to accept the
            // hint (validate_session_workspace_allowed canonicalizes it).
            let _ = std::fs::create_dir_all(p);
            Some(p.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            None
        }
    }

    fn current_workspace_cwd() -> Option<String> {
        // Android: leave the per-session workspace default. a2app memory is made
        // reachable via OCTOS_SKILLS_PATH (a skill read-zone) in stdio_spawn(),
        // which is honored regardless of the workspace (the `session.workspace_cwd`
        // path was not applied by the embedded serve).
        #[cfg(target_os = "android")]
        {
            None
        }
        #[cfg(not(target_os = "android"))]
        {
            std::env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
                .filter(|p| p != "/")
        }
    }

    /// Resolve the bearer token for `(host, profile_id)`. `OCTOS_APP_TOKEN`
    /// wins (`keychain::load_token` already honours it as a bypass), the
    /// keychain entry is consulted next, and an empty `SecretString` is
    /// returned otherwise so the caller still has a syntactically-valid
    /// `TransportConfig` (the REST round-trip 401s, which we surface
    /// silently in M1).
    fn resolve_bearer(base_url: &url::Url, profile_id_str: &str) -> SecretString {
        let host = octos_app_store::auth::ServerHost::from(
            crate::app::login::host_from_url(base_url),
        );
        let pid = ProfileId::from(profile_id_str.to_owned());
        match octos_app_store::keychain::load_token(&host, &pid) {
            Ok(Some(tok)) => SecretString::new(tok.expose().to_owned()),
            Ok(None) => SecretString::new(String::new()),
            Err(e) => {
                log::warn!("keychain load_token failed ({e}); using empty bearer");
                SecretString::new(String::new())
            }
        }
    }

    fn clear_chat(&mut self, cx: &mut Cx) {
        // A previous web app card floats as a native overlay — hide it with the
        // chat it belonged to.
        cx.system_browser(web_card_browser_id()).detach();
        {
            let mut data = CHAT_DATA.write().unwrap();
            data.messages.clear();
            data.streaming_text.clear();
            data.authoritative_text.clear();
            data.thinking_text.clear();
            data.is_streaming = false;
            data.a2app_state.clear();
            data.save_to_disk();
        }
        // New session — the Splash manual must be re-primed into it.
        self.splash_primed = false;
        // Composer stays FOLDED (only the "+" FAB) — it never auto-expands, so
        // it never covers a card. The user taps "+" to unfold when they want to
        // type. (Was: expand into "compose state".)
        self.composer_shown = false;
        self.sync_composer(cx);

        if let Some(agent) = &mut self.agent {
            let app_cfg = || SessionConfig {
                system_prompt: Some(OCTOS_PLACEHOLDER_SYSTEM_PROMPT.to_string()),
                ..Default::default()
            };
            // ONE app agent PER DOMAIN, all live concurrently; each is its own
            // octos session so its context stays dedicated to its domain. The
            // AMA's routing decision activates the matching one (decision →
            // activation). `foreground` = whichever last took the screen.
            let weather = agent.create_session(cx, app_cfg());
            let stock = agent.create_session(cx, app_cfg());
            let news = agent.create_session(cx, app_cfg());
            let web = agent.create_session(cx, app_cfg());
            let youtube = agent.create_session(cx, app_cfg());
            let nav = agent.create_session(cx, app_cfg());
            self.apps = vec![
                AppRecord::with_domain(weather, "Weather", "weather"),
                AppRecord::with_domain(stock, "Stock", "stock"),
                AppRecord::with_domain(news, "News", "news"),
                AppRecord::with_domain(web, "Web", "web"),
                AppRecord::with_domain(youtube, "YouTube", "youtube"),
                AppRecord::with_domain(nav, "Nav", "nav"),
            ];
            self.foreground = 0;
            self.pending_intent = None;
            // The AMA (routing brain) is its OWN concurrent session. Its
            // workspace is cwd-hinted INTO the app-cards memory tree
            // (`session.workspace_cwd.v1`, default-on for stdio) so the
            // composer path can author `apps/<id>/app.md` + `lint.json` with
            // plain relative write_file calls — new app specs land where every
            // NEWLY OPENED app-agent session injects them from. Keep
            // `appui.sessions_in_cwd: false` in the kernel config
            // (ensure_kernel_config_knobs) or transcripts relocate into the
            // card tree.
            let ama_config = SessionConfig {
                cwd: Self::app_cards_memory_dir(),
                system_prompt: Some(AMA_SYSTEM_PROMPT.to_string()),
                ..Default::default()
            };
            self.ama_session = Some(agent.create_session(cx, ama_config));
            log::info!("AMA + 6 domain app agents (weather/stock/news/web/youtube/nav) created concurrently");

            // DEV-GOAL HARNESS: only when the launch intent asks for it.
            // `--es makepad.DEV_GOAL_FILE <path>` names a host-authored mission
            // file (readable: /data/local/tmp, 0644). The card comes back via
            // /data/local/tmp/dev_card.splash (pre-created 0666 by the host —
            // this app cannot CREATE there, only write into what exists), and
            // findings arrive via /data/local/tmp/dev_findings.txt.
            if let Ok(goal_path) = std::env::var("MAKEPAD_DEV_GOAL_FILE") {
                match std::fs::read_to_string(&goal_path) {
                    Ok(goal) => {
                        let dev_cfg = SessionConfig {
                            system_prompt: Some(DEV_MASTER_PROMPT.to_string()),
                            ..Default::default()
                        };
                        let dev = agent.create_session(cx, dev_cfg);
                        let pid = agent.send_prompt(cx, dev, &goal);
                        self.dev_session = Some(dev);
                        self.dev_prompt = Some(pid);
                        self.dev_round = 1;
                        log::info!("[devgoal] round 1 started ({} goal bytes)", goal.len());
                        // Findings ferry: mtime-watch the findings file; on
                        // change push its text and wake the UI thread.
                        std::thread::spawn(|| {
                            let path = "/data/local/tmp/dev_findings.txt";
                            let mut last: Option<std::time::SystemTime> = None;
                            loop {
                                std::thread::sleep(std::time::Duration::from_secs(3));
                                let Ok(meta) = std::fs::metadata(path) else { continue };
                                let Ok(mt) = meta.modified() else { continue };
                                if last == Some(mt) {
                                    continue;
                                }
                                last = Some(mt);
                                if let Ok(text) = std::fs::read_to_string(path) {
                                    if text.trim().is_empty() {
                                        continue;
                                    }
                                    if let Ok(mut q) = DEV_FINDINGS.lock() {
                                        q.push(text);
                                    }
                                    makepad_widgets::SignalToUI::set_ui_signal();
                                }
                            }
                        });
                    }
                    Err(e) => log::warn!("[devgoal] goal file unreadable: {e}"),
                }
            }
        }
        // §5.12: hand the durable store to the VM before anything renders, or
        // the first card drawn after a launch shows an empty list and fills in
        // only once something else happens to write.
        app::l0_card::publish_collections();
        self.update_empty_state_visibility(cx);
        self.sync_app_tabs(cx);
        self.ui.redraw(cx);
    }

    /// Wipe the shared conversation surface (`CHAT_DATA`). Shared by
    /// `clear_chat`, `open_new_app`, and `switch_to_app`.
    fn wipe_chat_surface(&mut self) {
        let mut data = CHAT_DATA.write().unwrap();
        data.messages.clear();
        data.streaming_text.clear();
            data.authoritative_text.clear();
        data.thinking_text.clear();
        data.is_streaming = false;
        data.a2app_state.clear();
        data.save_to_disk();
        CHAT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Snapshot the shared `CHAT_DATA` into app `i`'s record — call before
    /// leaving app `i` in the foreground so switching back restores it.
    fn snapshot_into(&mut self, i: usize) {
        if let Some(a) = self.apps.get_mut(i) {
            let data = CHAT_DATA.read().unwrap();
            a.saved_messages = data.messages.clone();
            a.saved_a2app = data.a2app_state.clone();
            log::info!(
                "snapshot_into app {i}: {} msgs, {} card-states",
                a.saved_messages.len(),
                a.saved_a2app.len()
            );
        }
    }

    /// Restore app `i`'s snapshot into the shared `CHAT_DATA` — call after
    /// making app `i` the foreground.
    fn restore_from(&self, i: usize) {
        if let Some(a) = self.apps.get(i) {
            let mut data = CHAT_DATA.write().unwrap();
            data.messages = a.saved_messages.clone();
            data.a2app_state = a.saved_a2app.clone();
            data.streaming_text.clear();
            data.authoritative_text.clear();
            data.thinking_text.clear();
            data.is_streaming = false;
            data.save_to_disk();
            log::info!(
                "restore_from app {i}: {} msgs, {} card-states",
                data.messages.len(),
                data.a2app_state.len()
            );
        }
        // Force ChatList to re-parse the restored card (drop its render cache).
        CHAT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Layer 3.2 — open ANOTHER app: a fresh octos session that becomes the
    /// foreground while the existing apps stay open in the background. Unlike
    /// `clear_chat` (which resets to a single app), this PUSHES a record.
    fn open_new_app(&mut self, cx: &mut Cx) {
        if self.agent.is_none() {
            return;
        }
        // Snapshot the app we're leaving so switching back restores its card.
        if !self.apps.is_empty() {
            let prev = self.foreground;
            self.snapshot_into(prev);
        }
        let config = SessionConfig {
            system_prompt: Some(OCTOS_PLACEHOLDER_SYSTEM_PROMPT.to_string()),
            ..Default::default()
        };
        let sid = self.agent.as_mut().unwrap().create_session(cx, config);
        let n = self.apps.len() + 1;
        self.apps.push(AppRecord::new(sid, format!("App {n}")));
        self.foreground = self.apps.len() - 1;
        // Fresh foreground app → clear the shared surface; re-prime the manual.
        self.wipe_chat_surface();
        self.splash_primed = false;
        // Stay folded to the "+" FAB; the user taps "+" to unfold and type the
        // new app's first request. (Was: auto-expand.)
        self.composer_shown = false;
        self.sync_composer(cx);
        self.update_empty_state_visibility(cx);
        self.sync_app_tabs(cx);
        self.collapse_sidebar_if_narrow(cx);
        self.ui.redraw(cx);
    }

    /// Layer 3.3 — bring already-open app `i` to the foreground (Path A-lite
    /// snapshot/restore). Snapshots the outgoing app's `CHAT_DATA`, then
    /// restores app `i`'s saved conversation. Instant and fully offline — no
    /// server round-trip. (`resume_session`/hydrate remains available for the
    /// online/multi-device case; the sidebar session list still uses it.)
    fn switch_to_app(&mut self, cx: &mut Cx, i: usize) {
        if i >= self.apps.len() {
            return;
        }
        if i == self.foreground {
            // Re-tapping the current tab just clears its unread badge.
            if let Some(a) = self.fg_mut() {
                a.has_updates = false;
            }
            self.sync_app_tabs(cx);
            return;
        }
        // Snapshot the app we're leaving, then enter and restore app `i`.
        let prev = self.foreground;
        self.snapshot_into(prev);
        self.foreground = i;
        if let Some(a) = self.apps.get_mut(i) {
            a.has_updates = false;
        }
        self.restore_from(i);
        self.splash_primed = false;
        let count = { CHAT_DATA.read().unwrap().messages.len() };
        // Composer stays folded to the "+" FAB regardless of content; the user
        // taps "+" to unfold. (Was: expand when the app is empty — `count == 0`.)
        self.composer_shown = false;
        self.sync_composer(cx);
        self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
        self.update_empty_state_visibility(cx);
        self.sync_app_tabs(cx);
        self.collapse_sidebar_if_narrow(cx);
        self.update_status(cx);
        if count > 0 {
            let list = self
                .ui
                .widget(cx, ids!(chat_list))
                .portal_list(cx, ids!(list));
            list.set_tail_range(true);
            list.set_first_id_and_scroll(count.saturating_sub(1), 0.0);
            // Repaint burst so the restored card re-shapes and its background
            // image decodes — the same trigger `TurnComplete` fires when a
            // fresh card lands (a single redraw_all leaves the card blank).
            self.settle_ticks = 0;
            self.settle_timer = cx.start_interval(0.35);
        }
        cx.redraw_all();
    }

    /// Layer 3.3 — reflect the `apps`/`foreground` state onto the fixed set of
    /// tab-chip slots. The strip is hidden until a second app opens (keeps the
    /// single-app full-screen look). Foreground chip is marked `▸`; a
    /// background app with unseen output gets a `•` badge.
    /// The visible switcher moved into the native composer pill (＋/⟳), so
    /// there's no top strip to sync. Kept as a no-op hook: `open_new_app` /
    /// `switch_to_app` / `clear_chat` still call it, and it logs app state for
    /// on-device diagnosis. `_cx` unused now that no widget is updated.
    fn sync_app_tabs(&mut self, _cx: &mut Cx) {
        log::info!(
            "apps={} fg={}",
            self.apps.len(),
            self.foreground
        );
    }

    /// Offer a system gesture to the latest L0 card and redraw it if a cell
    /// moved. Returns whether the gesture was consumed. The redraw is the same
    /// rewrite the notify path does: replace lowered DSL, bump the generation.
    fn l0_gesture(&mut self, cx: &mut Cx, event: &str) -> bool {
        let Some((item, body)) = app::l0_card::gesture(cx, event) else {
            return false;
        };
        if let Ok(mut chat) = CHAT_DATA.write() {
            if let Some(msg) = chat.messages.get_mut(item) {
                if !msg.text.contains("```runl0") {
                    msg.text = format!("```runsplash\n{body}\n```");
                }
            }
        }
        CHAT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        cx.redraw_all();
        log::info!("[l0] {event} gesture -> redrew item {item}");
        true
    }

    fn update_empty_state_visibility(&self, cx: &mut Cx) {
        let (show_empty_state, is_streaming) = {
            let data = CHAT_DATA.read().unwrap();
            (
                data.messages.is_empty() && !data.is_streaming,
                data.is_streaming,
            )
        };
        self.ui
            .view(cx, ids!(empty_state))
            .set_visible(cx, show_empty_state);
        // Metaballs over the whole screen = "the model is working on it". Full
        // screen so the card being replaced can't be mistaken for the answer.
        self.ui
            .view(cx, ids!(thinking_curtain))
            .set_visible(cx, is_streaming);
    }

    fn send_message(&mut self, cx: &mut Cx) {
        let input = self.ui.text_input(cx, ids!(input));
        let text = input.text();
        if text.trim().is_empty() {
            return;
        }
        // Clear the Makepad composer's own input. On Android the visible
        // composer is the native floating overlay (which clears itself); this
        // clears the hidden Makepad TextInput on desktop.
        input.set_text(cx, "");
        self.submit_prompt(cx, text);
    }

    /// Send `text` through the octos agent, reusing the splash-mode wrapping,
    /// saved-card injection, streaming state and list scroll. Both the Makepad
    /// composer (`send_message`) and the native Android floating composer
    /// (`NativeComposerSubmit`, routed from `handle_actions`) land here.
    /// TEST/automation hook: `--es makepad.AUTO_PROMPT "<text>"` (env
    /// MAKEPAD_AUTO_PROMPT) auto-submits ONE prompt once a session is open, so a
    /// live LLM generation can be driven without touching the native composer via
    /// adb input. Fires once (clears the env var). Session/open and turn/start
    /// are ordered on the stdio pipe, so submitting right after `clear_chat` is
    /// safe.
    fn fire_auto_prompt(&mut self, cx: &mut Cx) {
        if let Ok(p) = std::env::var("MAKEPAD_AUTO_PROMPT") {
            std::env::remove_var("MAKEPAD_AUTO_PROMPT");
            let p = p.trim().to_string();
            if !p.is_empty() {
                log::info!("AUTO_PROMPT: submitting {p:?}");
                self.submit_prompt(cx, p);
            }
        }
    }

    fn submit_prompt(&mut self, cx: &mut Cx, text: String) {
        // The youtube live-id cache used to be warmed here, on EVERY submit, so a
        // routed youtube intent could inject ground-truth ids into its generation
        // prompt. That prompt is gone (the L0 card searches instead), and the only
        // remaining reader is the `OCTOS_SEED_CARD` reference card — a build-time
        // diagnostic — so four channel fetches per prompt fed nothing. Warmed
        // where that card is actually built now.
        if text.trim().is_empty() {
            return;
        }

        if self.agent.is_none() || self.fg_session().is_none() {
            return;
        }

        // Reject a new submit while ANY turn is in flight — the AMA routing
        // turn (singleton `ama_prompt`/`ama_text`/`pending_intent`) OR the
        // routed app's generation turn. Both share the singleton streaming
        // surface; a second submit mid-turn overwrites it and the first turn's
        // late deltas leak in as foreground text. `is_streaming` is set for the
        // whole window (submit → TurnComplete), so it covers both phases. The
        // user can Cancel to abort and recover (also unwedges a transport-drop
        // where no terminal event arrives).
        if self.ama_prompt.is_some() || CHAT_DATA.read().unwrap().is_streaming {
            log::info!("submit ignored: a turn is still in flight (Cancel to abort)");
            return;
        }

        let items_len = {
            let mut data = CHAT_DATA.write().unwrap();
            data.messages.push(ChatMessage {
                role: ChatRole::User,
                text: text.clone(),
            });
            data.streaming_text.clear();
            data.authoritative_text.clear();
            data.thinking_text.clear();
            data.saved_stream_cards.clear();
            data.is_streaming = true;
            data.messages.len() + 1
        };
        self.update_empty_state_visibility(cx);

        // Collapse the composer to its "+" button while the card generates — the
        // answer renders full-screen behind it. On Android the native
        // submitComposer() already collapsed for instant feedback; this keeps
        // `composer_shown` in sync (and drives the desktop composer/pill).
        self.composer_shown = false;
        self.sync_composer(cx);

        let session_id = self.fg_session().unwrap();
        let agent = self.agent.as_mut().unwrap();

        // Octos sessions are stateful server-side, so we don't inject
        // history client-side (aichat's stateless replay is gone — see
        // `05-AICHAT-REUSE-MAP.md` "Stuff we drop or replace").
        //
        // Splash mode: the bubble shows the user's original `text`, but the
        // LLM receives the Splash UI-generation prompt + manual so it returns
        // a `runsplash` block the Markdown widget renders live.
        // Splash generation is server-side: a tiny router (in the MESSAGE — the
        // profile system prompt buries it) tells the octos main agent to spawn the
        // `splash-gen` sub-agent, which loads the per-appid memory under
        // `octos-home/a2app/` in its own clean context and returns the `runsplash`
        // block. No client-side manual/template/saved-card injection — that all
        // lives in the a2app memory the sub-agent reads.
        // AMA-first routing (splash mode): send the intent to the AMA to classify,
        // and HOLD it. On the AMA's decision (`AgentEvent::TurnComplete` for
        // `ama_prompt`) we activate the routed domain agent and dispatch the
        // generation prompt to it — that's "decision → activation". Plain chat (or
        // a missing AMA) still goes straight to the foreground agent.
        let ama_session = self.ama_session;
        let splash = self.splash_mode;
        let (ama_pid, direct_pid) = if splash {
            if let Some(ama) = ama_session {
                let ama_msg = format!(
                    "{AMA_SYSTEM_PROMPT}\n\nUser message: {text}\n\nYour one-line routing decision:"
                );
                (Some(agent.send_prompt(cx, ama, &ama_msg)), None)
            } else {
                let sent = format!("{APP_SPLASH_ROUTER}\n\nUser request: {text}");
                (None, Some(agent.send_prompt(cx, session_id, &sent)))
            }
        } else {
            (None, Some(agent.send_prompt(cx, session_id, &text)))
        };
        // `agent` borrow ends above; now touch `self` fields.
        if let Some(ama_pid) = ama_pid {
            self.ama_prompt = Some(ama_pid);
            self.ama_text.clear();
            self.pending_intent = Some(text.clone());
        } else if let Some(pid) = direct_pid {
            self.set_fg_prompt(Some(pid));
        }
        self.sync_app_tabs(cx);
        self.ui.view(cx, ids!(cancel_button)).set_visible(cx, true);

        let chat_list = self.ui.widget(cx, ids!(chat_list));
        let list = chat_list.portal_list(cx, ids!(list));
        list.set_tail_range(true);
        list.set_first_id_and_scroll(items_len.saturating_sub(1), 0.0);
        self.ui.redraw(cx);
    }

    fn cancel_request(&mut self, cx: &mut Cx) {
        // Cancel an in-flight AMA ROUTING turn too: during routing the
        // foreground prompt is still `None`, so cancelling only the fg prompt
        // did nothing and generation proceeded. Abort the route and release the
        // held intent so the singleton state is clean for the next submit.
        if let Some(ama_pid) = self.ama_prompt.take() {
            if let Some(agent) = &mut self.agent {
                agent.cancel_prompt(cx, ama_pid);
            }
            // Remember it: the interrupt is async, so a delta/TurnComplete
            // already in flight for this pid must be DROPPED (not streamed as
            // foreground text) — see the TextDelta/TurnComplete handlers.
            self.cancelled_ama = Some(ama_pid);
            self.ama_text.clear();
            self.pending_intent = None;
            let mut data = CHAT_DATA.write().unwrap();
            data.streaming_text.clear();
            data.authoritative_text.clear();
            data.thinking_text.clear();
            data.is_streaming = false;
            drop(data);
            self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
            self.update_empty_state_visibility(cx);
            self.ui.redraw(cx);
            return;
        }
        let taken = self.fg_prompt_take();
        if let (Some(agent), Some(prompt_id)) = (&mut self.agent, taken) {
            agent.cancel_prompt(cx, prompt_id);

            let mut data = CHAT_DATA.write().unwrap();
            let text = std::mem::take(&mut data.streaming_text);
            data.thinking_text.clear();
            if !text.is_empty() {
                data.messages.push(ChatMessage {
                    role: ChatRole::Assistant,
                    text,
                });
            }
            data.is_streaming = false;
            drop(data);

            self.update_empty_state_visibility(cx);
            self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
            self.ui.redraw(cx);
        }
    }

    /// Reflect `composer_shown` into the floating composer + reveal pill: when
    /// expanded the glass composer shows and the pill hides; when collapsed
    /// (after a card renders) only the slim pill shows, giving the card the
    /// full screen. A full redraw is required after flipping glass-composite
    /// visibility or the old composite lingers (see [[octos-app-android]]).
    fn sync_composer(&mut self, cx: &mut Cx) {
        // OHOS uses the native overlay for the same reasons Android does, and
        // additionally because makepad has no text-input bridge there at all —
        // a makepad TextInput on OHOS can never receive a character.
        #[cfg(mobile)]
        {
            // The native floating composer overlay replaces the Makepad docked
            // composer + reveal pill on Android; keep both Makepad widgets hidden.
            // The overlay stays present (it floats over every card); its SUB-state
            // — full input pill vs collapsed "+" button — tracks `composer_shown`,
            // so it shrinks to "+" while a card generates / after it renders and
            // expands when the user taps "+".
            self.ui.widget(cx, ids!(composer)).set_visible(cx, false);
            self.ui.button(cx, ids!(reveal_pill)).set_visible(cx, false);
            cx.show_native_composer();
            if self.composer_shown {
                cx.expand_native_composer();
            } else {
                cx.collapse_native_composer();
            }
            cx.redraw_all();
        }
        #[cfg(not(mobile))]
        {
            let show = self.composer_shown;
            self.ui.widget(cx, ids!(composer)).set_visible(cx, show);
            self.ui.button(cx, ids!(reveal_pill)).set_visible(cx, !show);
            cx.redraw_all();
        }
    }

    /// Status label content. W01 will rewrite this to show `Connected ·
    /// {latency}ms · cursor {seq}` per `04-IA-AND-NAVIGATION.md` §
    /// "Top bar contents"; for now it reflects whether a profile has been
    /// selected.
    fn update_status(&self, cx: &mut Cx) {
        let status = match self.current_profile.as_ref() {
            Some(profile) => format!("Connected · profile={}", profile),
            None => "Initializing...".to_string(),
        };
        self.ui.label(cx, ids!(status_label)).set_text(cx, &status);
    }

    /// W04 follow-up #3 — render `APP_STATE.connection` as the top-bar
    /// status dot + label. Green = Connected, amber = Reconnecting, red =
    /// Offline. Pure read of `AppState` mirrored by `OctosUiAgent` on
    /// `TransportEvent::ConnectionState`.
    fn update_connection_indicator(&self, cx: &mut Cx) {
        use octos_app_store::state::ConnectionState as StoreCs;
        let cs = APP_STATE
            .read()
            .map(|s| s.connection)
            .unwrap_or(StoreCs::Offline);
        let (label, color) = match cs {
            StoreCs::Connected => ("Live", "#x4FCB6E"),
            StoreCs::Reconnecting => ("Reconnecting", "#xF6BE63"),
            StoreCs::Offline => ("Offline", "#xE36363"),
        };
        let _ = color; // referenced in the script_apply_eval below
        self.ui
            .label(cx, ids!(connection_state_label))
            .set_text(cx, label);
        let mut dot = self.ui.label(cx, ids!(connection_dot));
        match cs {
            StoreCs::Connected => script_apply_eval!(cx, dot, {
                draw_text +: { color: #x4FCB6E }
            }),
            StoreCs::Reconnecting => script_apply_eval!(cx, dot, {
                draw_text +: { color: #xF6BE63 }
            }),
            StoreCs::Offline => script_apply_eval!(cx, dot, {
                draw_text +: { color: #xE36363 }
            }),
        }
    }

    /// Re-render every assistant message's markdown with the current A2App
    /// counter substituted into `{{state.count}}`. Mirrors aichat's
    /// `refresh_visible_state_templates`: set_text directly on each pooled
    /// PortalList item's markdown (a plain redraw does NOT re-run the item's
    /// draw), so a live counter updates in place.
    fn refresh_a2app_templates(&self, cx: &mut Cx) {
        let messages: Vec<(usize, String, CardState)> = {
            let data = CHAT_DATA.read().unwrap();
            data.messages
                .iter()
                .enumerate()
                .filter_map(|(i, m)| match m.role {
                    ChatRole::Assistant => Some((
                        i,
                        m.text.clone(),
                        data.a2app_state.get(&i).cloned().unwrap_or_default(),
                    )),
                    _ => None,
                })
                .collect()
        };
        let chat_list = self.ui.widget(cx, ids!(chat_list));
        let list = chat_list.portal_list(cx, ids!(list));
        for (item_id, text, state) in messages {
            if let Some((_, item)) = list.get_item(item_id) {
                // Re-feed the whole markdown (keeps non-splash content current).
                let unwrapped = unwrap_outer_markdown_fence(&text);
                let materialized = materialize_runplan_for_display(unwrapped);
                let rendered = wrap_bare_latex(&materialized);
                let rendered = resolve_a2app_card(cx, &rendered, item_id, &state);
                item.markdown(cx, ids!(selectable)).set_text(cx, &rendered);
                // Also push the resolved `runsplash` body straight to the
                // Splash widget — its `set_text` re-evals on change, and this
                // guarantees the update even if the markdown re-parse doesn't
                // re-dispatch to the pooled splash_view.
                if let Some(body) = card_splash_body(&text) {
                    let resolved = substitute_card_state(&body, item_id, &state);
                    item.widget(cx, ids!(splash_view)).set_text(cx, &resolved);
                }
            }
        }
        cx.redraw_all();
    }

    /// Drive the toast strip from `APP_STATE.toasts`. Shows the front
    /// (oldest) queued toast for a few seconds, then the timer dismisses it
    /// and advances to the next. No-op while a toast is already on screen
    /// (`toast_timer` non-empty).
    fn sync_toasts(&mut self, cx: &mut Cx) {
        if !self.toast_timer.is_empty() {
            return;
        }
        let front = APP_STATE
            .read()
            .ok()
            .and_then(|s| s.toasts.iter().next().cloned());
        match front {
            Some(t) => {
                self.ui.label(cx, ids!(toast_label)).set_text(cx, &t.message);
                self.ui.view(cx, ids!(toast_row)).set_visible(cx, true);
                self.toast_timer = cx.start_timeout(3.8);
                cx.redraw_all();
            }
            None => {
                self.ui.view(cx, ids!(toast_row)).set_visible(cx, false);
            }
        }
    }

    /// Top-bar context-usage chip. Reads `APP_STATE.context` (updated every
    /// turn from `context/normalization`) and shows the model context-window
    /// fill — e.g. `◔ 10k · 68 msgs`. Blank until the first turn reports.
    fn update_context_indicator(&self, cx: &mut Cx) {
        let ctx = APP_STATE.read().ok().and_then(|s| s.context.clone());
        let text = match ctx {
            Some(c) => {
                let tok = c.token_estimate;
                let tok_str = if tok >= 1000 {
                    format!("{:.1}k", tok as f64 / 1000.0)
                } else {
                    format!("{tok}")
                };
                format!("\u{25D4} {tok_str} \u{00B7} {} msgs", c.item_count)
            }
            None => String::new(),
        };
        self.ui.label(cx, ids!(context_chip)).set_text(cx, &text);
    }

    fn apply_glass_opacity(&self, cx: &mut Cx, opacity: f64) {
        let opacity = opacity.clamp(MIN_GLASS_OPACITY, MAX_GLASS_OPACITY);
        let glass = glass_opacity_values(opacity);

        let mut app_shell = self.ui.view(cx, ids!(app_shell));
        script_apply_eval!(cx, app_shell, {
            draw_bg +: { tint_alpha: #(glass.app) }
        });

        let mut sidebar = self.ui.view(cx, ids!(sidebar));
        script_apply_eval!(cx, sidebar, {
            draw_bg +: { tint_alpha: #(glass.sidebar) }
        });

        let mut main_area = self.ui.view(cx, ids!(main_area));
        script_apply_eval!(cx, main_area, {
            draw_bg +: { tint_alpha: #(glass.main) }
        });

        let mut composer = self.ui.view(cx, ids!(composer));
        script_apply_eval!(cx, composer, {
            draw_bg +: { tint_alpha: #(glass.composer) }
        });

        self.ui
            .label(cx, ids!(opacity_value))
            .set_text(cx, &format!("{:.0}%", opacity * 100.0));
        self.ui.redraw(cx);
    }

    // ---- W04 / M2 — Content + Viewers helpers --------------------------

    /// Flip the active screen sibling based on `APP_STATE.navigation`.
    /// Mirrors `show_login`'s lockstep `set_visible` pattern; W06 added
    /// `coding_screen` and W07 added `studio/slides/sites_screen`.
    fn show_screen_for_nav(&self, cx: &mut Cx) {
        let nav = APP_STATE
            .read()
            .map(|s| s.navigation.clone())
            .unwrap_or_default();
        let is_content = matches!(nav, CurrentScreen::Content);
        // Chat is the implicit default — show it for any other navigation
        // state (incl. the removed Coding / Studio / Slides / Sites states,
        // should the store ever carry them).
        let is_chat = !is_content;
        self.ui
            .view(cx, ids!(chat_screen))
            .set_visible(cx, is_chat);
        self.ui
            .view(cx, ids!(content_screen))
            .set_visible(cx, is_content);
        // The native floating composer belongs to the chat screen — hide it
        // while the content browser is up so it doesn't float over it.
        #[cfg(mobile)]
        {
            if is_chat {
                cx.show_native_composer();
            } else {
                cx.hide_native_composer();
            }
        }
        self.ui.redraw(cx);
    }

    /// Sidebar `nav_content` click — flip to Content + fire REST hydrate.
    fn navigate_to_content(&mut self, cx: &mut Cx) {
        {
            let mut state = APP_STATE.write().unwrap();
            octos_app_store::state::reduce(
                &mut state,
                octos_app_store::state::Event::Navigation(
                    NavigationEvent::NavigateTo(CurrentScreen::Content),
                ),
            );
        }
        self.show_screen_for_nav(cx);
        self.fire_content_hydrate();
    }

    // navigate_to_coding / navigate_to_producer removed with the Coding /
    // Studio / Slides / Sites navs (unsupported in this build).

    /// The side panel is removed from the product (it covered the AMA
    /// surface; see the sidebar declaration) — this keeps it and the desktop
    /// glass toolbar hidden at every width, from every path that used to
    /// collapse them conditionally.
    fn collapse_sidebar_if_narrow(&self, cx: &mut Cx) {
        self.ui.view(cx, ids!(sidebar)).set_visible(cx, false);
        self.ui.view(cx, ids!(glass_toolbar)).set_visible(cx, false);
        cx.redraw_all();
    }

    /// Spawn an off-thread `task/output/read` and post the reply back as
    /// `TaskOutputAction`. Same lifecycle shape as `hydrate_sessions`
    /// (`app/src/app/sessions.rs:hydrate_sessions`) — short-lived
    /// `current_thread` runtime so the call site doesn't need to already
    /// be inside one.
    ///
    /// We can't reach the WS transport from this thread (it's owned by
    /// the agent's own runtime); instead we hop through the REST
    /// fallback path the agent uses for one-shot reads. For M3 we keep
    /// it simple and synthesize the call via the WS handle if available.
    fn fire_task_output_read(&self, task_id: octos_core::TaskId) {
        // Resolve the session id from APP_STATE — without it, the wire
        // params are invalid. Bail silently if no session is open.
        let Some(session_id) = APP_STATE
            .read()
            .ok()
            .and_then(|s| s.current_session.clone())
        else {
            return;
        };
        let params =
            crate::app::coding::build_output_read_params(session_id.clone(), task_id.clone());
        if let Some(handle) = self.task_output_handle.as_ref() {
            handle.read(params);
        } else {
            Cx::post_action(crate::app::coding::TaskOutputAction {
                task_id,
                session_id,
                outcome: crate::app::coding::TaskOutputOutcome::Failed(
                    "agent not initialized".to_owned(),
                ),
            });
        }
    }

    /// Spawn the off-thread REST hydrate. Reads filter / search from
    /// `CONTENT_STATE` (server-side `kind` / `q`).
    fn fire_content_hydrate(&self) {
        let cfg = Self::placeholder_transport_config();
        let client = Self::build_rest_client(&cfg);
        let (kind, q) = CONTENT_STATE
            .read()
            .ok()
            .map(|cs| {
                (
                    cs.filter.server_kind().map(|s| s.to_owned()),
                    if cs.search.trim().is_empty() {
                        None
                    } else {
                        Some(cs.search.trim().to_owned())
                    },
                )
            })
            .unwrap_or((None, None));
        content_mod::hydrate_content(client, MyContentQuery {
            kind,
            q,
            limit: None,
            cursor: None,
        });
    }

    /// Open the right viewer for `handle`. Markdown additionally fires a
    /// background `reqwest` for the body unless cached.
    fn open_viewer_for(&self, cx: &mut Cx, handle: octos_app_store::files::FileHandle) {
        let open = viewers_mod::viewer_for(&handle);
        let need_md_fetch = matches!(open, OpenViewer::Markdown { .. })
            && VIEWER_STATE
                .read()
                .map(|vs| !vs.markdown_cache.contains_key(&handle))
                .unwrap_or(true);
        if let Ok(mut vs) = VIEWER_STATE.write() {
            vs.open = open;
            vs.last_error = None;
        }
        if need_md_fetch {
            let cfg = Self::placeholder_transport_config();
            let client = Self::build_rest_client(&cfg);
            viewers_mod::fetch_markdown(client, handle);
        }
        // Full repaint — overlay visibility flip (see `show_login`).
        cx.redraw_all();
    }

    fn close_viewer(&self, cx: &mut Cx) {
        if let Ok(mut vs) = VIEWER_STATE.write() {
            vs.open = OpenViewer::Closed;
        }
        // Full repaint — overlay visibility flip (see `show_login`).
        cx.redraw_all();
    }

    /// Image album prev/next — clamps to [0, len).
    fn album_step(&self, cx: &mut Cx, delta: i32) {
        if let Ok(mut vs) = VIEWER_STATE.write() {
            if let OpenViewer::ImageAlbum { handles, active } = &mut vs.open {
                if !handles.is_empty() {
                    let len = handles.len() as i32;
                    let next = (*active as i32 + delta).clamp(0, len - 1);
                    *active = next as usize;
                }
            }
        }
        self.ui.redraw(cx);
    }

    /// Use `robius_open` to launch the OS default viewer for the handle.
    fn open_in_os(&self, handle: &octos_app_store::files::FileHandle) {
        let cfg = Self::placeholder_transport_config();
        let client = Self::build_rest_client(&cfg);
        let Some(url) = viewers_mod::url_for(&client, handle) else {
            log::warn!("open_in_os: file_url failed for {handle}");
            return;
        };
        if let Err(e) = robius_open::Uri::new(url.as_str()).open() {
            log::warn!("robius_open {handle}: {e:?}");
        }
    }

    // ---- W08 — login flow helpers ------------------------------------------

    /// Toggle between the LoginScreen overlay and the chat shell. Lockstep
    /// `set_visible` on `app_shell` and `login_overlay` so only one is
    /// interactive at a time.
    fn show_login(&self, cx: &mut Cx, show: bool) {
        self.ui.view(cx, ids!(app_shell)).set_visible(cx, !show);
        self.ui.view(cx, ids!(login_overlay)).set_visible(cx, show);
        // Full repaint, not just `ui.redraw`: the glass widgets draw into
        // self-managed overlay draw lists, and a partial redraw can leave a
        // stale composite on screen after a visibility flip (on Android this
        // showed as a black boot screen / a login card that never dismissed —
        // same failure mode aichat documents in its `clear_chat`).
        cx.redraw_all();
    }

    /// Push a status / error string to the LoginScreen status label. Empty
    /// string clears the surface (used after a successful step).
    fn login_set_status(&self, cx: &mut Cx, msg: &str) {
        self.ui
            .label(cx, ids!(login_status_label))
            .set_text(cx, msg);
    }

    /// Boot-time decision: are we already authed? Honours
    /// `OCTOS_APP_TOKEN` (dev shortcut) > server.json + keychain > go to
    /// Login. Side-effect: caches `login_server_url` / `login_profile_id`
    /// from the config file when present so the email / verify steps don't
    /// have to re-read disk.
    fn boot_is_authed(&mut self) -> bool {
        if let Ok(t) = std::env::var("OCTOS_APP_TOKEN") {
            if !t.is_empty() {
                log::info!("OCTOS_APP_TOKEN present; skipping LoginScreen");
                return true;
            }
        }
        let Some(cfg) = crate::app::login::load_server_config() else {
            log::info!("no server.json — starting at LoginScreen Step 1");
            return false;
        };
        let url = match url::Url::parse(&cfg.server_url) {
            Ok(u) => u,
            Err(e) => {
                log::warn!("server.json has invalid URL ({e}); falling back to Login");
                return false;
            }
        };
        let host = octos_app_store::auth::ServerHost::from(
            crate::app::login::host_from_url(&url),
        );
        let pid = ProfileId::from(cfg.profile_id.clone());
        self.login_server_url = Some(url);
        self.login_profile_id = Some(pid.clone());
        match octos_app_store::keychain::load_token(&host, &pid) {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(e) => {
                log::warn!("keychain load failed ({e}); falling back to Login");
                false
            }
        }
    }

    /// Step 1 — `Continue` button. Validates the server URL, persists
    /// `~/.config/octos-app/server.json`, hides Step 1 and shows Step 2.
    fn login_continue_clicked(&mut self, cx: &mut Cx) {
        let url_str = self.ui.text_input(cx, ids!(login_server_url_input)).text();
        let pid_str = self.ui.text_input(cx, ids!(login_profile_id_input)).text();
        let pid_trimmed = pid_str.trim();
        if pid_trimmed.is_empty() {
            self.login_set_status(cx, "Profile ID is required");
            return;
        }
        let parsed = match crate::app::login::validate_server_url(&url_str) {
            Ok(u) => u,
            Err(e) => {
                self.login_set_status(cx, &e);
                return;
            }
        };
        let cfg = crate::app::login::ServerConfig {
            server_url: parsed.to_string(),
            profile_id: pid_trimmed.to_string(),
        };
        if let Err(e) = crate::app::login::save_server_config(&cfg) {
            self.login_set_status(cx, &format!("Failed to save server config: {e}"));
            return;
        }
        self.login_server_url = Some(parsed.clone());
        self.login_profile_id = Some(ProfileId::from(pid_trimmed.to_string()));
        self.ui
            .view(cx, ids!(login_server_step))
            .set_visible(cx, false);
        // Before falling back to the email OTP flow, try the password-free
        // solo sign-in that `octos serve --solo` exposes (same flow as
        // octos-web's local sign-in button). The email step only appears if
        // solo is unavailable (SoloReply handler below).
        self.login_set_status(cx, "Trying password-free sign-in…");
        self.ui.redraw(cx);
        let url = parsed;
        let pid = ProfileId::from(pid_trimmed.to_string());
        std::thread::spawn(move || {
            let outcome = run_blocking_solo_login(&url, &pid);
            Cx::post_action(LoginAsyncAction {
                kind: LoginAsyncEvent::SoloReply,
                error: outcome.err(),
            });
        });
    }

    /// Step 2 — `Send code` button. Drives `POST /api/auth/send-code`
    /// (octos-cli auth_handlers.rs:389). Server always returns `ok: true`
    /// (per the design note about preventing email-enumeration), so on a
    /// non-transport response we unconditionally advance to Step 3.
    fn login_send_code_clicked(&mut self, cx: &mut Cx) {
        let email = self.ui.text_input(cx, ids!(login_email_input)).text();
        let trimmed = email.trim().to_string();
        if trimmed.is_empty() || !trimmed.contains('@') {
            self.login_set_status(cx, "Enter a valid email address");
            return;
        }
        let Some(server_url) = self.login_server_url.clone() else {
            self.login_set_status(cx, "No server configured (Step 1)");
            return;
        };
        self.login_pending_email = Some(trimmed.clone());
        self.login_set_status(cx, "Sending code...");
        self.ui.redraw(cx);

        // Off-thread REST call: the UI thread cannot host an async runtime
        // (Makepad owns the event loop), so we build a one-shot
        // single-threaded tokio runtime on a worker thread, run the call,
        // and post a typed action back via `Cx::post_action`. No global
        // runtime, no shared state — the worker dies once it's posted.
        std::thread::spawn(move || {
            let result = run_blocking_send_code(&server_url, &trimmed);
            Cx::post_action(LoginAsyncAction {
                kind: LoginAsyncEvent::SendCodeReply,
                error: result.err(),
            });
        });
    }

    /// Step 3 — `Verify` button. Drives `POST /api/auth/verify`
    /// (octos-cli auth_handlers.rs:543). On `ok && token` the keychain
    /// stores the bearer keyed under `<host>::<profile_id>`.
    fn login_verify_clicked(&mut self, cx: &mut Cx) {
        let code = self.ui.text_input(cx, ids!(login_code_input)).text();
        let trimmed = code.trim().to_string();
        if trimmed.is_empty() {
            self.login_set_status(cx, "Enter the verification code");
            return;
        }
        let Some(server_url) = self.login_server_url.clone() else {
            self.login_set_status(cx, "No server configured (Step 1)");
            return;
        };
        let Some(pid) = self.login_profile_id.clone() else {
            self.login_set_status(cx, "No profile id configured (Step 1)");
            return;
        };
        let Some(email) = self.login_pending_email.clone() else {
            self.login_set_status(cx, "Send a code first");
            return;
        };
        self.login_set_status(cx, "Verifying...");
        self.ui.redraw(cx);

        std::thread::spawn(move || {
            let outcome = run_blocking_verify(&server_url, &email, &trimmed, &pid);
            Cx::post_action(LoginAsyncAction {
                kind: LoginAsyncEvent::VerifyReply,
                error: outcome.err(),
            });
        });
    }

    /// `Sign out` — clear keychain + reset the LoginScreen step state +
    /// flip the overlay back on. Server-side `/api/auth/logout`
    /// (auth_handlers.rs:680) is not yet plumbed; the bearer becomes
    /// invalid client-side regardless.
    fn login_sign_out(&mut self, cx: &mut Cx) {
        if let (Some(url), Some(pid)) = (
            self.login_server_url.clone(),
            self.login_profile_id.clone(),
        ) {
            let host = octos_app_store::auth::ServerHost::from(
                crate::app::login::host_from_url(&url),
            );
            if let Err(e) = octos_app_store::keychain::delete_token(&host, &pid) {
                log::warn!("delete_token failed (continuing logout): {e}");
            }
        }
        self.login_pending_email = None;
        // Login-free flow: dropping the bearer just re-provisions in the
        // background (fresh solo identity/token); the shell stays up.
        self.auto_solo_login(cx);
    }

    /// Background password-free sign-in. Ensures a server config exists
    /// (default: the on-device solo server) and spawns the solo attempt;
    /// the reply lands as `LoginAsyncEvent::SoloReply` in `handle_actions`.
    fn auto_solo_login(&mut self, cx: &mut Cx) {
        const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:50080";
        const DEFAULT_PROFILE: &str = "octos";
        if crate::app::login::load_server_config().is_none() {
            let cfg = crate::app::login::ServerConfig {
                server_url: DEFAULT_SERVER_URL.to_string(),
                profile_id: DEFAULT_PROFILE.to_string(),
            };
            if let Err(e) = crate::app::login::save_server_config(&cfg) {
                log::warn!("auto-solo: save default server config: {e}");
            }
        }
        let Some(cfg) = crate::app::login::load_server_config() else {
            return;
        };
        let Ok(url) = url::Url::parse(&cfg.server_url) else {
            log::warn!("auto-solo: bad server_url in config");
            return;
        };
        let pid = ProfileId::from(cfg.profile_id.clone());
        self.login_server_url = Some(url.clone());
        self.login_profile_id = Some(pid.clone());
        self.ui
            .label(cx, ids!(status_label))
            .set_text(cx, "Signing in…");
        std::thread::spawn(move || {
            let outcome = run_blocking_solo_login(&url, &pid);
            Cx::post_action(LoginAsyncAction {
                kind: LoginAsyncEvent::SoloReply,
                error: outcome.err(),
            });
        });
    }
}

// ---------------------------------------------------------------------------
// Off-thread helpers for the LoginScreen REST calls. Each call builds a
// one-shot single-threaded tokio runtime on a `std::thread::spawn` worker,
// runs the call, and posts a typed `LoginAsyncAction` back via
// `Cx::post_action`. No global runtime, no shared state.

fn run_blocking_send_code(server_url: &url::Url, email: &str) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    rt.block_on(async move {
        let client = octos_app_transport::rest::RestClient::new(
            reqwest::Client::new(),
            server_url.clone(),
            octos_app_transport::SecretString::new(""),
            octos_app_transport::ProfileId::new(""),
        );
        client
            .send_code(email)
            .await
            .map(|_| ())
            .map_err(|e| format!("send-code: {e}"))
    })
}

fn run_blocking_verify(
    server_url: &url::Url,
    email: &str,
    code: &str,
    profile_id: &ProfileId,
) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    let host = octos_app_store::auth::ServerHost::from(
        crate::app::login::host_from_url(server_url),
    );
    let pid = profile_id.clone();
    rt.block_on(async move {
        let client = octos_app_transport::rest::RestClient::new(
            reqwest::Client::new(),
            server_url.clone(),
            octos_app_transport::SecretString::new(""),
            octos_app_transport::ProfileId::new(""),
        );
        let resp = client
            .verify(email, code)
            .await
            .map_err(|e| format!("verify: {e}"))?;
        if !resp.ok {
            return Err(resp
                .message
                .unwrap_or_else(|| "Server rejected the code".to_string()));
        }
        let token = resp
            .token
            .ok_or_else(|| "Server returned ok=true but no token".to_string())?;
        let secret = octos_app_store::auth::SecretToken::from(token);
        octos_app_store::keychain::store_token(&host, &pid, &secret)
            .map_err(|e| format!("store_token: {e}"))
    })
}

/// Password-free sign-in against a server running `octos serve --solo`:
/// `POST /api/auth/solo` re-login first, then `POST /api/auth/solo/create`
/// on 404 (no solo owner yet) — mirroring octos-web's local sign-in. Stores
/// the bearer under the same keychain key the OTP flow uses.
fn run_blocking_solo_login(
    server_url: &url::Url,
    profile_id: &ProfileId,
) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct SoloUserLite {
        id: String,
    }
    #[derive(serde::Deserialize)]
    struct SoloCreateLite {
        profile_id: String,
    }
    #[derive(serde::Deserialize)]
    struct SoloTokenResp {
        token: String,
        // `POST /api/auth/solo` re-login returns the existing owner; adopt
        // its id so the bearer keys/config match the server's identity even
        // when the local default profile guess differs.
        #[serde(default)]
        user: Option<SoloUserLite>,
        #[serde(default)]
        result: Option<SoloCreateLite>,
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    let host = octos_app_store::auth::ServerHost::from(
        crate::app::login::host_from_url(server_url),
    );
    let pid = profile_id.clone();
    rt.block_on(async move {
        let client = reqwest::Client::new();
        let login_url = server_url
            .join("api/auth/solo")
            .map_err(|e| format!("solo url: {e}"))?;
        let resp = client
            .post(login_url)
            .send()
            .await
            .map_err(|e| format!("solo sign-in: {e}"))?;
        let parsed = match resp.status().as_u16() {
            200 => resp
                .json::<SoloTokenResp>()
                .await
                .map_err(|e| format!("solo response: {e}"))?,
            404 => {
                // No solo owner yet — create it (server must be in --solo
                // mode; anything else 403s below).
                let create_url = server_url
                    .join("api/auth/solo/create")
                    .map_err(|e| format!("solo create url: {e}"))?;
                let body = serde_json::json!({
                    "name": pid.as_str(),
                    "username": pid.as_str(),
                    "email": format!("{}@octos.local", pid.as_str()),
                });
                let resp = client
                    .post(create_url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| format!("solo create: {e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("solo create: HTTP {}", resp.status()));
                }
                resp.json::<SoloTokenResp>()
                    .await
                    .map_err(|e| format!("solo create response: {e}"))?
            }
            403 => return Err("Solo sign-in is disabled on this server".to_string()),
            s => return Err(format!("solo sign-in: HTTP {s}")),
        };
        // Adopt the server's owner identity (re-login returns the existing
        // solo owner even when our local profile guess differs) and keep the
        // on-disk config in lockstep so `resolve_bearer` finds the token.
        let owner = parsed
            .user
            .map(|u| u.id)
            .or(parsed.result.map(|r| r.profile_id))
            .unwrap_or_else(|| pid.as_str().to_owned());
        let owner_pid = octos_app_store::auth::ProfileId::from(owner.clone());
        let secret = octos_app_store::auth::SecretToken::from(parsed.token);
        octos_app_store::keychain::store_token(&host, &owner_pid, &secret)
            .map_err(|e| format!("store_token: {e}"))?;
        let _ = crate::app::login::save_server_config(&crate::app::login::ServerConfig {
            server_url: server_url.to_string(),
            profile_id: owner,
        });
        Ok(())
    })
}

/// Discriminator for cross-thread login replies. Carrying all arms through
/// one `ActionTrait` (auto-derived from `Debug + 'static` per
/// `aichat/platform/src/action.rs:21`) keeps the `Cx::post_action`
/// boilerplate down.
#[derive(Clone, Copy, Debug)]
enum LoginAsyncEvent {
    SendCodeReply,
    VerifyReply,
    /// Password-free `--solo` attempt fired by the Step-1 `Continue` button.
    SoloReply,
}

#[derive(Debug)]
struct LoginAsyncAction {
    kind: LoginAsyncEvent,
    error: Option<String>,
}

impl MatchEvent for App {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        // Build-time seed prompt. makepad on OpenHarmony has no native
        // text-input bridge yet (the IME opens but keystrokes never reach the
        // composer), so there is no way to drive the app by typing on device.
        // Building with OCTOS_SEED_PROMPT="…" submits it once as soon as the
        // agent and a foreground session exist. Absent that env var this
        // compiles to nothing.
        if let Some(seed) = option_env!("OCTOS_SEED_PROMPT") {
            if !self.seed_prompt_sent
                && self.agent.is_some()
                && self.fg_session().is_some()
            {
                self.seed_prompt_sent = true;
                log::info!("seed prompt: {seed:?}");
                self.submit_prompt(cx, seed.to_string());
            }
        }


        let opacity_slider = self.ui.slider(cx, ids!(opacity_slider));
        if let Some(opacity) = opacity_slider
            .slided(actions)
            .or_else(|| opacity_slider.end_slide(actions))
        {
            self.apply_glass_opacity(cx, opacity);
        }
        // Thinking + A2App toggles were removed (always-on A2App card app).

        // Reveal pill → expand the floating composer again after it auto-hid.
        if self.ui.button(cx, ids!(reveal_pill)).clicked(actions) {
            self.composer_shown = true;
            self.sync_composer(cx);
        }

        // Markdown link click — dispatch through robius-open for cross-platform
        // coverage (macOS/Linux/Windows/iOS/Android/WASM). Desktop requires a
        // modifier (Cmd on macOS, Cmd/Ctrl elsewhere) so plain clicks stay
        // available for drag-selection inside the Markdown widget; mobile &
        // web have no modifier concept, so a plain tap opens the URL.
        for action in actions {
            // Button press inside LLM-generated A2App/Splash UI. Update the
            // live counter from common event names and redraw so the
            // `{{state.count}}` placeholder reflects the new value; also toast
            // the action so any event is visibly acknowledged.
            if let makepad_widgets::SplashAction::Notify { event_id, payload } = action.cast() {
                // event_id is tagged "<card_id>:<event>" (see `tag_notify_calls`)
                // so the press routes to the card that fired it; `payload` is
                // JSON, optionally {"key": "<name>", "value": "<string>"}.
                let (card_id, ev) = match event_id.split_once(':') {
                    Some((id, rest)) => (id.parse::<usize>().ok(), rest.to_lowercase()),
                    None => (None, event_id.to_lowercase()),
                };
                let pj: serde_json::Value =
                    serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
                // Stock list↔detail navigation is client-side: a row tap and the
                // detail "back" button both fire `set`/`selected` (handled below),
                // re-substituting `{{state.selected}}` in the one combined card —
                // no per-tap LLM round-trip, no separate render path.
                let key = pj.get("key").and_then(|v| v.as_str()).unwrap_or("count").to_owned();
                let value = pj.get("value").and_then(|v| v.as_str()).map(str::to_owned);
                // Navigation: a weather-list row fires agent.notify("city",
                // {value:"<name>|<lat>|<lon>|<cond>"}). Render that city's REAL-glass
                // detail card directly (no LLM) and tail to it.
                // An event the runtime cannot attribute to a card is not
                // trustworthy: `tag_notify_calls` only rewrites a LITERAL first
                // argument, so a card that builds its event id dynamically
                // arrives untagged. State writes below are already gated on
                // `card_id` and so fail safe; this branch was not, and would
                // navigate on an unattributable event.
                if card_id.is_none() {
                    log!("[splash] ignoring untagged agent.notify({ev:?})");
                } else if ev == app::l0_widgets::TAP_CHANNEL {
                    // A tap on an L0 card.
                    //
                    // THE CHANNEL AND THE PAYLOAD MUST MATCH `l0_widgets::emit`.
                    // This listened for `"l0"` with flat `key`/`event`/`value`
                    // while the emitter sent `"l0kit"` with `{target: "l0:{…}"}`,
                    // so every tap on every L0 card fell through this branch and
                    // did nothing at all. Nothing caught it: the only tests that
                    // exercised a tap called `l0_card::tap` directly — the seeded
                    // `SEED_L0_EVENT` path does exactly that — so dispatch was
                    // well covered and the wire between the button and dispatch
                    // was covered nowhere.
                    //
                    // `target` is one string because `tag_notify_calls` rewrites
                    // only a LITERAL channel, so everything else has to travel in
                    // the payload. The `l0:` prefix distinguishes it from this
                    // renderer's own `set:` verbs (see `kit::tap_target`).
                    let target = pj.get("target").and_then(|v| v.as_str()).unwrap_or_default();
                    let (l0_key, l0_event, l0_value) =
                        app::l0_widgets::parse_tap(target).unwrap_or_default();
                    let (l0_key, l0_event, l0_value) =
                        (l0_key.as_str(), l0_event.as_str(), l0_value.as_str());
                    // The motion that just paged a card is not also a tap on
                    // the row it swept across (see L0_SWIPE_AT).
                    let swept = L0_SWIPE_AT.with(|t| {
                        t.get().is_some_and(|at| at.elapsed().as_millis() < 300)
                    });
                    if swept {
                        log!("[l0] tap suppressed: it was the swipe");
                        continue;
                    }
                    // A keystroke's state is applied NOW; its re-render is coalesced.
                    let is_keystroke = target.contains("\"c\":1");
                    match app::l0_card::tap(cx, card_id.unwrap_or(0), l0_key, l0_event, l0_value) {
                        Ok(Some((item, body))) if is_keystroke => {
                            let _ = (item, body);
                            L0_TYPING_PENDING.store(true, std::sync::atomic::Ordering::Relaxed);
                            self.l0_typing_rebuild = cx.start_timeout(0.35);
                        }
                        Ok(Some((item, body))) => {
                            // Only a message that still holds LOWERED DSL is
                            // rewritten. A `runl0` ledger is left alone: the
                            // draw path resolves it from the session, whose
                            // store the tap just wrote, so replacing the text
                            // with this body would trade a live card for the
                            // photograph of one — and every later fetch, tap and
                            // re-theme would land on nothing.
                            if let Ok(mut chat) = CHAT_DATA.write() {
                                if let Some(msg) = chat.messages.get_mut(item) {
                                    if !msg.text.contains("```runl0") {
                                        msg.text = format!("```runsplash\n{body}\n```");
                                    }
                                }
                            }
                            // The render cache is keyed by (item, raw text,
                            // state); the text changed, so it misses on its own.
                            // The generation bump is what makes the list rebuild
                            // the item rather than reuse its widget tree.
                            CHAT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            cx.redraw_all();
                            log!("[l0] {l0_event} on {l0_key} -> redrew item {item}");
                        }
                        // Distinguishable on purpose: "applied to nothing" and
                        // "refused" look identical on screen and are completely
                        // different to whoever is debugging.
                        Ok(None) => log!("[l0] {l0_event} on {l0_key} applied to nothing"),
                        Err(why) => log!("[l0] {l0_event} failed: {why}"),
                    }
                } else if ev.contains("city") {
                    if let Some(v) = value.as_deref() {
                        let p: Vec<&str> = v.split('|').collect();
                        if p.len() == 4 {
                            let dsl = glass_detail_card(p[0], p[1], p[2], p[3]);
                            if let Ok(mut data) = CHAT_DATA.write() {
                                data.messages.push(ChatMessage {
                                    role: ChatRole::Assistant,
                                    text: format!("```runsplash\n{dsl}\n```"),
                                });
                            }
                            CHAT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let chat_list = self.ui.widget(cx, ids!(chat_list));
                            chat_list.portal_list(cx, ids!(list)).set_tail_range(true);
                            self.update_empty_state_visibility(cx);
                            cx.redraw_all();
                        }
                    }
                    // fall through: ev "city" matches none of the counter ops below.
                }
                let mut changed = false;
                if let Some(card_id) = card_id {
                    if let Ok(mut data) = CHAT_DATA.write() {
                        let card = data.a2app_state.entry(card_id).or_default();
                        let cur = |c: &CardState| -> i64 {
                            c.get(&key).and_then(|s| s.parse().ok()).unwrap_or(0)
                        };
                        changed = true;
                        if ev.contains("inc") || ev.contains("plus") || ev.contains("add") {
                            let n = cur(card);
                            card.insert(key.clone(), (n + 1).to_string());
                        } else if ev.contains("dec") || ev.contains("minus") || ev.contains("sub") {
                            let n = cur(card);
                            card.insert(key.clone(), (n - 1).to_string());
                        } else if ev.contains("reset") || ev.contains("clear") {
                            card.insert(key.clone(), "0".to_owned());
                        } else if ev.starts_with("set") {
                            // `set` last: "reset" also contains "set".
                            match value {
                                Some(v) => {
                                    card.insert(key.clone(), v);
                                }
                                None => changed = false,
                            }
                        } else {
                            changed = false;
                        }
                    }
                }
                if changed {
                    self.refresh_a2app_templates(cx);
                }
            }
            if let Some(widget_action) = action.as_widget_action() {
                if let makepad_widgets::markdown::MarkdownAction::LinkNavigated { url, modifiers } =
                    widget_action.cast()
                {
                    let should_open = {
                        #[cfg(any(
                            target_os = "ios",
                            target_os = "android",
                            target_arch = "wasm32"
                        ))]
                        {
                            let _ = modifiers;
                            true
                        }
                        #[cfg(not(any(
                            target_os = "ios",
                            target_os = "android",
                            target_arch = "wasm32"
                        )))]
                        {
                            modifiers.logo || modifiers.control
                        }
                    };
                    if should_open {
                        if let Err(e) = robius_open::Uri::new(&url).open() {
                            log::warn!("failed to open URL {}: {:?}", url, e);
                        }
                    }
                }
            }
        }
        if self.ui.button(cx, ids!(send_button)).clicked(actions) {
            self.send_message(cx);
        }
        if self.ui.button(cx, ids!(cancel_button)).clicked(actions) {
            self.cancel_request(cx);
        }
        if self.ui.button(cx, ids!(clear_button)).clicked(actions) {
            self.clear_chat(cx);
        }
        // Sidebar `+ 新对话` — same semantics as Clear: wipe the local chat
        // surface and open a fresh session on the wire. On phone-width
        // windows also collapse the sidebar so the chat surface (previously
        // pushed off-screen) becomes visible — this is what makes the button
        // *look* like it did something on a portrait phone.
        if self.ui.button(cx, ids!(nav_new)).clicked(actions) {
            self.clear_chat(cx);
            {
                let mut state = APP_STATE.write().unwrap();
                octos_app_store::state::reduce(
                    &mut state,
                    octos_app_store::state::Event::Navigation(
                        NavigationEvent::NavigateTo(CurrentScreen::Home),
                    ),
                );
            }
            self.show_screen_for_nav(cx);
            self.collapse_sidebar_if_narrow(cx);
        }
        // Layer 3 (W08) — new-app / switch now live in the NATIVE composer pill
        // (see the NativeComposerNewApp/Switch action handlers above); no
        // top-strip or sidebar buttons.
        // The ☰ toggle went with the side panel: nothing may bring the
        // pane back over the AMA surface. (The button's row is already
        // invisible; this keeps a stray action from resurrecting the pane.)
        if self.ui.button(cx, ids!(nav_toggle)).clicked(actions) {
            self.ui.view(cx, ids!(sidebar)).set_visible(cx, false);
        }
        if self
            .ui
            .text_input(cx, ids!(input))
            .returned(actions)
            .is_some()
        {
            self.send_message(cx);
        }
        if self.ui.text_input(cx, ids!(input)).escaped(actions) {
            self.cancel_request(cx);
        }

        // Native floating composer submit → the same send path as the Makepad
        // composer (splash-mode wrapping, saved cards, streaming). The action is
        // posted from the platform's `onComposerSubmit` JNI callback on Android
        // (`android.rs::handle_message`) and from the ArkTS composer overlay's
        // napi callback on OpenHarmony (`open_harmony.rs::handle_message`).
        // Never fires on desktop.
        for action in actions {
            if let Some(sub) = action
                .downcast_ref::<makepad_widgets::makepad_platform::event::NativeComposerSubmit>()
            {
                let text = sub.text.clone();
                self.submit_prompt(cx, text);
            }
            // Deep link / share (e.g. a YouTube URL shared from another app): emit a
            // `deeplink` event to the web card, which plays it (octos.on("deeplink")).
            if let Some(dl) = action
                .downcast_ref::<makepad_widgets::makepad_platform::event::NativeDeepLink>()
            {
                let payload = format!(
                    "\"{}\"",
                    dl.url.replace('\\', "\\\\").replace('"', "\\\"")
                );
                cx.system_browser(web_card_browser_id()).emit("deeplink", &payload);
            }
            // Layer 3 — native composer "＋" / "⟳" controls (app management lives
            // in the composer now; the screen is otherwise just the a2app card).
            if action
                .downcast_ref::<makepad_widgets::makepad_platform::event::NativeComposerNewApp>()
                .is_some()
            {
                self.open_new_app(cx);
            }
            if action
                .downcast_ref::<makepad_widgets::makepad_platform::event::NativeComposerSwitch>()
                .is_some()
            {
                let n = self.apps.len();
                if n > 1 {
                    self.switch_to_app(cx, (self.foreground + 1) % n);
                }
            }
            // Native composer "+" FAB tapped to UNFOLD. Java already expanded the
            // pill + raised the keyboard; mark composer_shown so the app state
            // matches and a later sync_composer won't re-fold it mid-typing.
            if action
                .downcast_ref::<makepad_widgets::makepad_platform::event::NativeComposerExpand>()
                .is_some()
            {
                self.composer_shown = true;
            }
            // Composer QR scan → provision the LLM from the decoded JSON payload,
            // then respawn the kernel so the new provider/key takes effect.
            if let Some(scan) = action
                .downcast_ref::<makepad_widgets::makepad_platform::event::NativeQrScanned>()
            {
                let json = scan.json.clone();
                // A TOAST, not the status label: that label lives in the
                // desktop header, which this app hides ("no header chrome"),
                // so a scan reported success to a surface the phone never
                // draws — the user saw nothing and could not tell a good
                // scan from a bad one. Measured on the OnePlus 6.
                let (kind, message) = match crate::app::login::apply_provision_config_json(&json) {
                    Ok(what) => {
                        log::info!("QR provisioned LLM: {what}");
                        self.connect_transport(cx); // respawn kernel → reads new _main.json
                        self.clear_chat(cx);
                        (
                            octos_app_store::toasts::ToastKind::ReconnectSuccess,
                            format!("✓ {what}"),
                        )
                    }
                    Err(e) => {
                        log::warn!("QR provision failed: {e}");
                        (
                            octos_app_store::toasts::ToastKind::Error,
                            format!("QR error: {e}"),
                        )
                    }
                };
                if let Ok(mut st) = APP_STATE.write() {
                    st.toasts
                        .push(octos_app_store::toasts::Toast::new(kind, message.clone()));
                }
                self.ui.label(cx, ids!(status_label)).set_text(cx, &message);
                self.sync_toasts(cx);
            }
        }

        // ---- W08 — LoginScreen buttons + Sign out -------------------------
        if self.ui.button(cx, ids!(login_continue_button)).clicked(actions) {
            self.login_continue_clicked(cx);
        }
        if self.ui.button(cx, ids!(login_send_code_button)).clicked(actions) {
            self.login_send_code_clicked(cx);
        }
        if self
            .ui
            .text_input(cx, ids!(login_email_input))
            .returned(actions)
            .is_some()
        {
            self.login_send_code_clicked(cx);
        }
        if self.ui.button(cx, ids!(login_verify_button)).clicked(actions) {
            self.login_verify_clicked(cx);
        }
        if self
            .ui
            .text_input(cx, ids!(login_code_input))
            .returned(actions)
            .is_some()
        {
            self.login_verify_clicked(cx);
        }
        if self.ui.button(cx, ids!(sign_out_button)).clicked(actions) {
            self.login_sign_out(cx);
        }
        // Cross-thread login replies (`Cx::post_action`-delivered).
        for action in actions {
            let Some(la) = action.downcast_ref::<LoginAsyncAction>() else {
                continue;
            };
            match la.kind {
                LoginAsyncEvent::SendCodeReply => {
                    if let Some(err) = la.error.as_ref() {
                        self.login_set_status(cx, err);
                    } else {
                        self.login_set_status(cx, "Code sent — check your email.");
                        self.ui
                            .view(cx, ids!(login_email_step))
                            .set_visible(cx, false);
                        self.ui
                            .view(cx, ids!(login_code_step))
                            .set_visible(cx, true);
                        self.ui.redraw(cx);
                    }
                }
                LoginAsyncEvent::VerifyReply => {
                    if let Some(err) = la.error.as_ref() {
                        self.login_set_status(cx, err);
                    } else {
                        self.login_set_status(cx, "");
                        self.show_login(cx, false);
                        // Reset step visibility for a future logout.
                        self.ui
                            .view(cx, ids!(login_email_step))
                            .set_visible(cx, true);
                        self.ui
                            .view(cx, ids!(login_code_step))
                            .set_visible(cx, false);
                        // Pick up the fresh bearer without an app restart.
                        self.connect_transport(cx);
                        self.clear_chat(cx);
                    }
                }
                LoginAsyncEvent::SoloReply => {
                    if let Some(err) = la.error.as_ref() {
                        // Login-free flow: no OTP fallback UI — surface the
                        // reason on the shell status line and stay up.
                        self.ui.label(cx, ids!(status_label)).set_text(
                            cx,
                            &format!("Sign-in unavailable: {err}"),
                        );
                        self.ui.redraw(cx);
                    } else {
                        // Refresh cached identity from the (possibly
                        // solo-rewritten) server config before connecting.
                        if let Some(cfg) = crate::app::login::load_server_config() {
                            if let Ok(u) = url::Url::parse(&cfg.server_url) {
                                self.login_server_url = Some(u);
                            }
                            self.login_profile_id =
                                Some(ProfileId::from(cfg.profile_id));
                        }
                        // Pick up the fresh bearer without an app restart.
                        self.connect_transport(cx);
                        self.clear_chat(cx);
                        self.fire_auto_prompt(cx);
                    }
                }
            }
        }

        // Profile dropdown selection. M1 has at most one stub label; W08
        // populates `available_profiles` from `/api/my/profile` and switches
        // sessions when the user picks a different one. Until then we just
        // record the selection so `update_status` can reflect it.
        if let Some(index) = self
            .ui
            .drop_down(cx, ids!(backend_dropdown))
            .selected(actions)
        {
            if let Some((profile_id, _label)) = self.available_profiles.get(index) {
                self.current_profile = Some(profile_id.clone());
                self.update_status(cx);
            }
        }

        // (Per-message delete handler removed with the bubble close buttons
        // — user directive.)

        // W04 — fold `SessionListAction`s posted from REST hydrate / delete
        // tasks plus the `SessionList` widget's own click events. See
        // `app/src/app/sessions.rs`.
        for action in actions {
            // Session-resume history arrived (`session/hydrate` reply routed
            // through the transport drain). Fill the chat thread if the user
            // is still on that session.
            if let Some(h) =
                action.downcast_ref::<crate::backend::octos_ui::SessionResumeHydrated>()
            {
                if self.fg_session() == Some(h.session_id) {
                    let count = {
                        let mut data = CHAT_DATA.write().unwrap();
                        data.messages = h
                            .messages
                            .iter()
                            .filter_map(|(role, content)| {
                                let role = match role.as_str() {
                                    "user" => ChatRole::User,
                                    "assistant" => ChatRole::Assistant,
                                    // Tool/system rows aren't chat bubbles.
                                    _ => return None,
                                };
                                Some(ChatMessage { role, text: content.clone() })
                            })
                            .collect();
                        data.is_streaming = false;
                        data.messages.len()
                    };
                    self.update_status(cx);
                    self.update_empty_state_visibility(cx);
                    let chat_list = self.ui.widget(cx, ids!(chat_list));
                    let list = chat_list.portal_list(cx, ids!(list));
                    list.set_tail_range(true);
                    list.set_first_id_and_scroll(count.saturating_sub(1), 0.0);
                    cx.redraw_all();
                }
                continue;
            }
            let Some(sa) = action.downcast_ref::<SessionListAction>() else { continue };
            match sa {
                SessionListAction::Hydrated(list) => {
                    let mut state = APP_STATE.write().unwrap();
                    // Replace whatever skeleton was there. W04 § 4 calls
                    // `/api/sessions` "Locked"; the merged list is canonical.
                    state.sessions = octos_app_store::sessions::SessionMap::new();
                    // Insert reverse — `SessionMap::insert` puts the newest
                    // at the front, but the wire returns most-recent-first;
                    // pushing in reverse keeps the visible order stable.
                    for s in list.iter().rev() {
                        state.sessions.insert(s.clone());
                    }
                    drop(state);
                    self.ui.redraw(cx);
                }
                SessionListAction::Failed(msg) => {
                    // Surface in the status label until the M2 toast queue
                    // lands. Don't clobber the existing label if it carries
                    // an error from the chat path.
                    log::warn!("session list REST: {msg}");
                }
                SessionListAction::Selected(id) => {
                    {
                        let mut state = APP_STATE.write().unwrap();
                        // W04 / M2 — also flip out of Content (or wherever)
                        // back to Chat so picking a session in the sidebar
                        // re-shows the chat surface.
                        octos_app_store::state::reduce(
                            &mut state,
                            octos_app_store::state::Event::Navigation(
                                NavigationEvent::OpenSession(id.clone()),
                            ),
                        );
                    }
                    // Resume the server-side session and request its history
                    // (`session/hydrate` → `SessionResumeHydrated` action).
                    let resumed = self
                        .agent
                        .as_mut()
                        .and_then(|agent| agent.resume_session(cx, &id.0));
                    if let Some(sid) = resumed {
                        // Switch foreground to the resumed session (open a
                        // record if it isn't an app yet). Path B: the hydrate
                        // reply (SessionResumeHydrated) refills CHAT_DATA below.
                        self.focus_session(sid, "Session");
                        {
                            let mut data = CHAT_DATA.write().unwrap();
                            data.messages.clear();
                            data.streaming_text.clear();
            data.authoritative_text.clear();
                            data.thinking_text.clear();
                            data.is_streaming = false;
                            data.a2app_state.clear();
                        }
                        // The resumed session may or may not carry the Splash
                        // manual in its history — re-prime on next A2App use.
                        self.splash_primed = false;
                        self.ui
                            .label(cx, ids!(status_label))
                            .set_text(cx, "Loading session\u{2026}");
                        self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
                        self.update_empty_state_visibility(cx);
                        self.collapse_sidebar_if_narrow(cx);
                        cx.redraw_all();
                    }
                    self.show_screen_for_nav(cx);
                }
                SessionListAction::DeleteRequested(id) => {
                    // Optimistic remove + spawn the REST DELETE.
                    {
                        let mut state = APP_STATE.write().unwrap();
                        state.sessions.remove(id);
                        if state.current_session.as_ref() == Some(id) {
                            state.current_session = None;
                        }
                    }
                    let cfg = Self::placeholder_transport_config();
                    let rest_client = Self::build_rest_client(&cfg);
                    let fallback_profile = octos_app_store::auth::ProfileId::from(
                        cfg.profile_id.0.clone(),
                    );
                    sessions_mod::delete_session_remote(rest_client, id.clone(), fallback_profile);
                    self.ui.redraw(cx);
                }
                SessionListAction::Deleted(_id) => {
                    // Optimistic remove already applied; nothing to do until
                    // M2 toast surfaces a "deleted" confirmation.
                }
            }
        }

        // W05 — Approve / Deny clicks bubble through `ApprovalUiAction`,
        // dispatched in `app/src/app/approvals.rs::post_decision`. Optimistic
        // local transition to `PendingResponse` happens here so the buttons
        // immediately disable; the wire RPC reply lands as
        // `ApprovalAsyncAction` (see below).
        for action in actions {
            let Some(ui_a) = action.downcast_ref::<crate::app::approvals::ApprovalUiAction>()
            else {
                continue;
            };
            {
                let mut state = APP_STATE.write().unwrap();
                // `ApprovalDecision` is no longer `Copy` (FIX-01); clone for
                // both call sites below.
                state
                    .approvals
                    .pending_response(&ui_a.approval_id, ui_a.decision.clone());
            }
            if let Some(handle) = self.approval_handle.as_ref() {
                handle.respond(
                    ui_a.session_id.clone(),
                    ui_a.approval_id.clone(),
                    ui_a.decision.clone(),
                    ui_a.scope.clone(),
                );
            } else {
                // No agent yet (M1 boots without one) — surface as failed so
                // the buttons re-enable.
                let mut state = APP_STATE.write().unwrap();
                state
                    .approvals
                    .failed(&ui_a.approval_id, "agent not initialized");
            }
            self.ui.redraw(cx);
        }
        // W05 — wire RPC reply lands here. `Accepted` flips to `Decided`.
        // On `Failed` with code `-32011 APPROVAL_NOT_PENDING`, parse
        // `data.recorded_decision` and collapse the retry into the same
        // `Decided` transition the success path uses (handles double-click
        // idempotently; see octos-cli/src/api/ui_protocol_approvals.rs:198
        // and the v1 spec § approval/respond). Anything else flips to
        // `Failed { msg }` (the user can re-click; server-side idempotency
        // catches duplicates).
        const APPROVAL_NOT_PENDING: i64 = -32011;
        for action in actions {
            let Some(async_a) =
                action.downcast_ref::<crate::app::approvals::ApprovalAsyncAction>()
            else {
                continue;
            };
            let mut state = APP_STATE.write().unwrap();
            match &async_a.outcome {
                crate::app::approvals::ApprovalAsyncOutcome::Accepted { .. } => {
                    // FIX-01: ApprovalDecision is no longer Copy.
                    state
                        .approvals
                        .decided(&async_a.approval_id, async_a.decision.clone());
                }
                crate::app::approvals::ApprovalAsyncOutcome::Failed { message, code, data } => {
                    if *code == APPROVAL_NOT_PENDING {
                        let recorded = data
                            .as_ref()
                            .and_then(|d| d.get("recorded_decision"))
                            .and_then(|d| d.as_str())
                            .and_then(parse_recorded_decision)
                            .unwrap_or_else(|| async_a.decision.clone());
                        state.approvals.decided(&async_a.approval_id, recorded);
                    } else {
                        state.approvals.failed(&async_a.approval_id, message.clone());
                    }
                }
            }
            drop(state);
            self.ui.redraw(cx);
        }

        // ---- W04 / M2 — Content nav + filter wiring ----------------------
        if self.ui.button(cx, ids!(nav_content)).clicked(actions) {
            self.navigate_to_content(cx);
            self.collapse_sidebar_if_narrow(cx);
        }

        // (Coding / Studio / Slides / Sites navs removed — unsupported in
        // this build.)

        // ---- W07 / M3 — ProducerUiAction (source add / open external) -
        for action in actions {
            let Some(pa) =
                action.downcast_ref::<crate::app::producers::ProducerUiAction>()
            else {
                continue;
            };
            match pa {
                crate::app::producers::ProducerUiAction::AddSource { kind, text } => {
                    crate::app::producers::fold_add_source(*kind, text.clone());
                    self.ui.redraw(cx);
                }
                crate::app::producers::ProducerUiAction::SourceInputChanged {
                    kind,
                    text,
                } => {
                    crate::app::producers::fold_source_input_changed(
                        *kind,
                        text.clone(),
                    );
                }
                crate::app::producers::ProducerUiAction::OpenGeneration {
                    kind: _,
                    url,
                } => {
                    crate::app::producers::open_generation_externally(url);
                }
            }
        }

        // ---- W06 / M3 — CodingUiAction (queue / history selection) -------
        for action in actions {
            let Some(ca) = action.downcast_ref::<crate::app::coding::CodingUiAction>()
            else {
                continue;
            };
            match ca {
                crate::app::coding::CodingUiAction::SelectApproval(id) => {
                    crate::app::coding::fold_select_approval(id.clone());
                    self.ui.redraw(cx);
                }
                crate::app::coding::CodingUiAction::SelectHistory(id) => {
                    // History click reuses the same selection slot; the
                    // right-pane preview stays read-only because the
                    // `ApprovalState::Decided` rows have no Approve/Deny
                    // controls in the queue card.
                    crate::app::coding::fold_select_approval(id.clone());
                    self.ui.redraw(cx);
                }
                crate::app::coding::CodingUiAction::SelectTask(task_id) => {
                    crate::app::coding::fold_select_task(task_id.clone());
                    self.fire_task_output_read(task_id.clone());
                    self.ui.redraw(cx);
                }
            }
        }

        // ---- W06 / M3 — TaskOutputAction (output buffer fold) ------------
        for action in actions {
            let Some(ta) = action.downcast_ref::<crate::app::coding::TaskOutputAction>()
            else {
                continue;
            };
            match &ta.outcome {
                crate::app::coding::TaskOutputOutcome::Loaded(_) => {
                    // Clone the action so `fold_task_output` can take
                    // ownership — `downcast_ref` returns a borrow.
                    let cloned = crate::app::coding::TaskOutputAction {
                        task_id: ta.task_id.clone(),
                        session_id: ta.session_id.clone(),
                        outcome: match &ta.outcome {
                            crate::app::coding::TaskOutputOutcome::Loaded(r) => {
                                crate::app::coding::TaskOutputOutcome::Loaded(r.clone())
                            }
                            crate::app::coding::TaskOutputOutcome::Failed(s) => {
                                crate::app::coding::TaskOutputOutcome::Failed(s.clone())
                            }
                        },
                    };
                    crate::app::coding::fold_task_output(cloned);
                    self.ui.redraw(cx);
                }
                crate::app::coding::TaskOutputOutcome::Failed(msg) => {
                    log::warn!("task/output/read: {msg}");
                }
            }
        }
        if self
            .ui
            .button(cx, ids!(content_refresh_button))
            .clicked(actions)
        {
            self.fire_content_hydrate();
        }
        if let Some(idx) = self
            .ui
            .drop_down(cx, ids!(content_filter_dropdown))
            .selected(actions)
        {
            if let Ok(mut cs) = CONTENT_STATE.write() {
                cs.filter = ContentFilter::from_dropdown_index(idx);
            }
            self.fire_content_hydrate();
            self.ui.redraw(cx);
        }
        if let Some(text) = self
            .ui
            .text_input(cx, ids!(content_search_input))
            .changed(actions)
        {
            if let Ok(mut cs) = CONTENT_STATE.write() {
                cs.search = text;
            }
            self.ui.redraw(cx);
        }

        // ---- W04 / M2 — ContentAction (REST hydrate + card click) -------
        for action in actions {
            let Some(ca) = action.downcast_ref::<ContentAction>() else { continue };
            match ca {
                ContentAction::Hydrated(metas) => {
                    let mut state = APP_STATE.write().unwrap();
                    content_mod::fold_hydrated(&mut state, metas.clone());
                    drop(state);
                    if let Ok(mut cs) = CONTENT_STATE.write() {
                        cs.last_error = None;
                    }
                    self.ui.redraw(cx);
                }
                ContentAction::Failed(msg) => {
                    log::warn!("content hydrate REST: {msg}");
                    if let Ok(mut cs) = CONTENT_STATE.write() {
                        cs.last_error = Some(msg.clone());
                    }
                    self.ui.redraw(cx);
                }
                ContentAction::Open(handle) => {
                    self.open_viewer_for(cx, handle.clone());
                }
            }
        }

        // ---- W04 / M2 — ViewerAction (overlay close, prev/next, OS handoff) -
        for action in actions {
            let Some(va) = action.downcast_ref::<ViewerAction>() else { continue };
            match va {
                ViewerAction::Close => self.close_viewer(cx),
                ViewerAction::AlbumStep(delta) => self.album_step(cx, *delta),
                ViewerAction::OpenInOs(handle) => self.open_in_os(handle),
                ViewerAction::MarkdownLoaded { handle, body } => {
                    if let Ok(mut vs) = VIEWER_STATE.write() {
                        vs.markdown_cache.insert(handle.clone(), body.clone());
                        vs.last_error = None;
                    }
                    self.ui.redraw(cx);
                }
                ViewerAction::MarkdownFailed { handle, error } => {
                    log::warn!("markdown fetch {handle}: {error}");
                    if let Ok(mut vs) = VIEWER_STATE.write() {
                        vs.last_error = Some(error.clone());
                    }
                    self.ui.redraw(cx);
                }
            }
        }
    }

    fn handle_startup(&mut self, cx: &mut Cx) {
        // Android: route the real `log` facade (transport/store crates) to
        // logcat — without this their records are dropped silently.
        octos_app_transport::install_android_logger();

        // This app is a full-screen A2App card generator: A2App mode is always
        // on (the toggle was removed). The floating composer starts FOLDED —
        // only the "+" FAB shows until the user taps it to unfold (keeps the
        // card full-screen; matches the native overlay's folded-by-default).
        self.splash_mode = true;
        self.composer_shown = false;

        // DEBUG: enable the fork's image decode tracing (decode_start/done,
        // gpu_commit) — diagnosing the first-image-of-a-fresh-process black
        // photo. Must be set before the first decode (OnceLock).
        std::env::set_var("MAKEPAD_GLTF_TEX_DEBUG", "1");

        // Mobile: the process has no usable HOME, and everything below
        // (server.json, the token store, chat persistence) is HOME-relative.
        // Point HOME at the app-private files dir the platform reports —
        // `getFilesDir()` on Android, the HAP sandbox root on OpenHarmony —
        // before any config path is resolved. Without it boot fails with
        // "auto-solo: save default server config: Operation not permitted"
        // and the UI comes up blank.
        #[cfg(mobile)]
        {
            // OHOS needs a fallback: its sandbox root is fixed and always
            // writable, whereas a missing Android data dir means there is
            // nothing sensible to point at.
            #[cfg(target_env = "ohos")]
            let dir = Some(
                cx.get_data_dir()
                    .unwrap_or_else(|| "/data/storage/el2/base/files".to_string()),
            );
            #[cfg(not(target_env = "ohos"))]
            let dir = cx.get_data_dir();

            if let Some(dir) = dir {
                let _ = std::fs::create_dir_all(&dir);
                std::env::set_var("HOME", &dir);
                log::info!("mobile: HOME={dir}");
            }
        }

        // Provisioning deploy (non-rooted devices): `makepad.PROVISION_DIR`
        // (→ env MAKEPAD_PROVISION_DIR) names a world-readable staging dir
        // (`adb push …/octos-provision`) whose tree is copied into the app's
        // octos-home BEFORE octos spawns — deploying the GLM profile + a2app
        // memory tree onto a device that can't be written via su/run-as.
        #[cfg(target_os = "android")]
        if let Ok(src) = std::env::var("MAKEPAD_PROVISION_DIR") {
            let home = std::path::PathBuf::from(
                "/data/user/0/dev.makepad.octos_app/files/octos-home",
            );
            match deploy_provision(std::path::Path::new(&src), &home) {
                Ok(n) => log::info!("provision: deployed {n} files from {src}"),
                Err(e) => log::warn!("provision: deploy from {src} failed: {e}"),
            }
        }

        // No-UI provisioning: a `makepad.APP_CONFIG` launch-intent extra
        // (`adb shell am start … --es makepad.APP_CONFIG
        // 'http://host:port|profile|token'`) surfaces here as the
        // MAKEPAD_APP_CONFIG env var. It writes the server config + bearer
        // BEFORE the boot-auth decision, so a provisioned device lands
        // straight on the home shell — no LoginScreen typing. A QR-scan
        // onboarding can feed the same `apply_provision_string` entry later.
        if let Ok(prov) = std::env::var("MAKEPAD_APP_CONFIG") {
            match crate::app::login::apply_provision_string(&prov) {
                Ok(()) => log::info!("provisioned from launch intent"),
                Err(e) => log::warn!("provisioning failed: {e}"),
            }
        }
        // QR / intent LLM provisioning: a `makepad.PROVISION_CONFIG` extra (a JSON
        // payload `{"llm_family":..,"llm_model":..,"llm_key":..}`, the same content
        // the composer's QR scan yields) writes the provider + key into the octos
        // profile config BEFORE the kernel spawns below, so the first turn uses it.
        if let Ok(cfg) = std::env::var("MAKEPAD_PROVISION_CONFIG") {
            match crate::app::login::apply_provision_config_json(&cfg) {
                Ok(what) => log::info!("provisioned LLM from intent: {what}"),
                Err(e) => log::warn!("LLM provisioning failed: {e}"),
            }
            std::env::remove_var("MAKEPAD_PROVISION_CONFIG");
        }


        // Construct the OctosUiAgent up-front so the chat surface has
        // somewhere to send a prompt (config/token state as currently on
        // disk; re-run by the login flow once a fresh bearer lands).
        self.connect_transport(cx);

        // Profile dropdown. W08 will populate `available_profiles` from
        // `/api/my/profile`; for M1 we hand the dropdown the stub label
        // already declared in the live-DSL.
        if !self.available_profiles.is_empty() {
            self.ui
                .drop_down(cx, ids!(backend_dropdown))
                .set_selected_item(cx, 0);
            self.current_profile = self
                .available_profiles
                .first()
                .map(|(id, _)| id.clone());
        }

        self.update_status(cx);
        self.update_connection_indicator(cx);
        self.update_context_indicator(cx);
        self.update_empty_state_visibility(cx);
        self.ui
            .slider(cx, ids!(opacity_slider))
            .set_value(cx, DEFAULT_GLASS_OPACITY);
        // Thinking toggle is inert in M1 (see `handle_actions` comment); the
        // initial state is whatever the DSL declared (`active: false`).
        self.apply_glass_opacity(cx, DEFAULT_GLASS_OPACITY);

        // ---- W08 — boot decision: LoginScreen vs Home ---------------------
        // Login-free boot (user directive): the LoginScreen is never shown.
        // Auth resolves silently — stored bearer > background solo sign-in
        // against the configured (or default on-device) server. Provisioning
        // stays available via the `makepad.APP_CONFIG` intent extra.
        // EMBEDDED KERNEL short-circuit: when liboctos.so is bundled/staged,
        // the app talks to a trusted local process over stdio — no bearer, no
        // solo sign-in. Without this, boot fell through to auto_solo_login,
        // which POSTs http://127.0.0.1:50080/api/auth/solo — a port nothing
        // listens on in stdio mode — so sign-in always failed, `clear_chat`
        // never ran, no sessions were created, and every composer submit was
        // silently dropped (dead app on a fresh embedded-kernel install).
        #[cfg(mobile)]
        let authed = Self::has_embedded_kernel() || self.boot_is_authed();
        #[cfg(not(mobile))]
        let authed = self.boot_is_authed();
        self.show_login(cx, false);
        // W04 / M2 — make sure the chat_screen / content_screen pair
        // matches the boot navigation state (defaults to Home → Chat).
        self.show_screen_for_nav(cx);
        if authed {
            // Open the first session immediately so the composer is live.
            self.clear_chat(cx);
            self.fire_auto_prompt(cx);
        } else {
            self.auto_solo_login(cx);
        }
        // TEST-ONLY: seed a canned `runsplash` card from a file (bypasses the
        // server/LLM) so on-device render/scroll/map tests don't depend on card
        // generation. `--es makepad.SEED_CARD_FILE <app-readable path>` surfaces as
        // MAKEPAD_SEED_CARD_FILE. Push AFTER the boot decision above (clear_chat
        // wipes CHAT_DATA), then refresh the empty-state + redraw so it shows.
        if let Ok(path) = std::env::var("MAKEPAD_SEED_CARD_FILE") {
            match std::fs::read_to_string(&path) {
                Ok(body) => {
                    let body_trim = body.trim();
                    // An HTML document seeds a runhtml web app card; anything
                    // else is a Splash card as before.
                    let fence = if body_trim.starts_with("<!DOCTYPE")
                        || body_trim.starts_with("<!--")
                        || body_trim.starts_with("<html")
                    {
                        "runhtml"
                    } else {
                        "runsplash"
                    };
                    if let Ok(mut data) = CHAT_DATA.write() {
                        data.messages.push(ChatMessage {
                            role: ChatRole::Assistant,
                            text: format!("```{}\n{}\n```", fence, body_trim),
                        });
                    }
                    self.update_empty_state_visibility(cx);
                    cx.redraw_all();
                    log::info!("SEED_CARD injected {} bytes from {path}", body.len());
                }
                Err(e) => log::warn!("SEED_CARD_FILE read failed: {e}"),
            }
        }

        // `--es makepad.SEED_L0_FILE <card> --es makepad.SEED_L0_DATA <json>`
        // seeds an L0 LEDGER rather than a lowered card, so the app holds the
        // source and can re-realize it when a tap arrives. SEED_CARD_FILE above
        // pushes already-lowered DSL, which is inert by construction: nothing on
        // device knows what card produced it or what a tap would mean.
        if let Ok(card_path) = std::env::var("MAKEPAD_SEED_L0_FILE") {
            let data_path = std::env::var("MAKEPAD_SEED_L0_DATA").unwrap_or_default();
            let card = std::fs::read_to_string(&card_path);
            let blob = std::fs::read_to_string(&data_path);
            match (card, blob) {
                (Ok(source), Ok(raw)) => {
                    let data: serde_json::Value =
                        serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
                    let store = splash_ui_l0::InstanceStore::default();
                    // Realize once to CHECK it, and report the reason if it
                    // fails — a seeded card that cannot realize must say so
                    // rather than leave a blank screen.
                    match app::l0_card::render(cx, &source, &data, &store) {
                        Ok(_) => {
                            // Then inject the LEDGER, not what it lowered to.
                            //
                            // This pushed the lowered DSL, and that made the
                            // seeded card a photograph: `resolve_a2app_card`
                            // finds no `runl0` fence in it, so no redraw ever
                            // resolves it again. The stock card's prices stayed
                            // em dashes over data that had already arrived —
                            // the epoch bumped, the cache cleared, the frame
                            // drew, and the message still held the dead text
                            // rendered before the fetch landed.
                            //
                            // A ledger goes through the same path a generated
                            // card does, which is also the only way seeding
                            // tests anything the app actually runs.
                            let item = if let Ok(mut chat) = CHAT_DATA.write() {
                                chat.messages.push(ChatMessage {
                                    role: ChatRole::Assistant,
                                    text: format!("```runl0\n{source}\n```"),
                                });
                                chat.messages.len() - 1
                            } else {
                                0
                            };
                            app::l0_card::begin(source, data, item);
                            // `--es makepad.SEED_L0_EVENT <event> --es
                            // makepad.SEED_L0_VALUE <payload>` opens the card on
                            // a state a tap would have reached. The harness has
                            // passed these all along and nothing read them, so
                            // every "detail view" capture was silently a capture
                            // of the list.
                            if let Ok(event) = std::env::var("MAKEPAD_SEED_L0_EVENT") {
                                let value =
                                    std::env::var("MAKEPAD_SEED_L0_VALUE").unwrap_or_default();
                                match app::l0_card::tap(cx, item, "root", &event, &value) {
                                    Ok(Some(_)) => {
                                        log::info!("SEED_L0 event {event}({value}) applied")
                                    }
                                    Ok(None) => {
                                        log::warn!("SEED_L0 event {event}({value}) applied to nothing")
                                    }
                                    Err(why) => log::warn!("SEED_L0 event failed: {why}"),
                                }
                            }
                            self.update_empty_state_visibility(cx);
                            cx.redraw_all();
                            log::info!("SEED_L0 injected from {card_path} as item {item}");
                        }
                        // A card that does not realize is a blank screen with
                        // the reason in logcat, so say the reason on screen.
                        Err(why) => {
                            if let Ok(mut chat) = CHAT_DATA.write() {
                                chat.messages.push(ChatMessage {
                                    role: ChatRole::Assistant,
                                    text: format!("L0 card did not realize: {why}"),
                                });
                            }
                            self.update_empty_state_visibility(cx);
                            cx.redraw_all();
                            log::warn!("SEED_L0 realize failed: {why}");
                        }
                    }
                }
                (card, blob) => log::warn!(
                    "SEED_L0 read failed: card={:?} data={:?}",
                    card.err(),
                    blob.err()
                ),
            }
        }

        // `--es makepad.FAKE_GPS_FILE <path>` walks a track of `lat,lon` lines,
        // one fix per interval, as if the device were driving it.
        //
        // DEBUG SCAFFOLDING, and it exists because there is no other way to test
        // that navigation MOVES. Everything downstream of `sys.gps` was verified
        // correct and frozen: the follow camera, the turn instruction, the distance
        // remaining. The handset that renders these cards sits 3.7 km from the
        // nearest routable road, so every route that can be named near it reports
        // zero progress — correctly — and a broken follow camera and a working one
        // are the same screenshot. Driving the phone is the only alternative.
        //
        // A card cannot see this. It writes the same store the Android
        // `LocationListener` writes, through the same function, so the fix arrives
        // by the path a real one does and bumps the same epoch. Nothing in the
        // language, the kit or the widgets knows the difference — which is the
        // point: it tests the real chain rather than a parallel one.
        if let Ok(track_path) = std::env::var("MAKEPAD_FAKE_GPS_FILE") {
            match std::fs::read_to_string(&track_path) {
                Ok(text) => {
                    let track: Vec<(f64, f64)> = text
                        .lines()
                        .filter_map(|l| {
                            let (a, b) = l.trim().split_once(',')?;
                            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
                        })
                        .collect();
                    let step_ms: u64 = std::env::var("MAKEPAD_FAKE_GPS_MS")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1000);
                    log::info!(
                        "FAKE_GPS: walking {} fixes from {track_path} every {step_ms}ms",
                        track.len()
                    );
                    // The track OWNS the position from here on: the real
                    // LocationListener is muted, or a phone whose location wakes
                    // up mid-run feeds the camera two positions 700 ms apart and
                    // live nav jumps between the drive and the desk.
                    makepad_widgets::makepad_draw::makepad_platform::gps::claim_fake_gps();
                    std::thread::spawn(move || {
                        for (i, (lat, lon)) in track.iter().enumerate() {
                            makepad_widgets::makepad_draw::makepad_platform::gps::set_gps_fix(
                                *lat, *lon, 8.0,
                            );
                            log::info!("FAKE_GPS: fix {i} at {lat:.5},{lon:.5}");
                            std::thread::sleep(std::time::Duration::from_millis(step_ms));
                        }
                        log::info!("FAKE_GPS: track finished");
                    });
                }
                Err(e) => log::warn!("FAKE_GPS_FILE read failed: {e}"),
            }
        }

        // Phone boot: land on the chat surface, not the menu — ☰ opens it.
        self.collapse_sidebar_if_narrow(cx);
        // Settle composer visibility now (not only via the auth→clear_chat
        // path): on Android this hides the Makepad docked composer and raises
        // the native floating overlay, so an unauthed boot doesn't briefly show
        // both. On desktop it just reflects `composer_shown` (true at boot).
        self.sync_composer(cx);
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        // NOTE: `agent.notify(...)` for A2App/Splash button callbacks is
        // registered inside `makepad_widgets::script_mod` so it reaches the
        // isolated Splash VMs too (see aichat/widgets/src/lib.rs).
        crate::makepad_widgets::script_mod(vm);
        crate::makepad_code_editor::script_mod(vm);
        crate::makepad_diagram_kit::script_mod(vm);
        // W08 — register the LoginScreen DSL prototype before this file's
        // `script_mod` runs so `body +: { LoginScreen { … } }` resolves.
        crate::app::login::script_mod(vm);
        // W05 — register the ApprovalsPane / ApprovalCardView prototypes
        // so the chat scene can place `ApprovalsPane {}` between
        // `chat_shell` and `composer_row`.
        crate::app::approvals::script_mod(vm);
        // W04 / M2 — register `ContentBrowser` and `ViewerOverlay`
        // prototypes so the live-DSL `content_screen := ContentBrowser {}`
        // and `viewer_overlay := ViewerOverlay {}` references resolve.
        crate::app::content_browser::script_mod(vm);
        crate::app::viewers::script_mod(vm);
        // Swimming-octopus thinking indicator (chat screen, above composer).
        crate::app::octo_thinking::script_mod(vm);
        // W06 / M3 — register `CodingScreen` so the live-DSL
        // `coding_screen := CodingScreen {}` sibling resolves.
        crate::app::coding::script_mod(vm);
        // W07 / M3 — `StudioScreen` / `SlidesScreen` / `SitesScreen`
        // and the inner `GenerationCard` DSL prototypes are inlined into
        // `self::script_mod` below (mirrors the `SessionList` / `TaskDock`
        // pattern); their Rust impls live in `app/src/app/producers.rs`.
        self::script_mod(vm)
    }

    fn after_new_from_script(_vm: &mut ScriptVm, app: &mut Self) {
        // W04 will replace this with a SQLite per-session cache hydrate +
        // REST snapshot. For now, `load_from_disk` is a no-op stub so the
        // binary boots without touching disk.
        CHAT_DATA.write().unwrap().messages = ChatData::load_from_disk();
        // `available_profiles` stays empty until W08 hydrates it.
        app.available_profiles = Vec::new();
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // The L0 reader overlay (a card wrote a page to `sys.link`). The spawn
        // happened in l0_card::tap, where no widget area exists — the native
        // view is positioned HERE, once, to the full window, on the first
        // event after it. System back closes the reader instead of the app;
        // `handled` is how the platform is told the press was consumed.
        if app::l0_card::L0_READER_OPEN.load(std::sync::atomic::Ordering::Relaxed) {
            if let Event::BackPressed { handled } = event {
                cx.system_browser(web_card_browser_id()).detach();
                makepad_widgets::splash::set_link("");
                app::l0_card::L0_READER_OPEN
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                handled.set(true);
            } else if !app::l0_card::L0_READER_PLACED
                .swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                // The Window WidgetRef's own area is a zero rect (measured);
                // the chat list is the drawn region the card occupies, which
                // is exactly what the reader should cover.
                let over = self.ui.widget(cx, ids!(chat_list)).area();
                log::info!("[l0] reader placed over {:?}", over.rect(cx));
                cx.system_browser(web_card_browser_id()).update(over, true);
            }
        } else if let Event::BackPressed { handled } = event {
            // No reader up: offer the press to the latest card as its own
            // `back` event — the edge-swipe gesture is how a story detail
            // returns to its list. Consumed only when a cell actually moved,
            // so at the card's top view the press still leaves the app.
            if !handled.get() && self.l0_gesture(cx, "back") {
                handled.set(true);
            }
        } else if let Event::TouchUpdate(tu) = event {
            // A horizontal swipe over the card, offered as the card's own
            // swipe event — how the weather card pages through saved cities.
            // The raw touch stream carries no start position, so Start points
            // are remembered here and matched by uid at Stop. Thresholds are
            // tuned to a THUMB, not to adb: a real swipe takes up to a second
            // and arcs diagonally (measured — a 700ms drag with 80px of drift
            // is a normal human swipe, and the first cut rejected it), so the
            // gate is mostly-horizontal, far enough to be deliberate, and
            // under 1.5s; only a slow press-and-hold drag stays excluded.
            use makepad_widgets::makepad_draw::makepad_platform::event::TouchState;
            for touch in &tu.touches {
                match touch.state {
                    TouchState::Start => {
                        L0_TOUCH_STARTS.with(|m| {
                            m.borrow_mut()
                                .insert(touch.uid, (touch.abs.x, touch.abs.y, touch.time))
                        });
                    }
                    TouchState::Stop => {
                        let start =
                            L0_TOUCH_STARTS.with(|m| m.borrow_mut().remove(&touch.uid));
                        if let Some((x0, y0, t0)) = start {
                            let dx = touch.abs.x - x0;
                            let dy = touch.abs.y - y0;
                            let dt = touch.time - t0;
                            // No time cap: a person testing a reveal drags
                            // SLOWLY, expecting the row to follow the finger,
                            // and a 2s deliberate drag is still a swipe. Only
                            // direction and distance gate now.
                            let fired = dx.abs() > 48.0 && dx.abs() > dy.abs() * 1.2;
                            // One line per stroke, fired or not — "not
                            // working" is only diagnosable if the rejected
                            // stroke's numbers are in the log.
                            log::info!(
                                "[l0] stroke dx={dx:.0} dy={dy:.0} dt={dt:.2}s fired={fired}"
                            );
                            if fired {
                                let ev =
                                    if dx < 0.0 { "swipe_left" } else { "swipe_right" };
                                L0_SWIPE_AT.with(|t| t.set(Some(std::time::Instant::now())));
                                self.l0_gesture(cx, ev);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // Build with OCTOS_SEED_CARD=1 to push one of the prebuilt Splash
        // weather cards straight into the conversation, bypassing the AMA and
        // the LLM entirely. The card is pure Splash DSL with live
        // `sys.weather(...)` bindings, so it exercises the real native renderer
        // and real open-meteo data — useful when no LLM key is available.
        // Re-inject whenever the conversation is empty rather than once: the
        // app bulk-replaces CHAT_DATA during session restore / app switch,
        // which wiped a one-shot injection (card flashed, then vanished).
        // Pushing only when empty is self-limiting.
        if option_env!("OCTOS_SEED_CARD").is_some()
            && CHAT_DATA.read().map(|d| d.messages.is_empty()).unwrap_or(false)
        {
            self.seed_card_shown = true;
            // The reference card substitutes freshly-resolved live ids over its
            // baked placeholders (live ids rotate every few days). This is the
            // only reader of that cache, so it warms it itself.
            refresh_youtube_live_ids();
            // OCTOS_SEED_CARD=web (or =youtube) seeds a `runhtml` web app card
            // instead of the native Splash one, so the webview substrate can be
            // exercised without an LLM. The card embeds a YouTube live stream,
            // which is also the youtube app's own card format.
            // `None` means "not yet" — the card defers this frame WITHOUT
            // returning from handle_event, which would starve every other
            // handler below (including the ones that drive the resolver we are
            // waiting on).
            let text = match option_env!("OCTOS_SEED_CARD") {
                // The nav app is a DETERMINISTIC served card (no LLM), so it can
                // be seeded verbatim exactly the way `route_to_app` serves it —
                // including the origin/destination state, which is what makes it
                // open on a real A->B route preview instead of an empty search
                // box. Destination/origin come from OCTOS_SEED_NAV[_FROM].
                Some("nav") => {
                    let dest = option_env!("OCTOS_SEED_NAV").unwrap_or("SFO");
                    let orig = option_env!("OCTOS_SEED_NAV_FROM");
                    log::info!(
                        "seed card: nav canonical card, {} bytes, to={dest:?} from={orig:?}",
                        NAV_CANONICAL_CARD.len()
                    );
                    self.seed_nav_state = Some((dest.to_string(), orig.map(|s| s.to_string())));
                    Some(format!("```runsplash\n{NAV_CANONICAL_CARD}\n```"))
                }
                // Both substrates on screen at once: a native Splash card
                // (GPU fragment shader) stacked above a webview card, to check
                // the ArkTS Web overlay really clips to its own rect instead of
                // covering the whole surface. Pushed as TWO messages so the
                // chat list stacks them; handled below via `seed_split_web`.
                // Both substrates in ONE message: a native Splash card (GPU
                // fragment shader) above a webview card. One message rather
                // than two so the blocks are adjacent and the self-healing
                // re-injection (which fires whenever CHAT_DATA is empty) can't
                // interleave duplicates of a two-message pair.
                Some("split") => {
                    let gpu = include_str!("../../../docs/webview-cards/split-gpu.splash");
                    let web = include_str!("../../../docs/webview-cards/split-web.html");
                    log::info!("seed card: split — native GPU card + webview card");
                    Some(format!("```runsplash\n{gpu}\n```\n\n```runhtml\n{web}\n```"))
                }
                // A real news reader: crawls Hacker News and ZeroHedge live
                // through the http.fetch bridge (both hosts are on the
                // allowlist in web_card.rs) and renders an Apple-News-style
                // reader with sections, a lead story, article pages and
                // external links.
                Some("news") => {
                    let card = include_str!("../../../docs/webview-cards/news.html");
                    log::info!("seed card: {} bytes of html (news)", card.len());
                    Some(format!("```runhtml\n{card}\n```"))
                }
                // Exercises the JS→native bridge (octos_native.invoke) against
                // real native components: filesystem, clipboard, and the system
                // file picker.
                Some("bridge") => {
                    let card = include_str!("../../../docs/webview-cards/native-bridge.html");
                    log::info!("seed card: {} bytes of html (bridge)", card.len());
                    Some(format!("```runhtml\n{card}\n```"))
                }
                Some(kind @ ("web" | "youtube")) => {
                    // Every video id comes from the app's own live-id resolver.
                    // NOTHING is hardcoded: a YouTube live id goes stale within
                    // days, and a stale one does not degrade gracefully — the
                    // player shows "this live stream recording is not
                    // available" instead of playing anything.
                    //
                    // Resolution takes a few seconds after boot and lands one
                    // channel at a time, so wait for the whole set (bounded)
                    // rather than seeding with a half-filled card.
                    let resolved: Vec<(String, String)> = {
                        let cache = youtube_live_cache().lock().unwrap();
                        YOUTUBE_LIVE_CHANNELS
                            .iter()
                            .filter_map(|(handle, label)| {
                                cache.get(*handle).map(|id| (id.clone(), (*label).to_string()))
                            })
                            .collect()
                    };
                    self.seed_card_waits += 1;
                    let all_in = resolved.len() == YOUTUBE_LIVE_CHANNELS.len();
                    if !all_in && self.seed_card_waits < 600 {
                        None
                    } else {
                        // `youtube` seeds the REAL app — the full YouTube player
                        // the youtube agent composes (top bar, sticky player,
                        // feed, channel rows, PiP), which is what runs on
                        // Android. `web` seeds a minimal card instead, kept for
                        // diagnosing the webview substrate itself.
                        let card = if kind == "youtube" {
                            patch_youtube_live_ids(include_str!(
                                "../../../docs/youtube-player-reference.html"
                            ))
                        } else {
                            let json = resolved
                                .iter()
                                .map(|(id, label)| {
                                    format!("{{\"id\":\"{id}\",\"label\":\"{label}\"}}")
                                })
                                .collect::<Vec<_>>()
                                .join(",");
                            include_str!("../../../docs/webview-cards/youtube-live.html")
                                .replace("__CHANNELS_JSON__", &format!("[{json}]"))
                        };
                        log::info!(
                            "seed card: {} bytes of html ({kind}), {} live channels resolved",
                            card.len(),
                            resolved.len()
                        );
                        Some(format!("```runhtml\n{card}\n```"))
                    }
                }
                // A card lowered from a real PLAN, so the plan pipeline and its
                // live bindings can be exercised on device without a backend --
                // the seed-prompt path needs the agent, and the agent needs a
                // gateway. This is the same `lower_plan` the runplan fence uses.
                Some(kind @ ("aimovers" | "shanghai")) => {
                    let plan = if kind == "aimovers" {
                        SEED_PLAN_AI_MOVERS
                    } else {
                        SEED_PLAN_SHANGHAI
                    };
                    match crate::app::plan::lower_plan(plan) {
                        Ok(dsl) => {
                            log::info!("seed card: {kind} plan -> {} bytes", dsl.len());
                            Some(format!("```runsplash\n{dsl}\n```"))
                        }
                        Err(e) => {
                            log::warn!("seed plan {kind} rejected: {e}");
                            Some(format!(
                                "```runsplash\n{}\n```",
                                {
                                    log::warn!("seed plan refused, not rendered: {e}");
                                    app::l0_card::quiet_card()
                                }
                            ))
                        }
                    }
                }
                _ => {
                    let card = include_str!("../../../docs/weather-styles/style-glass.splash");
                    log::info!("seed card: {} bytes of splash", card.len());
                    Some(format!("```runsplash\n{card}\n```"))
                }
            };
            if let Some(text) = text {
                let mut data = CHAT_DATA.write().unwrap();
                data.messages.push(ChatMessage {
                    role: ChatRole::Assistant,
                    text,
                });
                // Seed the nav card's search state the same way `route_to_app`
                // does, so it opens on the A->B preview. Seeding STATE (not the
                // card text) keeps the in-card search boxes live for re-search.
                if let Some((dest, orig)) = self.seed_nav_state.take() {
                    let item_id = data.messages.len() - 1;
                    let st = data.a2app_state.entry(item_id).or_default();
                    st.insert("q".to_string(), dest);
                    st.insert("sel".to_string(), "1".to_string());
                    if let Some(o) = orig {
                        st.insert("oq".to_string(), o);
                    }
                }
                data.is_streaming = false;
            }
            self.update_empty_state_visibility(cx);
            cx.redraw_all();
        }
        // Central drain for async image decodes: guarantee every decoded image
        // buffer lands in the global ImageCache even when NO Image widget
        // catches the one-shot AsyncImageLoad action (a Splash card evals twice
        // — streaming then pooled — and the instance that spawned the decode
        // may be gone when the result posts; the first image of a fresh process
        // also pays decode-pool cold-start, widening that window). Widgets then
        // adopt the texture from the cache via the draw_walk self-heal. Taking
        // the result here is safe: any widget that sees the action afterwards
        // finds it already taken (no-op) and loads from the cache instead.
        if let Event::Actions(actions) = event {
            use makepad_widgets::makepad_draw::{process_async_image_load, AsyncImageLoad};
            for action in actions {
                if let Some(AsyncImageLoad { image_path, result }) = action.downcast_ref() {
                    if let Some(result) = result.borrow_mut().take() {
                        process_async_image_load(cx, image_path, result);
                        cx.redraw_all();
                    }
                }
            }
        }
        // Composer folds the moment its soft keyboard is dismissed. The
        // composer is "unfolded" ONLY while actively being typed into, so
        // dismissing the keyboard (BACK, or the IME "down" chevron) tucks the
        // pill back to the "+" FAB — otherwise unfolding via "+" and then
        // hiding the keyboard left the pill stuck open, covering the card.
        // Submit already folds first, so this is a harmless no-op there.
        // Mobile only (desktop has no soft keyboard / uses the reveal pill).
        // OHOS reports the same VirtualKeyboard events, so it needs this too —
        // without it, dismissing the keyboard there leaves the composer pill
        // expanded over the card.
        #[cfg(mobile)]
        if let Event::VirtualKeyboard(
            makepad_widgets::makepad_platform::event::VirtualKeyboardEvent::DidHide { .. },
        ) = event
        {
            if self.composer_shown {
                self.composer_shown = false;
                self.sync_composer(cx);
            }
        }
        // Streaming repaint tick — see `stream_tick` field docs.
        if self.stream_tick.is_event(event).is_some() {
            if self.stream_dirty {
                self.stream_dirty = false;
                cx.redraw_all();
            } else if !CHAT_DATA.read().map(|d| d.is_streaming).unwrap_or(false) {
                // Turn finished and nothing pending — park the interval.
                cx.stop_timer(self.stream_tick);
                self.stream_tick = Timer::empty();
            }
        }
        // Post-card repaint burst: draw for ~5.6s so a remote background image
        // adopts its texture (Image::draw_walk self-heals from the cache) once
        // its fetch+decode settle, then park.
        if self.settle_timer.is_event(event).is_some() {
            self.settle_ticks += 1;
            cx.redraw_all();
            if self.settle_ticks >= 16 {
                cx.stop_timer(self.settle_timer);
                self.settle_timer = Timer::empty();
            }
        }
        // Toast auto-dismiss: pop the shown toast and advance to the next.
        if self.l0_typing_rebuild.is_event(event).is_some() {
            // The coalesced rebuild for everything the keystrokes changed.
            L0_TYPING_PENDING.store(false, std::sync::atomic::Ordering::Relaxed);
            CHAT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            cx.redraw_all();
        }
        if self.toast_timer.is_event(event).is_some() {
            self.toast_timer = Timer::empty();
            if let Ok(mut state) = APP_STATE.write() {
                octos_app_store::state::reduce(
                    &mut state,
                    octos_app_store::state::Event::DismissOldestToast,
                );
            }
            self.sync_toasts(cx);
        }
        // Android: window size may be unknown during handle_startup, so
        // re-apply the phone-boot sidebar collapse once the first real
        // layout exists.
        if let Event::Draw(_) = event {
            static FIRST_DRAW: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(true);
            if FIRST_DRAW.swap(false, std::sync::atomic::Ordering::Relaxed) {
                self.collapse_sidebar_if_narrow(cx);
            }
        }
        if let Event::WindowDragQuery(dq) = event {
            if Some(dq.window_id) == self.ui.window(cx, ids!(main_window)).window_id() {
                let size = self.ui.window(cx, ids!(main_window)).get_inner_size(cx);
                if should_start_window_drag(dq.abs, size) {
                    dq.response.set(WindowDragQueryResponse::Caption);
                    cx.set_cursor(MouseCursor::Default);
                }
            }
        }

        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());

        // Transport wake-ups arrive as signals; refresh the top-bar
        // connection dot/label from APP_STATE so Live/Reconnecting/Offline
        // tracks reality instead of the boot snapshot.
        if let Event::Signal = event {
            // Dev-goal findings: one at a time, only between turns.
            if self.dev_prompt.is_none() {
                if let Some(dev) = self.dev_session {
                    let next = DEV_FINDINGS.lock().ok().and_then(|mut q| {
                        if q.is_empty() { None } else { Some(q.remove(0)) }
                    });
                    if let Some(findings) = next {
                        if let Some(agent) = self.agent.as_mut() {
                            let pid = agent.send_prompt(cx, dev, &findings);
                            self.dev_prompt = Some(pid);
                            self.dev_round += 1;
                            log::info!("[devgoal] round {} started (findings {} bytes)", self.dev_round, findings.len());
                        }
                    }
                }
            }
            self.update_connection_indicator(cx);
        self.update_context_indicator(cx);
            // Streaming state flips on transport events — keep the octopus
            // (and empty-state) in sync even when no widget action fired.
            self.update_empty_state_visibility(cx);
            // Re-assert the Profile pill: a set_labels issued during
            // handle_startup can land on a not-yet-ready widget ref and
            // silently no-op, leaving the "(no profile)" stub on screen.
            if let Some((_, label)) = self.available_profiles.first() {
                let dd = self.ui.drop_down(cx, ids!(backend_dropdown));
                if &dd.selected_label() != label {
                    dd.set_labels(cx, vec![label.clone()]);
                    dd.set_selected_item(cx, 0);
                    dd.redraw(cx);
                }
            }
        }

        if let Some(agent) = &mut self.agent {
            for event in agent.handle_event(cx, event) {
                match event {
                    AgentEvent::SessionReady { .. } => {
                        self.update_status(cx);
                    }
                    AgentEvent::SessionError { error, .. } => {
                        self.ui
                            .label(cx, ids!(status_label))
                            .set_text(cx, &format!("Error: {}", error));
                    }
                    AgentEvent::TextAuthoritative { prompt_id, text } => {
                        // Same guards as TextDelta: the AMA's stream is routing
                        // metadata and a background app must not touch the
                        // shared surface.
                        if Some(prompt_id) == self.cancelled_ama
                            || Some(prompt_id) == self.ama_prompt
                        {
                            continue;
                        }
                        if let Some(i) = self.app_of_prompt(prompt_id) {
                            if i != self.foreground {
                                continue;
                            }
                        }
                        CHAT_DATA.write().unwrap().authoritative_text = text;
                    }
                    AgentEvent::TextDelta { prompt_id, text } => {
                        // Dev-goal stream: collected for the file bridge, never
                        // rendered — same discipline as the AMA's routing text.
                        if Some(prompt_id) == self.dev_prompt {
                            self.dev_text.push_str(&text);
                            continue;
                        }
                        // A cancelled AMA turn's late deltas are stale routing
                        // metadata — drop them (they would otherwise fall past
                        // the AMA/foreground guards and stream as card text).
                        if Some(prompt_id) == self.cancelled_ama {
                            continue;
                        }
                        // AMA MVP: the AMA's stream is routing metadata — collect
                        // it for the log, never render it to the screen.
                        if Some(prompt_id) == self.ama_prompt {
                            self.ama_text.push_str(&text);
                            continue;
                        }
                        // Layer 3 foreground guard: a delta for a BACKGROUND app
                        // must not stream into the shared CHAT_DATA — badge it
                        // and skip. Orphan prompts (None) fall through as the
                        // pre-Layer-3 single-app behavior.
                        if let Some(i) = self.app_of_prompt(prompt_id) {
                            if i != self.foreground {
                                self.apps[i].has_updates = true;
                                self.tabs_dirty = true;
                                continue;
                            }
                        }
                        // Perf: tokens arrive far faster than 60 fps, and the
                        // draw path re-parses the whole accumulated reply —
                        // so only accumulate here and let the ~10 Hz
                        // `stream_tick` drive redraws (first delta of a burst
                        // paints immediately).
                        {
                            let card_owner = self.app_of_prompt(prompt_id);
                            let card_domain =
                                card_owner.and_then(|i| self.apps[i].domain.clone());
                            let card_session = card_owner
                                .map(|i| format!("{:?}", self.apps[i].session_id));
                            let mut data = CHAT_DATA.write().unwrap();
                            data.streaming_text.push_str(&text);
                            // A completed card fence is a renderable artifact —
                            // persist it NOW so a stalled/cancelled turn still
                            // leaves a traceable save.
                            save_completed_stream_cards(
                                &mut data,
                                card_domain,
                                card_session,
                            );
                        }
                        self.stream_dirty = true;
                        if self.stream_tick.is_empty() {
                            self.stream_tick = cx.start_interval(0.1);
                            self.stream_dirty = false;
                            cx.redraw_all();
                        }
                    }
                    AgentEvent::ThinkingDelta { prompt_id, text } => {
                        if Some(prompt_id) == self.ama_prompt {
                            continue;
                        }
                        // Foreground guard (see TextDelta).
                        if let Some(i) = self.app_of_prompt(prompt_id) {
                            if i != self.foreground {
                                self.apps[i].has_updates = true;
                                self.tabs_dirty = true;
                                continue;
                            }
                        }
                        let first = {
                            let mut data = CHAT_DATA.write().unwrap();
                            let first = data.thinking_text.is_empty();
                            data.thinking_text.push_str(&text);
                            first
                        };
                        if first {
                            self.ui
                                .label(cx, ids!(status_label))
                                .set_text(cx, "Thinking...");
                        }
                        self.stream_dirty = true;
                        if self.stream_tick.is_empty() {
                            self.stream_tick = cx.start_interval(0.1);
                            self.stream_dirty = false;
                            cx.redraw_all();
                        }
                    }
                    AgentEvent::TurnComplete { prompt_id, .. } => {
                        if Some(prompt_id) == self.dev_prompt {
                            let text = std::mem::take(&mut self.dev_text);
                            self.dev_prompt = None;
                            let card = text
                                .split("BEGIN_CARD")
                                .nth(1)
                                .and_then(|t| t.split("END_CARD").next())
                                .map(str::trim)
                                .unwrap_or("");
                            let done = text.contains("DONE");
                            if card.is_empty() {
                                log::warn!("[devgoal] round {} ended with NO card between markers", self.dev_round);
                            } else {
                                match std::fs::write("/data/local/tmp/dev_card.splash", card) {
                                    Ok(()) => log::info!(
                                        "[devgoal] round {} card written ({} bytes) done={}",
                                        self.dev_round, card.len(), done
                                    ),
                                    Err(e) => log::warn!("[devgoal] card write failed: {e}"),
                                }
                            }
                            // Narration outside the markers is the model's own
                            // report — surface the head of it for the log.
                            let head: String = text.chars().take(300).collect();
                            log::info!("[devgoal] narration: {}", head.replace('\n', " | "));
                            continue;
                        }
                        // A cancelled AMA turn finally completed — swallow it
                        // (its decision is void; the intent was already released
                        // by Cancel). Clear the marker so its slot is reusable.
                        if Some(prompt_id) == self.cancelled_ama {
                            self.cancelled_ama = None;
                            continue;
                        }
                        // AMA MVP: the AMA's turn finished — parse + apply its
                        // routing decision (proves the routing brain ran
                        // concurrently with the app agent), render nothing.
                        if Some(prompt_id) == self.ama_prompt {
                            // The DECISION is the AMA's FINAL non-empty line: a
                            // composing turn legitimately narrates its file
                            // writes first, and glm sometimes thinks aloud —
                            // parsing the first token of the whole text once
                            // spawned an agent literally named "this". The
                            // prompt contract says the decision line comes
                            // last; hold it to that.
                            // Parse the decision robustly. The contract is a
                            // final `<appid> — <reason>` (or `none`, or
                            // `compose <id> — <reason>`), but the model often
                            // narrates first and runs the decision onto the SAME
                            // line without a newline, so a line-based heuristic
                            // grabs a narration word ("let"). Anchor on the
                            // em-dash separator instead: the app id is the last
                            // token before the LAST `—`, and it's a compose if
                            // the token before THAT is "compose".
                            let decision = self.ama_text.trim().to_string();
                            let (is_compose, app_id) = Self::parse_ama_decision(&decision);
                            self.ama_prompt = None;
                            // Dynamic composition: the AMA matched NO existing app
                            // and has just authored the new app's spec into the
                            // memory tree — spin up a NEW peer agent session for
                            // that id (a fresh session gets the updated memory
                            // injected on open) and route the still-held intent.
                            if is_compose {
                                if app_id.is_empty() {
                                    // Malformed compose line — release the intent
                                    // via the no-match path.
                                    self.route_to_app(cx, "none", &decision);
                                } else {
                                    self.compose_app(cx, &app_id, &decision);
                                }
                                continue;
                            }
                            // decision → activation: hand the held intent to the app
                            // agent whose domain matches, foreground it, and let it
                            // generate its card. Domains WITHOUT a boot-time agent
                            // (tree-declared apps like "activity"/"weather-activity",
                            // or a previously composed app after a restart) go through
                            // compose_app, which creates the peer session on demand
                            // and then routes — same fresh-injection guarantee as an
                            // explicit `compose` decision.
                            let known = self
                                .apps
                                .iter()
                                .any(|a| a.domain.as_deref() == Some(app_id.as_str()));
                            if !known
                                && app_id != "none"
                                && !app_id.is_empty()
                                && app_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                            {
                                self.compose_app(cx, &app_id, &decision);
                                continue;
                            }
                            self.route_to_app(cx, &app_id, &decision);
                            continue;
                        }
                        // Foreground guard: a BACKGROUND app finishing must not
                        // steal the foreground's streaming_text or render into
                        // CHAT_DATA. Clear that app's prompt, badge it, skip —
                        // its card is on the server ledger and hydrates on switch.
                        let prompt_owner = self.app_of_prompt(prompt_id);
                        if let Some(i) = prompt_owner {
                            if i != self.foreground {
                                self.apps[i].current_prompt = None;
                                self.apps[i].has_updates = true;
                                self.tabs_dirty = true;
                                continue;
                            }
                        }
                        // Set when the completed card fails its app's shipped
                        // lint rules; fired as ONE repair turn after the message
                        // is stored (the corrected card streams in over it).
                        let mut card_repair: Option<String> = None;
                        // Which budget the pending repair draws from: an L0
                        // CHECKER refusal spends `l0_repair_attempts` (up to
                        // L0_REPAIR_BUDGET); lint/security spend the one-shot
                        // `repair_attempted` as before.
                        let mut l0_refusal_repair = false;
                        let mut data = CHAT_DATA.write().unwrap();
                        let streamed = std::mem::take(&mut data.streaming_text);
                        let authoritative = std::mem::take(&mut data.authoritative_text);
                        // Prefer what the kernel durably stored. The deltas we
                        // accumulated are a best-effort mirror of it; one lost
                        // in transit leaves `streamed` short by that chunk with
                        // its edges spliced together mid-token, which silently
                        // turns a valid card DSL into one that cannot parse.
                        let text = if authoritative.is_empty() {
                            streamed
                        } else {
                            if authoritative != streamed {
                                log!(
                                    "aichat UI stream/persisted MISMATCH — streamed={} persisted={} chars; \
                                     using persisted (a delta was lost or reordered)",
                                    streamed.chars().count(),
                                    authoritative.chars().count()
                                );
                            }
                            authoritative
                        };
                        log!(
                            "aichat UI turn complete content_chars={}",
                            text.chars().count()
                        );
                        data.thinking_text.clear();
                        let mut rendered_card = false;
                        if !text.is_empty() {
                            if assistant_message_is_safe_to_store(&text) {
                                // Card-archive context: which app produced the
                                // card and the intent that triggered it — saved
                                // alongside the card for traceability.
                                let card_domain =
                                    prompt_owner.and_then(|i| self.apps[i].domain.clone());
                                let card_session = prompt_owner
                                    .map(|i| format!("{:?}", self.apps[i].session_id));
                                let card_prompt = data
                                    .messages
                                    .iter()
                                    .rev()
                                    .find(|m| m.role == ChatRole::User)
                                    .map(|m| m.text.clone());
                                // Persist a named A2App card so it can be
                                // retrieved by name and refined over time.
                                if let Some(body) = card_splash_body(&text) {
                                    let body: &str = &body;
                                    rendered_card = true;
                                    // Never PERSIST a forbidden card (it would be
                                    // reused by name later); repair it instead.
                                    // Scan ALL blocks, not just `body` (first):
                                    // a safe first + unsafe second must also trip.
                                    let forbidden = extract_all_runsplash_bodies(&text)
                                        .into_iter()
                                        .find_map(runsplash_body_forbidden);
                                    if let Some(reason) = forbidden {
                                        log::warn!("a2app: refusing to save unsafe card: {reason}");
                                        if prompt_owner
                                            .map(|i| !self.apps[i].repair_attempted)
                                            .unwrap_or(false)
                                        {
                                            card_repair = Some(format!(
                                                "SECURITY: your card was rejected — {reason}. \
                                                 Re-emit the card using ONLY sys.* helpers and \
                                                 http_resource for images; remove all \
                                                 net.http_request usage."
                                            ));
                                        }
                                        // fall past save/lint for this body
                                    } else {
                                    // DEBUG: dump the generated DSL in chunks.
                                    for (i, chunk) in body.as_bytes().chunks(600).enumerate() {
                                        log::info!("CARDDSL[{i}]{}", String::from_utf8_lossy(chunk));
                                    }
                                    // The plan this card was lowered from, when the model
                                    // emitted one. `None` for a hand-written runsplash
                                    // card, which simply has no plain-data sibling.
                                    let last_plan = extract_runplan_body(&text).map(str::to_string);
                                    match extract_card_name(body) {
                                        Some(name) => save_card_artifact(
                                            &name,
                                            "runsplash",
                                            body,
                                            card_domain.as_deref(),
                                            card_prompt.as_deref(),
                                            card_session.as_deref(),
                                            last_plan.as_deref(),
                                        ),
                                        None => log::warn!(
                                            "a2app: runsplash card has no `// name:` line — not saved"
                                        ),
                                    }
                                    // Machine-check the card against the rules
                                    // of the app that OWNS this prompt — never
                                    // the foreground app's lint.json, which may
                                    // belong to a different domain (a stock
                                    // card must not be checked by weather
                                    // rules). Orphan prompts (no owner) skip
                                    // lint rather than guess.
                                    if let Some(owner_idx) = prompt_owner {
                                        if !self.apps[owner_idx].repair_attempted {
                                            if let Some(domain) =
                                                self.apps[owner_idx].domain.clone()
                                            {
                                                if let Some(rules) =
                                                    crate::app::card_lint::load_rules(&domain)
                                                {
                                                    let violations =
                                                        crate::app::card_lint::lint(body, &rules);
                                                    if !violations.is_empty() {
                                                        log::warn!(
                                                            "card lint ({domain}): {} violation(s): {}",
                                                            violations.len(),
                                                            violations.join(" | ")
                                                        );
                                                        card_repair = Some(
                                                            crate::app::card_lint::repair_prompt(
                                                                &violations,
                                                            ),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    } // end else (safe card path)
                                }
                                // An L0 card is checked HERE, where a repair turn
                                // is still possible.
                                //
                                // `card_lint` only sees a `runsplash` body, and an
                                // L0 ledger is a different fence — so a refused L0
                                // card fell through to render time, where
                                // `resolve_l0_blocks` draws the diagnostics and
                                // there is no way back to the model. The first
                                // live GLM-5.2 card died exactly there: it read
                                // `quote.volume` without listing `volume` in the
                                // source's `fields:`, and the user got an error
                                // card instead of a second attempt.
                                //
                                // The checker's own message is the repair prompt.
                                // It already names the line, the offending field
                                // and the alternatives, which is more than a
                                // hand-written instruction would say.
                                if card_repair.is_none() {
                                    if let Some(owner_idx) = prompt_owner {
                                        if self.apps[owner_idx].l0_repair_attempts
                                            < L0_REPAIR_BUDGET
                                        {
                                            let mut why: Vec<String> = Vec::new();
                                            for piece in app::l0_card::split_l0_blocks(&text) {
                                                if let app::l0_card::Piece::Ledger(src) = piece {
                                                    let report =
                                                        splash_ui_l0::check_ui_l0_named("card", src);
                                                    if !report.valid {
                                                        why.extend(
                                                            report.diagnostics.iter().map(|d| {
                                                                format!(
                                                                    "line {}: {}",
                                                                    d.line, d.message
                                                                )
                                                            }),
                                                        );
                                                    }
                                                    // A VALID card can still be too
                                                    // wide for the app that asked for
                                                    // it. See `l0_level_refusal`.
                                                    if let Some(domain) =
                                                        self.apps[owner_idx].domain.as_deref()
                                                    {
                                                        why.extend(l0_level_refusal(
                                                            domain, &report,
                                                        ));
                                                    }
                                                }
                                            }
                                            if !why.is_empty() {
                                                rendered_card = true;
                                                log::warn!(
                                                    "L0 card refused (attempt {}/{}, {} diagnostic(s)): {}",
                                                    self.apps[owner_idx].l0_repair_attempts + 1,
                                                    L0_REPAIR_BUDGET,
                                                    why.len(),
                                                    why.join(" | ")
                                                );
                                                // The REQUEST rides along. Without it the
                                                // model re-emits something closer to the
                                                // exemplar, whose weather card declares
                                                // `city: ""` — device location — so a
                                                // repaired card answered the wrong place
                                                // while reporting success (measured: five
                                                // refusals for "weather in osaka", repaired,
                                                // rendered San Jose).
                                                let asked = self.apps[owner_idx]
                                                    .last_request
                                                    .as_deref()
                                                    .unwrap_or("");
                                                let restate = if asked.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!(
                                                        "\n\nThe card must still answer THIS \
                                                         request, with the same places, tickers \
                                                         and options it named: {asked:?}"
                                                    )
                                                };
                                                card_repair = Some(format!(
                                                    "Your L0 card was REFUSED by the checker. Fix \
                                                     exactly these and re-emit the whole card in \
                                                     one ```runl0 block — no prose, no other \
                                                     fenced blocks:\n- {}{restate}",
                                                    why.join("\n- ")
                                                ));
                                                l0_refusal_repair = true;
                                            }
                                        }
                                    }
                                }
                                // Webview (runhtml) cards get the same archive
                                // treatment — previously they were ephemeral.
                                if let Some(html) = extract_runhtml_body(&text) {
                                    match extract_html_card_name(html) {
                                        Some(name) => save_card_artifact(
                                            &name,
                                            "runhtml",
                                            html,
                                            card_domain.as_deref(),
                                            card_prompt.as_deref(),
                                            card_session.as_deref(),
                                            None,
                                        ),
                                        None => log::warn!(
                                            "a2app: runhtml card has no `<!-- name: -->` — not saved"
                                        ),
                                    }
                                }
                                // Store the NEUTRALIZED text: a forbidden fence
                                // must not survive in history to be re-rendered
                                // by the completed-message path or a session
                                // hydrate (which don't run the streaming gate).
                                let materialized = materialize_runplan_for_display(&text);
                                let stored = neutralize_forbidden_cards(&materialized).into_owned();
                                data.messages.push(ChatMessage {
                                    role: ChatRole::Assistant,
                                    text: stored,
                                });
                            } else {
                                self.ui.label(cx, ids!(status_label)).set_text(
                                    cx,
                                    "Error: incomplete diagram response discarded; retry",
                                );
                            }
                        }
                        data.is_streaming = false;
                        data.save_to_disk();
                        drop(data);

                        self.set_fg_prompt(None);
                        self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
                        // One-shot repair pass: the completed card violated its
                        // app's machine-checkable rules. Send the violation list
                        // back to the SAME app agent session; the corrected card
                        // streams in over the visible (imperfect) one.
                        if let Some(repair) = card_repair.take() {
                            // card_repair is only set when the prompt's owner
                            // was identified — route the repair back to THAT
                            // app, not whatever happens to be foreground.
                            let i = prompt_owner.unwrap_or(self.foreground);
                            let sid = self.apps[i].session_id;
                            let pid = self.agent.as_mut().unwrap().send_prompt(cx, sid, &repair);
                            self.apps[i].current_prompt = Some(pid);
                            // Draw down the budget the repair belongs to: an
                            // L0 refusal counts against L0_REPAIR_BUDGET and
                            // leaves the lint/security one-shot untouched, so
                            // a later lint miss on the repaired card still
                            // gets its single turn.
                            if l0_refusal_repair {
                                self.apps[i].l0_repair_attempts =
                                    self.apps[i].l0_repair_attempts.saturating_add(1);
                            } else {
                                self.apps[i].repair_attempted = true;
                            }
                            self.set_fg_prompt(Some(pid));
                            CHAT_DATA.write().unwrap().is_streaming = true;
                            self.ui.label(cx, ids!(status_label)).set_text(
                                cx,
                                "Card failed validation — auto-repairing…",
                            );
                        }
                        self.update_empty_state_visibility(cx);
                        // A card just rendered — collapse the floating composer to
                        // the reveal pill so the card gets the full screen.
                        if rendered_card {
                            self.composer_shown = false;
                        }
                        self.sync_composer(cx);
                        // Clear the transient "Thinking..." status back to the
                        // idle connection line (it was set by ThinkingDelta and
                        // otherwise stuck after the reply landed).
                        self.update_status(cx);
                        cx.redraw_all();
                        // A full-screen card just rendered: scroll it into view
                        // (the redraw_all above can reset the list to the top).
                        if rendered_card {
                            let count = { CHAT_DATA.read().unwrap().messages.len() };
                            let list = self
                                .ui
                                .widget(cx, ids!(chat_list))
                                .portal_list(cx, ids!(list));
                            list.set_tail_range(true);
                            list.set_first_id_and_scroll(count.saturating_sub(1), 0.0);
                            // Repaint burst so the card's background image adopts
                            // its decoded texture once the fetch+decode settle.
                            self.settle_ticks = 0;
                            self.settle_timer = cx.start_interval(0.35);
                        }
                    }
                    AgentEvent::PromptError { prompt_id, error } => {
                        if Some(prompt_id) == self.ama_prompt {
                            log::warn!("AMA turn error: {error} — falling back to weather");
                            self.ama_prompt = None;
                            // Don't strand the held intent: route to a default.
                            self.route_to_app(cx, "weather", "AMA error fallback");
                            continue;
                        }
                        // Foreground guard: a BACKGROUND app's error clears its
                        // prompt + badges it; it must not write CHAT_DATA.
                        if let Some(i) = self.app_of_prompt(prompt_id) {
                            if i != self.foreground {
                                self.apps[i].current_prompt = None;
                                self.apps[i].has_updates = true;
                                self.tabs_dirty = true;
                                continue;
                            }
                        }
                        log!("aichat UI prompt error: {}", error);
                        {
                            let mut data = CHAT_DATA.write().unwrap();
                            data.messages.push(ChatMessage {
                                role: ChatRole::Assistant,
                                text: format!("Error: {error}"),
                            });
                            data.is_streaming = false;
                            data.thinking_text.clear();
                            data.save_to_disk();
                        }
                        self.set_fg_prompt(None);
                        self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
                        self.update_empty_state_visibility(cx);
                        self.ui
                            .label(cx, ids!(status_label))
                            .set_text(cx, &format!("Error: {}", error));
                        cx.redraw_all();
                    }
                    AgentEvent::ToolRequest { .. } => {}
                    AgentEvent::TextAuthoritative { .. } => {}
                    _ => {}
                }
            }
        }

        // Layer 3 — flush any switcher badge/title changes accumulated during
        // this drain (batched so background streaming doesn't re-sync per delta).
        if self.tabs_dirty {
            self.tabs_dirty = false;
            self.sync_app_tabs(cx);
        }
        // W04 follow-up #3 — refresh the top-bar connection indicator each
        // tick. `OctosUiAgent` mirrors transport `ConnectionState` into
        // `APP_STATE.connection`; reading it here keeps the dot in sync
        // without a separate signal/post_action.
        self.update_connection_indicator(cx);
        self.update_context_indicator(cx);
        // Show any toasts queued by the store during this drain (compaction,
        // memory-saved, warnings).
        self.sync_toasts(cx);
        // Keep the swimming-octopus row in lockstep with `is_streaming`
        // (flips inside the agent drain above — actions, not signals).
        self.update_empty_state_visibility(cx);
    }
}

#[cfg(test)]
mod tests {
    use makepad_widgets::DVec2;

    use super::{
        app_card_docs, assistant_message_is_safe_for_history,
        assistant_message_is_safe_to_store, baked_app_md, baked_widget_md, card_root_height,
        embeddable_card, expand_card_embeds, extract_nav_destination, matching_brace,
        defer_unclosed_runplan, materialize_runplan_for_display, namespace_child_state,
        parse_nav_places, glass_opacity_values, pin_fullbleed_root_height,
        rewrite_child_emits, should_start_window_drag, splash_gen_prompt, substitute_card_state,
        substitute_props, EmitHandler, DEFAULT_GLASS_OPACITY,
        FULLBLEED_FALLBACK_HEIGHT, MAX_GLASS_OPACITY, MIN_GLASS_OPACITY, NAV_CANONICAL_CARD,
    };
    use std::collections::BTreeMap;

    // ── Phase 1: Card{} composition ─────────────────────────────────────────
    fn expand(body: &str) -> String {
        let mut inst = 0u32;
        expand_card_embeds(body, 0, &mut inst)
    }

    #[test]
    fn matching_brace_is_string_and_double_brace_aware() {
        // Braces inside "…" and the {{…}} token delimiters must NOT unbalance.
        let s = r#"Card{ props: { dest: "{{state.drop}}", note: "a}b{c" } }"#;
        let open = s.find('{').unwrap();
        let end = matching_brace(s, open).unwrap();
        // The matched span is the whole Card body incl. the nested props block.
        assert_eq!(s.as_bytes()[end], b'}');
        assert_eq!(&s[end..], "}");
    }

    #[test]
    fn substitute_props_binds_and_defaults_unset_to_zero() {
        let mut p = BTreeMap::new();
        p.insert("mode".to_string(), "drive".to_string());
        // bound prop resolves; unbound prop -> "0" (the cards' unset sentinel)
        assert_eq!(
            substitute_props("m={{props.mode}} d={{props.dest}}", &p),
            "m=drive d=0"
        );
    }

    #[test]
    fn props_can_carry_a_parent_state_ref_through() {
        let mut p = BTreeMap::new();
        p.insert("dest".to_string(), "{{state.drop}}".to_string());
        // a prop value that is itself a parent {{state.x}} passes through verbatim,
        // to be resolved later by the parent's state substitution.
        assert_eq!(substitute_props("x={{props.dest}}", &p), "x={{state.drop}}");
    }

    #[test]
    fn namespace_child_state_prefixes_reads_and_writes() {
        let body = r#"let v = "{{state.view}}"  ... agent.notify("set", {key: "view", value: "2d"})"#;
        let out = namespace_child_state(body, 2);
        assert!(out.contains("{{state._c2_view}}"), "read: {out}");
        assert!(out.contains(r#"{key: "_c2_view""#), "write: {out}");
    }

    #[test]
    fn rewrite_emit_maps_to_parent_key_with_literal_value() {
        let mut on = BTreeMap::new();
        on.insert(
            "end".to_string(),
            EmitHandler { key: "nav_end".to_string(), value: Some("1".to_string()) },
        );
        let out = rewrite_child_emits(r#"agent.notify("emit", {event: "end"})"#, 0, &on);
        assert_eq!(out, r#"agent.notify("set", {key: "nav_end", value: "1"})"#);
    }

    #[test]
    fn rewrite_emit_passes_through_emitted_value_when_handler_omits_it() {
        // nav.picker pattern: on:{ pick:{key:"pickup"} } stores the emitted place.
        let mut on = BTreeMap::new();
        on.insert("pick".to_string(), EmitHandler { key: "pickup".to_string(), value: None });
        let out = rewrite_child_emits(
            r#"agent.notify("emit", {event: "pick", value: "37.3,-121.9|Napa"})"#,
            0,
            &on,
        );
        assert!(out.contains(r#"key: "pickup""#), "{out}");
        // comma inside the quoted place value must survive top-level comma split
        assert!(out.contains(r#"value: "37.3,-121.9|Napa""#), "{out}");
    }

    #[test]
    fn unknown_card_use_becomes_inert_placeholder_not_crash() {
        let out = expand(r#"Card{ use: "nav.teleporter" props: {} }"#);
        assert!(!out.contains("Card{"), "embed not consumed: {out}");
        assert!(out.contains("unknown card: nav.teleporter"), "{out}");
    }

    #[test]
    fn word_prefixed_card_ident_is_not_an_embed() {
        // `MyCard{` / `ScoreCard{` must not be mistaken for a Card{} embed.
        let src = "ScoreCard{ width: Fill }";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn expand_navigate_embed_end_to_end() {
        // The real standalone host body (mirrors cards/navigate-standalone.splash).
        let host = r#"Card{ use: "nav.navigate"
            props: { dest: "{{state.dest}}", origin: "{{state.orig}}", mode: "drive" }
            on: { end: { key: "nav_end", value: "1" } } }"#;
        let out = expand(host);
        // (1) the embed is fully expanded — no Card{} and no unresolved props remain
        assert!(!out.contains("Card{"), "Card{{}} not expanded");
        assert!(!out.contains("{{props."), "unresolved props: {out}");
        // (2) the required MapView drive body is inlined
        assert!(out.contains("MapView{"), "map missing");
        assert!(out.contains(r#"nav_mode: "3d""#), "3d map missing");
        // (3) a required-prop binding became a PARENT state ref (un-namespaced)
        assert!(out.contains("{{state.dest}}"), "dest parent ref missing: {out}");
        // (4) the child's INTERNAL `view` state is namespaced to instance 0
        assert!(out.contains("{{state._c0_view}}"), "view not namespaced: {out}");
        assert!(!out.contains("{{state.view}}"), "view leaked un-namespaced");
        // (5) the End emit is wired to the parent nav_end key
        assert!(
            out.contains(r#"agent.notify("set", {key: "nav_end", value: "1"})"#),
            "end not wired: {out}"
        );
        assert!(!out.contains(r#""emit""#), "raw emit left in: {out}");
    }

    #[test]
    fn two_embeds_get_distinct_namespaces() {
        let host = r#"Card{ use:"nav.navigate" props:{dest:"a"} on:{end:{key:"e1"}} }
                     Card{ use:"nav.navigate" props:{dest:"b"} on:{end:{key:"e2"}} }"#;
        let out = expand(host);
        // instance 0 and instance 1 keep separate internal `view` state
        assert!(out.contains("{{state._c0_view}}"), "c0 view missing");
        assert!(out.contains("{{state._c1_view}}"), "c1 view missing");
    }

    #[test]
    fn registry_navigate_card_is_present_and_props_based() {
        let raw = embeddable_card("nav.navigate").expect("nav.navigate registered");
        // the component reads inputs via props and emits (not raw state/nav_end)
        assert!(raw.contains("{{props.dest}}"), "card not props-based");
        assert!(raw.contains(r#"agent.notify("emit", {event: "end"})"#), "no emit");
    }

    // ── Composition scenarios: through the FULL render pipeline ──────────────
    // These run `substitute_card_state` (embed expansion + state substitution +
    // notify tagging + all the safety rewrites), i.e. exactly what ships to the
    // Splash VM — with a real seeded CardState, as the AMA would seed on a route.

    /// A composed card is well-formed to ship: no unexpanded embeds, no unresolved
    /// tokens, balanced braces (an imbalance crashes the Splash eval).
    fn assert_shippable(out: &str) {
        assert!(!out.contains("Card{"), "unexpanded Card embed:\n{out}");
        assert!(!out.contains("{{props."), "unresolved prop token:\n{out}");
        assert!(!out.contains("{{state."), "unresolved state token:\n{out}");
        assert_eq!(
            out.matches('{').count(),
            out.matches('}').count(),
            "unbalanced braces:\n{out}"
        );
    }

    /// A LOWERED PLAN must be shippable through the same pipeline as a
    /// model-written card. This is the verification that does not need a device:
    /// `assert_shippable` catches the fatal class — an unbalanced brace crashes the
    /// Splash eval outright — plus unexpanded embeds and unresolved tokens.
    #[test]
    fn lowered_plan_is_shippable_through_the_full_pipeline() {
        let plan = r#"{
            "plan": "weather", "locale": "en",
            "place": { "query": "Kyoto" },
            "photo": "kyoto city cloudy sky",
            "sections": [
                { "block": "CurrentConditions" },
                { "block": "Forecast", "args": { "days": 7 } },
                { "block": "AirQualityField" },
                { "block": "SunMoon" },
                { "block": "Details", "args": { "tiles": ["aqi","uv","humidity","wind"] } }
            ]
        }"#;
        let card = crate::app::plan::lower_plan(plan).expect("plan must lower");
        let out = substitute_card_state(&card, 3, &BTreeMap::new());
        assert_shippable(&out);
        // And it must survive as a `runplan` fence in a message, which is how it
        // actually arrives.
        let msg = format!("```runplan\n{plan}\n```");
        let body = crate::card_splash_body(&msg).expect("runplan fence must lower");
        assert_shippable(&substitute_card_state(&body, 4, &BTreeMap::new()));
        assert!(body.contains("// name: weather-app"), "needs the name line to be saved");

        // The completed message shown to the user must contain the lowered card,
        // never the implementation-detail JSON fence that the model emitted.
        let display = materialize_runplan_for_display(&msg);
        assert!(display.contains("```runsplash\n"), "lowered card fence missing: {display}");
        assert!(!display.contains("```runplan"), "raw plan leaked into UI: {display}");
        assert!(display.contains("// name: weather-app"), "lowered body missing: {display}");
    }

    #[test]
    fn an_unclosed_runplan_is_hidden_while_streaming() {
        let partial = "before\n```runplan\n{\"plan\":\"news\"";
        let display = defer_unclosed_runplan(partial);
        assert_eq!(display, "before\n\u{1F6E0} Building app UI\u{2026}");
        assert!(!display.contains("\"plan\""), "partial plan JSON leaked into UI");
    }

    /// A rejected plan must yield a card that shows NOTHING — never diagnostics.
    ///
    /// This asserted the opposite: that the card said "This card was refused" and
    /// named the offending field. That is the compiler's opinion of source the
    /// person holding the phone did not write and cannot fix, and showing it to
    /// them was the wrong audience for the right information. The reasons go to
    /// the log, where whoever is building the generator reads them, and the
    /// repair path re-prompts the model with them.
    ///
    /// Still never a PARTIAL card — half a card looks complete and is not — and
    /// still never prose, which is indistinguishable from the model choosing to
    /// answer in words. An empty surface is what a card still arriving looks
    /// like, which is what this is.
    #[test]
    fn a_rejected_plan_shows_nothing_rather_than_diagnostics() {
        let bad = "```runplan\n{\"plan\":\"weather\",\"locale\":\"en\",\
            \"place\":{\"query\":\"Kyoto\",\"lat\":35.0},\"sections\":[]}\n```";
        // A rejected plan renders a DEFAULT ERROR CARD. The two states this
        // replaces were each wrong in one direction: an empty surface is
        // indistinguishable from "still loading", so a permanent failure read
        // as a hang; and dev's `None` dropped the message to ordinary prose, so
        // a rejected plan and a model that chose to answer in words looked
        // identical. It says something went wrong and NOT what — the reasons
        // are about generated source and are unusable by whoever is holding the
        // phone.
        let body = crate::card_splash_body(bad).expect("a refusal is still a card");
        assert!(
            !body.contains("refused") && !body.contains("lat"),
            "a refusal must not show its reasons:\n{body}"
        );
        assert!(!body.contains("Forecast"), "no fragment of the real card:\n{body}");
        assert!(body.contains("SolidView"), "it is still a card:\n{body}");
        assert!(
            body.contains("Couldn't build that"),
            "and it SAYS something went wrong:\n{body}"
        );
        // The plan itself is still rejected, and still not presented as a card
        // by the runplan materialiser (dev's half of the contract).
        assert!(
            matches!(materialize_runplan_for_display(bad), std::borrow::Cow::Borrowed(_)),
            "a rejected plan must not be materialised as a card"
        );
    }

    #[test]
    fn compose_navigate_to_x_full_pipeline() {
        // "navigate to NVIDIA": one-line host embeds nav.navigate, dest seeded
        // from card state (as parse_nav_places → state.dest would).
        let host = "// name: go\n\
            Card{ use: \"nav.navigate\" \
            props: { dest: \"{{state.dest}}\", mode: \"drive\" } \
            on: { end: { key: \"done\", value: \"1\" } } }";
        let mut st = BTreeMap::new();
        st.insert("dest".to_string(), "37.37,-121.96|NVIDIA".to_string());
        let out = substitute_card_state(host, 7, &st);
        assert_shippable(&out);
        // seeded dest flowed props → parent state → value
        assert!(out.contains("37.37,-121.96|NVIDIA"), "dest not wired:\n{out}");
        // the 3D drive map + live tick are inlined
        assert!(out.contains("MapView{"), "no map");
        assert!(out.contains("fn tick()"), "no tick");
        // End is wired to the parent `done` key, tagged with THIS card's id (7:)
        assert!(
            out.contains(r#"agent.notify("7:set", {key: "done", value: "1"})"#),
            "end not wired to parent:\n{out}"
        );
        // the child's internal 2D/3D toggle stays namespaced + tagged
        assert!(out.contains("7:set"), "notify not tagged");
    }

    #[test]
    fn compose_two_navigate_cards_resolve_state_independently() {
        // A 2-pane "compare routes" app: two nav.navigate embeds. Instance 0 is
        // put in 2D and instance 1 in 3D via their NAMESPACED view state — proving
        // the two embeds don't share internal state.
        let host = "Card{ use:\"nav.navigate\" props:{dest:\"{{state.a}}\"} on:{end:{key:\"x\"}} }\n\
                    Card{ use:\"nav.navigate\" props:{dest:\"{{state.b}}\"} on:{end:{key:\"y\"}} }";
        let mut st = BTreeMap::new();
        st.insert("a".to_string(), "1.0,1.0|Alpha".to_string());
        st.insert("b".to_string(), "2.0,2.0|Beta".to_string());
        st.insert("_c0_view".to_string(), "2d".to_string());
        st.insert("_c1_view".to_string(), "0".to_string());
        let out = substitute_card_state(host, 3, &st);
        assert_shippable(&out);
        // both destinations wired to their own instance
        assert!(out.contains("1.0,1.0|Alpha") && out.contains("2.0,2.0|Beta"), "dests");
        // per-instance view resolved independently: c0 → "2d", c1 → "0"
        assert!(out.contains(r#"let vw = "2d""#), "c0 view!=2d:\n{out}");
        assert!(out.contains(r#"let vw = "0""#), "c1 view!=0");
    }

    #[test]
    fn compose_shipped_standalone_host_end_to_end() {
        // The ACTUAL shipped host file — exactly what a standalone/on-device test
        // would serve — composes to a shippable card with the dest seeded.
        let host = include_str!("../../../a2app/apps/nav/cards/navigate-standalone.splash");
        let mut st = BTreeMap::new();
        st.insert("dest".to_string(), "38.58,-121.49|Sacramento".to_string());
        let out = substitute_card_state(host, 1, &st);
        assert_shippable(&out);
        assert!(out.contains("fn tick()"), "drive tick present");
        assert!(out.contains("38.58,-121.49|Sacramento"), "dest wired");
        // End wired back to the host's nav_end key
        assert!(out.contains(r#"key: "nav_end""#), "end→nav_end not wired:\n{out}");
    }

    #[test]
    fn demo_dump_composed_navigate() {
        // Print the composed card so the composition is inspectable:
        //   cargo test --bin octos-app demo_dump_composed_navigate -- --nocapture
        let host = include_str!("../../../a2app/apps/nav/cards/navigate-standalone.splash");
        let mut st = BTreeMap::new();
        st.insert("dest".to_string(), "37.37,-121.96|NVIDIA".to_string());
        st.insert("orig".to_string(), "37.26,-122.03|Saratoga High School".to_string());
        let out = substitute_card_state(host, 0, &st);
        eprintln!(
            "\n===== COMPOSED navigate-standalone (dest+orig seeded, item_id 0) =====\n{out}\n===== {} bytes, braces {}/{} =====",
            out.len(),
            out.matches('{').count(),
            out.matches('}').count()
        );
    }

    #[test]
    fn aichat_glass_opacity_slider_contract() {
        // v2: slider is a position value; per-layer alpha is derived.
        assert!((DEFAULT_GLASS_OPACITY - 0.90).abs() < f64::EPSILON);
        let values = glass_opacity_values(DEFAULT_GLASS_OPACITY);
        // Layer stack must read shell < main < sidebar < composer
        // so the wallpaper shows through more on the outer frame than on
        // the inner panels.
        assert!(values.app < values.main);
        assert!(values.main < values.sidebar);
        assert!(values.sidebar < values.composer);
        // Default keeps the wallpaper visible, but is opaque enough for text.
        assert!((0.82..0.87).contains(&values.app));
    }

    #[test]
    fn aichat_liquid_glass_shell_contract() {
        // v2: layer-stack ordering must hold at every legal slider value,
        // and no layer reaches alpha 1.0 at any slider <= 1.0.
        let low = glass_opacity_values(0.0);
        let high = glass_opacity_values(2.0);
        // Slider is clamped: low.app uses MIN_GLASS_OPACITY, high.app uses MAX.
        assert!(low.app < high.app);
        assert!(high.app > 0.90);
        assert!(high.app <= 1.0);
        // Ordering preserved across the range.
        for &slider in &[
            MIN_GLASS_OPACITY,
            0.30_f64,
            0.60,
            DEFAULT_GLASS_OPACITY,
            MAX_GLASS_OPACITY,
        ] {
            let v = glass_opacity_values(slider);
            assert!(v.app < v.main, "slider={}", slider);
            assert!(v.main <= v.sidebar, "slider={}", slider);
            assert!(v.sidebar <= v.composer, "slider={}", slider);
        }
    }

    #[test]
    fn aichat_drag_strip_preserves_resize_edges() {
        let size = DVec2 { x: 900.0, y: 700.0 };
        assert!(should_start_window_drag(
            DVec2 { x: 120.0, y: 24.0 },
            size
        ));
        assert!(!should_start_window_drag(DVec2 { x: 4.0, y: 24.0 }, size));
        assert!(!should_start_window_drag(DVec2 { x: 120.0, y: 4.0 }, size));
        assert!(!should_start_window_drag(
            DVec2 { x: 880.0, y: 24.0 },
            size
        ));
        assert!(!should_start_window_drag(
            DVec2 { x: 700.0, y: 24.0 },
            size
        ));
    }

    #[test]
    fn an_l0_domain_gets_the_l0_memory_and_not_the_splash_manual() {
        // `activity` has an L0 spec, so its prompt must teach L0 — and must NOT
        // carry the Splash manual, which describes a language the card cannot
        // use. A prompt with both is worse than either: the model has two
        // grammars and no way to know which one is enforced.
        let p = splash_gen_prompt("activity", "things to do nearby", "");
        assert!(p.contains("things to do nearby"), "carries the user intent");
        assert!(p.contains("```runl0"), "demands one runl0 block");
        assert!(p.contains("===== LANGUAGE ====="), "inlines the language");
        assert!(p.contains("===== CATALOG ====="), "inlines the catalog");
        assert!(p.contains("source parks"), "inlines a card that meets the spec");
        assert!(
            !p.contains("SPLASH SYNTAX MANUAL"),
            "must not carry the manual for a language this card cannot write"
        );
        assert!(
            !p.contains("```runsplash"),
            "and must not ask for a Splash card"
        );
    }

    #[test]
    fn a_domain_without_an_l0_spec_still_gets_the_splash_path() {
        // What makes this switchable per app rather than a cutover: an app with
        // no L0 spec keeps the prompt it always had. If this ever fails, an app
        // lost its generation path rather than gaining one.
        let docs = "\n----- apps/web/app.md -----\nmandatory: one runhtml block\n";
        let p = splash_gen_prompt("web", "make me a todo app", docs);
        assert!(p.contains("SPLASH SYNTAX MANUAL"), "inlines the syntax manual");
        assert!(p.contains("```runsplash"), "demands one runsplash block");
        assert!(p.contains("Card {"), "still names the forbidden pseudo-DSL");
        assert!(!p.contains("```runl0"), "and does not ask for an L0 card");
    }

    /// Weather is an L0 domain now, so its prompt asks for a CARD in the L0
    /// language — not a plan, and not the Splash DSL.
    ///
    /// This asserted the opposite while `PLAN_DOMAINS` held weather. The gate
    /// moved because L0 could not answer `sys.weather`/`sys.geocode` live; with
    /// those translated it can, so the prompt follows. `lower_plan` still lowers
    /// a weather plan — the kind outlives the routing — and its own tests cover
    /// that shape.
    #[test]
    fn weather_now_asks_for_an_l0_card() {
        assert!(!crate::app::plan::domain_uses_plan("weather"), "weather is off the plan path");
        let docs = "\n----- apps/weather/plan.md — THIS IS YOUR SPEC -----\nblocks: Forecast\n";
        let p = splash_gen_prompt("weather", "weather in tokyo", docs);
        assert!(p.contains("weather in tokyo"), "carries the user intent");
        assert!(p.contains("```runl0"), "demands one runl0 block");
        assert!(!p.contains("```runplan"), "no longer asks for a plan");
        assert!(!p.contains("SPLASH SYNTAX MANUAL"), "no DSL manual on the L0 path");
        assert!(p.contains("REFUSED"), "tells the model an off-catalog name is refused");
    }

    /// A composed app resolves to a spec AND a worked exemplar, or to nothing.
    ///
    /// The exemplar is what makes an app take the L0 prompt at all, so a composed
    /// app without one silently generated a pre-L0 card. The fallback is narrow on
    /// purpose: only an id whose PRIMARY parent is an L0 app, so a stray hyphen in
    /// an unknown domain does not start borrowing weather's exemplar.
    #[test]
    fn a_composed_app_borrows_its_primary_parents_exemplar() {
        // Baked apps answer with their own, hyphenated id or not.
        // A baked app answers with its own spec, hyphenated id or not.
        let (spec, _) = super::l0_spec_and_exemplar("city-picks").expect("city-picks is baked");
        assert!(
            !spec.is_empty(),
            "a baked composed app must get its OWN spec, not a parent's"
        );
        // A parent that is not an L0 app lends nothing.
        assert!(
            super::l0_spec_and_exemplar("web-something").is_none(),
            "`web` has no L0 exemplar to lend"
        );
        assert!(
            super::l0_spec_and_exemplar("nonsense").is_none(),
            "an unknown domain with no hyphen resolves to nothing"
        );
        // And the level a composed app is approved for is its exemplar's, not
        // absent — otherwise `l0_level_refusal` waves everything through.
        assert!(
            super::l0_level_for("city-picks").is_some(),
            "a composed app must have an approved level"
        );
    }

    /// The language reference names exactly the themes the checker admits.
    ///
    /// The closed set lives in `splash_ui_l0::catalog::THEMES` and the agents
    /// only ever learn it from the baked `framework/l0.md`. Those are in
    /// different repositories; this binary is the one place both are visible. A
    /// theme documented but not admitted gets cards REFUSED for following the
    /// reference, and one admitted but not documented is a look no agent can ask
    /// for — both silent.
    #[test]
    fn the_language_reference_lists_every_admitted_theme() {
        for theme in splash_ui_l0::catalog::THEMES {
            assert!(
                super::L0_LANGUAGE.contains(&format!("`{theme}`")),
                "the checker admits theme {theme:?} and framework/l0.md never names it"
            );
        }
        // And the reference must not promise one the checker refuses. The `theme`
        // section is the only place moods are written as `theme <name>`.
        for line in super::L0_LANGUAGE.lines() {
            if let Some(rest) = line.trim().strip_prefix("theme ") {
                let named = rest.split_whitespace().next().unwrap_or("");
                if named.starts_with('<') {
                    continue; // `theme <mood>` — the placeholder, not a name
                }
                assert!(
                    splash_ui_l0::catalog::theme(named).is_some(),
                    "framework/l0.md shows `theme {named}`, which the checker refuses"
                );
            }
        }
    }

    /// Every exemplar is a card the checker accepts, at the level it declares.
    ///
    /// `L0_APPS` documented this and nothing enforced it, so an exemplar could
    /// drift into something the model would be shown as correct and then be
    /// refused for copying. The exemplar is the single strongest instruction in
    /// the prompt — it is a worked answer — so a broken one is worse than none.
    ///
    /// The level is asserted as DECLARED rather than as L0: `city-picks` is L1
    /// on purpose, and pinning every app to L0 would have made adding it look
    /// like a regression.
    #[test]
    fn every_exemplar_is_a_card_the_checker_accepts() {
        for (domain, _, exemplar) in super::L0_APPS {
            let report = splash_ui_l0::check_ui_l0_named(domain, exemplar);
            assert!(
                report.valid,
                "{domain}'s exemplar must be valid: {:#?}",
                report.diagnostics
            );
            let declared = exemplar
                .lines()
                .find_map(|l| l.trim().strip_prefix("# level:"))
                .map(|l| l.trim().to_owned())
                .unwrap_or_else(|| "L0".to_owned());
            let got = format!("{:?}", report.level);
            assert_eq!(
                got, declared,
                "{domain}'s exemplar declares {declared} and checks as {got}"
            );
        }
    }

    /// Every value `weather-activity` branches on can be answered before realize.
    ///
    /// The card being VALID proved nothing here, which is the whole defect class:
    /// it checked clean, realized clean, built a clean widget tree, and on the 6T
    /// it drew a correct header, a rain tile reading 100 %, and no verdict at
    /// all. Both halves of every complementary pair were false, because a guard
    /// is decided at realize against injected data and a fetched scalar was not
    /// in it.
    ///
    /// So the assertion is not "the card is fine". It is that the three values
    /// the tree turns on each reach a CALL this backend answers — which is
    /// exactly what `resolve_guards` walks.
    #[test]
    fn the_weather_activity_verdict_has_numbers_to_branch_on() {
        let (_, _, exemplar) = super::L0_APPS
            .iter()
            .find(|(d, _, _)| *d == "weather-activity")
            .expect("weather-activity is registered");

        let store = splash_ui_l0::InstanceStore::default();
        let guards = splash_ui_l0::guard_bindings(exemplar, &serde_json::json!({}), &store);

        let mut branched: Vec<String> = guards
            .iter()
            .map(|g| format!("{}.{}", g.source, g.field))
            .collect();
        branched.sort();
        assert_eq!(
            branched,
            vec!["air.aqi", "now.precip", "now.temp"],
            "rain, then air, then temperature — the order IS the reasoning"
        );

        // And each is a call, not a hope. A guard whose field this backend cannot
        // translate is the original bug wearing a fetch policy.
        for g in &guards {
            assert!(
                splash_ui_l0::makepad::vm_call(&g.binding).is_some(),
                "nothing answers {}.{} — the guard would be false either way",
                g.source,
                g.field
            );
        }
    }

    /// The baked framework manual names every app that can be routed to it.
    ///
    /// `framework.md` is `include_str!`d into the binary and handed to the AMA as
    /// its routing list and to each app agent as its manual, so a stale line here
    /// is shipped guidance — not documentation. And it HAD gone stale in three
    /// ways at once: it called `city-picks` the only app above L0 after `convert`
    /// joined it, omitted four routable apps entirely, and told a `youtube`-routed
    /// agent it was "in the wrong document" for a release after youtube became an
    /// ordinary L0 card. That last one is not a typo — an agent that believes it
    /// is in the wrong document does not write a card.
    ///
    /// Registration is the source of truth, because that is what routing reads.
    #[test]
    fn the_baked_manual_names_every_registered_app() {
        for (domain, _, _) in super::L0_APPS {
            assert!(
                super::L0_FRAMEWORK.contains(&format!("**{domain}**")),
                "framework.md's routing list omits `{domain}`, which the AMA can \
                 route to — an agent sent there has no entry to follow"
            );
        }
        // And the level claim, from the exemplars rather than from a fourth list.
        let above_l0: Vec<&str> = super::L0_APPS
            .iter()
            .filter(|(_, _, ex)| {
                ex.lines()
                    .any(|l| l.trim().starts_with("# level:") && !l.contains("L0"))
            })
            .map(|(d, _, _)| *d)
            .collect();
        for d in &above_l0 {
            assert!(
                super::L0_FRAMEWORK.contains(d),
                "`{d}` is above L0 and the manual never mentions it"
            );
        }
        // The manual states the COUNT in prose, so the count has to be right.
        let claim = match above_l0.len() {
            1 => "**One app is above L0",
            2 => "**Two apps are above L0",
            n => panic!("{n} apps are above L0 and the manual has no phrasing for that"),
        };
        assert!(
            super::L0_FRAMEWORK.contains(claim),
            "the manual miscounts the apps above L0: {above_l0:?}"
        );
    }

    /// An L0 app refuses a card that declares a wider grammar.
    ///
    /// §7: "escalation is never silent." The level was derived for the prompt's sake
    /// and then never compared to what came back — `render_ledger` admits on `valid`
    /// alone, and an L1 card with no diagnostics is valid AT L1. So a model that put
    /// `# level: L1` on a weather card got it drawn, and the header the profile calls
    /// the whole point of raising a level was decorative in the one direction that
    /// matters.
    #[test]
    fn a_card_wider_than_its_app_is_refused() {
        let l1 = splash_ui_l0::check_ui_l0_named(
            "probe",
            "# level: L1\n\
             source w sys.weather(lat: 1, lon: 2, fields: [temp])\n\
             view root Surface { TextHero(value: w.temp) }\n",
        );
        assert_eq!(format!("{:?}", l1.level), "L1", "the probe is L1");
        assert!(l1.valid, "and valid at its own level: {:#?}", l1.diagnostics);

        let refusal = super::l0_level_refusal("weather", &l1).expect("an L0 app refuses it");
        // The message is a REPAIR PROMPT — the model reads it and re-emits. It has to
        // name the level it wrote, the level it may use, and what to remove.
        assert!(
            refusal.contains("L1") && refusal.contains("L0") && refusal.contains("# level:"),
            "the refusal must be actionable: {refusal}"
        );

        // And the same card is accepted by an app that IS approved for L1, or the
        // rule would just be "never L1".
        let l1_app = super::L0_APPS
            .iter()
            .map(|(d, _, _)| *d)
            .find(|d| super::l0_level_for(d) == Some(splash_ui_l0::Level::L1));
        if let Some(d) = l1_app {
            assert!(
                super::l0_level_refusal(d, &l1).is_none(),
                "{d} is approved for L1 and must accept an L1 card"
            );
        }

        // An L0 card is never refused by this rule, at any app.
        let l0 = splash_ui_l0::check_ui_l0_named(
            "probe",
            "source w sys.weather(lat: 1, lon: 2, fields: [temp])\n\
             view root Surface { TextHero(value: w.temp) }\n",
        );
        for (d, _, _) in super::L0_APPS {
            assert!(
                super::l0_level_refusal(d, &l0).is_none(),
                "{d} must accept an L0 card"
            );
        }
    }

    /// An L1 app must not be told there is no arithmetic.
    ///
    /// The prompt carried L0's rule for every app while the spec beside it asked
    /// `city-picks` for one expression — two instructions in one prompt, in
    /// direct contradiction, with nothing telling the model which wins.
    #[test]
    fn the_prompt_states_the_rule_for_the_apps_own_level() {
        let l0 = super::l0_prompt_for("weather", "weather in kyoto").expect("weather has a spec");
        assert!(l0.contains("Write an L0 CARD"), "an L0 app is told L0");
        assert!(l0.contains("There is no arithmetic"), "and gets L0's rule");

        let l1 = super::l0_prompt_for("city-picks", "compare my cities").expect("city-picks has a spec");
        assert!(l1.contains("Write an L1 CARD"), "an L1 app is told L1");
        assert!(!l1.contains("There is no arithmetic"), "and is NOT told L0's rule:\n{l1}");
        assert!(l1.contains("must READ something"), "it gets L1's rule instead");
    }

    /// The two syntax classes live generation actually shipped as blank
    /// screens — a `when` guard nested inside a constructor's argument list,
    /// and a comma where an argument's `:` belongs — must be warned against
    /// IN the prompt, with the wrong form shown next to the right one. The
    /// guidance lives in framework/l0.md §5, which `l0_prompt_for` inlines.
    #[test]
    fn the_prompt_warns_against_the_observed_syntax_classes() {
        let p = super::l0_prompt_for("weather", "weather in kyoto").expect("weather has a spec");
        assert!(
            p.contains("never an argument"),
            "states that a guard is a statement, not an argument"
        );
        assert!(
            p.contains("cannot appear inside an argument list"),
            "shows the refusal a nested `when` causes"
        );
        assert!(
            p.contains("commas separate arguments"),
            "states the `name: value` comma-separated argument form"
        );
        assert!(
            p.contains("expected \":\" after argument name"),
            "shows the refusal a bare value / stray comma causes"
        );
    }

    /// Every plan domain must be served its PLAN spec, and every other domain its DSL
    /// spec. Asserting both directions means moving a domain in or out of PLAN_DOMAINS
    /// cannot silently serve the wrong one.
    #[test]
    fn plan_domain_is_served_the_plan_spec() {
        for d in crate::app::plan::PLAN_DOMAINS {
            let md = baked_app_md(d).unwrap_or_else(|| panic!("{d} spec must be baked in"));
            assert!(md.contains("```runplan"), "{d} must get plan.md");
            assert!(
                md.contains("You do **not** write the card"),
                "{d} plan spec must say the runtime builds it"
            );
        }
        // A DSL domain still gets its DSL spec.
        let act = baked_app_md("activity").expect("activity spec must be baked in");
        assert!(!act.contains("```runplan"), "activity must stay on the DSL path");
    }

    #[test]
    fn app_card_docs_baked_fallback_covers_builtin_apps() {
        // The fix for "other party: missing weather md files". Every built-in
        // app's spec + the shared widget docs are compiled INTO the binary, so a
        // plain `git clone → build → install` renders cards even when the
        // on-device app-cards dir was never provisioned (nothing in the normal
        // build deploys it there).
        for domain in [
            "weather",
            "stock",
            "news",
            "activity",
            "weather-activity",
            "nav",
            "web",
            "youtube",
        ] {
            let md = baked_app_md(domain)
                .unwrap_or_else(|| panic!("built-in app '{domain}' has no baked app.md"));
            assert!(md.len() > 200, "baked spec for '{domain}' looks empty");
        }
        // The nav spec must ship the canonical card the agent reproduces
        // verbatim (the whole app is that embedded card).
        let nav = baked_app_md("nav").unwrap();
        assert!(
            nav.contains("// name: nav-app") && nav.contains("MapView{"),
            "nav spec must embed the canonical card"
        );
        for w in [
            "design-system",
            "containers",
            "interaction",
            "sys-helpers",
            "weather-icon",
        ] {
            assert!(baked_widget_md(w).is_some(), "no baked widget doc for '{w}'");
        }
        // Runtime-composed apps (`<a>-<b>`) live only on-device — no baked copy.
        assert!(baked_app_md("some-composed-app").is_none());
        assert!(baked_widget_md("unknown-widget").is_none());

        // The assembled docs for a built-in domain are non-empty and inline BOTH
        // the routed spec and the shared widget docs — even with no deployed
        // tree, since the baked fallback supplies them.
        let docs = app_card_docs("weather");
        assert!(
            docs.contains("apps/weather/app.md"),
            "inlines the routed weather spec"
        );
        assert!(
            docs.contains("widgets/weather-icon.md"),
            "inlines the shared widget docs"
        );
    }

    #[test]
    fn nav_intent_destination_extraction() {
        // Intent-based navigation: the served nav card is seeded with the place
        // named in the request so it opens on that route preview. English " to "/
        // " of " and Chinese 去/到, with origin ("from …") + trailing qualifiers
        // ("怎么走", "please", punctuation) stripped.
        let cases: &[(&str, Option<&str>)] = &[
            ("directions to SFO", Some("SFO")),
            ("navigate to the Ferry Building", Some("the Ferry Building")),
            (
                "how do I get to Blue Bottle Coffee from SOMA",
                Some("Blue Bottle Coffee"),
            ),
            ("route to 1 Infinite Loop please", Some("1 Infinite Loop")),
            ("take me to SFO.", Some("SFO")),
            ("show me a map of Golden Gate Bridge", Some("Golden Gate Bridge")),
            ("导航去北京南站", Some("北京南站")),
            ("去外滩怎么走", Some("外滩")),
            ("怎么去机场", Some("机场")),
            ("到上海虹桥", Some("上海虹桥")),
            // no destination named → no seeding, card opens on the search box
            ("navigate", None),
            ("weather in tokyo", None),
        ];
        for (intent, want) in cases {
            assert_eq!(
                extract_nav_destination(intent).as_deref(),
                *want,
                "extract_nav_destination({intent:?})"
            );
        }
    }

    #[test]
    fn nav_parse_places_from_ama_decision() {
        // "LLM drives": the AMA appends `from=…; to=…` (disambiguated) to the nav
        // decision line; route_to_app seeds these as the card's origin/dest queries.
        let cases: &[(&str, Option<&str>, Option<&str>)] = &[
            (
                "nav — directions; from=Saratoga High School; to=NVIDIA Santa Clara",
                Some("Saratoga High School"),
                Some("NVIDIA Santa Clara"),
            ),
            ("nav — directions; to=SFO", None, Some("SFO")),
            ("nav — go; from=; to=Golden Gate Bridge", None, Some("Golden Gate Bridge")),
            ("nav — no places named", None, None),
            // stray quotes tolerated
            ("nav — x; from='Palo Alto'; to='Nvidia Santa Clara'", Some("Palo Alto"), Some("Nvidia Santa Clara")),
        ];
        for (dec, want_o, want_d) in cases {
            let (o, d) = parse_nav_places(dec);
            assert_eq!(o.as_deref(), *want_o, "origin for {dec:?}");
            assert_eq!(d.as_deref(), *want_d, "dest for {dec:?}");
        }
    }

    /// nav is GENERATED, not served — and its L0 spec has to be reachable.
    ///
    /// This asserted the opposite: that the client emitted the 664-line L2 trip
    /// planner verbatim and that `a2app/apps/nav/app.md` embedded it byte for
    /// byte. That rationale was the card's SIZE — "the on-device model
    /// under-generates / truncates this ~14 KB card" — and the card is not that
    /// size any more. `a2app-l0/apps/nav/exemplar.card` is the same screen in 92
    /// lines of L0, which is a request a capable model answers.
    ///
    /// What matters now is that nav reaches the generation path with a spec, so
    /// the test guards that instead of guarding a card nobody serves.
    #[test]
    fn nav_is_generated_from_an_l0_spec() {
        let prompt = super::l0_prompt_for("nav", "directions to SFO").expect("nav has an L0 spec");
        assert!(prompt.contains("```runl0"), "it must ask for an L0 card");
        // The exemplar travels with it, and it is the SHORT one.
        let (_, _, exemplar) = super::L0_APPS
            .iter()
            .find(|(d, _, _)| *d == "nav")
            .expect("nav is registered");
        // CODE lines, not commentary. The L0 exemplar is heavily annotated —
        // most of its length is the record of what each declaration replaced —
        // and counting comments measured the wrong thing: it failed when the card
        // gained travel modes, a waypoint and per-leg times, which is the card
        // getting closer to the L2 app's function rather than further from the
        // claim. The comparison worth asserting is statements against statements.
        let code = exemplar
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with('#')
            })
            .count();
        // 250, and the SAME number `splash-ui-l0`'s `the_nav_trip_planner_is_
        // expressible_at_l0` asserts. Two copies of one rule, in two repositories,
        // and this one was missed every time the other moved — which is how the
        // exemplar came to be 100 lines behind the fixture without any test saying
        // so. `exemplar_drift` in `l0_card.rs` now pins them to the same card, so
        // this bound and that one have to be raised together or the sync test fails
        // first.
        // 300, raised with its twin in `the_nav_trip_planner_is_expressible_at_l0`,
        // which carries the reasoning: both endpoints became tappable rows that open
        // a find state, because a permanently-live `Field` cannot be focused on this
        // renderer and both were inert.
        assert!(
            code < 300,
            "the exemplar must be the L0 rewrite, not the 664-line L2 card; \
             this is {code} lines of declarations"
        );
        assert!(
            splash_ui_l0::check_ui_l0_named("nav", exemplar).valid,
            "and it must be a card the checker admits"
        );
        // The map guard is MANDATORY — an unguarded map centres on -9999 and
        // loads tiles without bound (441% CPU, 3 GB, measured).
        assert!(
            exemplar.contains("here.ok == 1"),
            "the exemplar must guard its Map on having a position"
        );
    }

    #[test]
    fn fullbleed_root_height_fill_root_is_pinned() {
        // A model that made the immersive root `height: Fill` (instead of the
        // template's `height: 1500`) ships a card that lays out but paints blank.
        // The root must be pinned to a fixed height, and ONLY the root's own
        // `height: Fill` — the child image's `height: Fill` is left for the image
        // fit to handle.
        let card = "SolidView{ width: Fill height: Fill flow: Overlay new_batch: true\n\
             Image{ src: http_resource(sys.photo(\"tokyo\")) width: Fill height: Fill }\n\
             View{ width: Fill height: Fit flow: Down } }";
        assert!(card_root_height(card).is_none(), "Fill root has no fixed height");
        let out = pin_fullbleed_root_height(card);
        assert_eq!(
            card_root_height(&out),
            Some(FULLBLEED_FALLBACK_HEIGHT),
            "root pinned so the image fit finds root == image"
        );
        // Exactly one `height: Fill` rewritten: the child Image's remains for the
        // image-fit pass.
        assert_eq!(out.matches("height: Fill").count(), 1);
        assert!(out.contains(&format!("height: {FULLBLEED_FALLBACK_HEIGHT} flow: Overlay")));
    }

    #[test]
    fn fullbleed_root_height_leaves_fixed_and_fit_cards_alone() {
        // Already pins its root (>= 700): untouched.
        let fixed = "SolidView{ width: Fill height: 1500 flow: Overlay\n\
             Image{ width: Fill height: Fill } }";
        assert_eq!(pin_fullbleed_root_height(fixed), fixed);
        // A small `height: Fit` list card whose only `height: Fill` is a CHILD
        // must not have that child rewritten (would blow up a small card to
        // full-screen). Root attr span ends at the first child `{`, so the
        // child's Fill is out of reach.
        let fit = "RoundedView{ width: Fill height: Fit flow: Down\n\
             Image{ width: Fill height: Fill } }";
        assert_eq!(pin_fullbleed_root_height(fit), fit);
    }

    // (W02 strip) — `aichat_backend_type_includes_claude_code`,
    // `aichat_create_claude_code_agent`, `aichat_defaults_to_moonshot_when_available`,
    // `non_splash_prompt_documents_sequence_diagrams`, and
    // `non_splash_prompt_documents_all_diagram_types` lived here. They tested
    // `BackendType` and the inline `system_prompt`, both of which are gone.

    #[test]
    fn history_injection_allows_valid_diagram_assistant_messages() {
        let text = r#"```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft","kind":"start"},{"id":"done","label":"Done","kind":"end","role":"focal"}],"transitions":[{"from":"draft","to":"done","label":"submit"}]}
```"#;

        assert!(assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn history_injection_rejects_incomplete_diagram_assistant_messages() {
        let text = r#"```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft","kind":"start"},{"id":"pending","label":"Pending Payment"},{"id":"paid","label":"
"#;

        assert!(!assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn history_injection_rejects_invalid_closed_diagram_assistant_messages() {
        let text = r#"```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft"}],
```"#;

        assert!(!assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn history_injection_allows_non_diagram_assistant_messages() {
        let text = "这里是普通解释，没有 diagram fence。";

        assert!(assistant_message_is_safe_to_store(text));
        assert!(assistant_message_is_safe_for_history(text));
    }

    // Regression: an unclosed *non-diagram* fence (e.g. response truncated
    // mid `rust`/`mermaid` block) was discarding the entire reply because
    // FenceScanError::Unclosed was treated the same as a malformed diagram.
    #[test]
    fn store_keeps_reply_with_unclosed_non_diagram_fence() {
        let text = "Here's a markdown demo:\n\n```rust\nfn main() {\n    println!(\"hi\";\n";
        assert!(assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn store_rejects_bad_diagram_even_with_later_unclosed_non_diagram_fence() {
        let text = r#"```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft"}],
```

```rust
fn main() {
"#;

        assert!(!assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn outer_markdown_wrapper_is_unwrapped_before_diagram_safety_scan() {
        let text = r#"```markdown
Here is a diagram:

```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft","kind":"start"},{"id":"done","label":"Done","kind":"end"}],"transitions":[{"from":"draft","to":"done","label":"submit"}]}
```
```"#;

        assert!(assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn outer_markdown_wrapper_rejects_bad_inner_diagram() {
        let text = r#"```markdown
Here is a broken diagram:

```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft"}],
```
```"#;

        assert!(!assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }
}

#[cfg(test)]
mod state_injection_review {
    use super::*;
    use std::collections::BTreeMap;

    /// A state value is spliced into the card SOURCE unescaped, and the
    /// framework documents reading it from INSIDE a string literal
    /// (`a2app/framework.md`: `Label{ text: "Count: {{state.count}}" }`,
    /// `apps/nav/app.md:187`: `let q = "{{state.q}}"`). A value containing a
    /// quote therefore terminates the literal.
    #[test]
    fn a_quote_in_state_breaks_out_of_the_card_string() {
        let mut st: CardState = BTreeMap::new();
        st.insert("q".to_string(), "Bei\"jing".to_string());
        let card = r#"let q = "{{state.q}}""#;
        let out = substitute_state_keys(card, &st);
        assert_eq!(out, r#"let q = "Bei"jing""#);
    }

    /// The same splice accepts arbitrary card DSL, so a crafted value is not
    /// merely a parse break — it lands as code in the card's VM.
    #[test]
    fn a_crafted_state_value_injects_dsl() {
        let mut st: CardState = BTreeMap::new();
        st.insert(
            "q".to_string(),
            r#"x" + sys.notify("pwned") + ""#.to_string(),
        );
        let out = substitute_state_keys(r#"let q = "{{state.q}}""#, &st);
        assert!(out.contains(r#"sys.notify("pwned")"#), "{out}");
    }
}

#[cfg(test)]
mod plan_fence_tolerance {
    /// A correct plan in a ```json fence still builds a card — measured on
    /// device, a model emitted exactly this and the whole answer rendered as
    /// raw JSON to the user.
    #[test]
    fn a_plan_in_a_json_fence_is_accepted() {
        let msg = "here you go\n\n```json\n{\"plan\":\"weather\",\"locale\":\"en\",\
            \"place\":{\"query\":\"Shanghai\"},\"photo\":\"x\",\
            \"sections\":[{\"block\":\"CurrentConditions\"}]}\n```";
        let body = crate::card_splash_body(msg).expect("a plan is a plan");
        assert!(body.contains("Shanghai"), "{body}");
    }

    /// Tolerant, not credulous: JSON that is not a plan stays text.
    #[test]
    fn unrelated_json_is_left_alone() {
        let msg = "```json\n{\"hello\":\"world\"}\n```";
        assert!(crate::card_splash_body(msg).is_none());
    }
}
