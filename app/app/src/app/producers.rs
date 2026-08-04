//! W07 / M3 — Studio / Slides / Sites producer surfaces.
//!
//! Three structurally identical triptych screens (source · chat · output),
//! parameterised by `ProducerKind`. Per `04-IA-AND-NAVIGATION.md` §
//! "StudioScreen / SlidesScreen / SitesScreen" and
//! `workstreams/W07-studio-slides-sites.md` — this is the M3 stub:
//! the IA shell + per-kind state slice + sidebar navigation.
//!
//! Deferred (W07 § "Out"): real generation API integration via
//! `system_prompt_id`, Slides PPTX export, Sites screenshot fallback,
//! present mode, embedded preview browser, source uploads, producer-
//! flavoured task dock filter.
//!
//! Patterns lifted: per-kind `LazyLock<RwLock>` state slice
//! (`sessions.rs:39`, `coding.rs:87`), embedded `ChatList` (`main.rs:359`),
//! generation row template (`coding.rs:217`), `robius_open` OS handoff
//! (`main.rs:2671`).

use std::sync::{LazyLock, RwLock};

use makepad_widgets::*;
use octos_app_store::navigation::{Producer, ProjectId};

/// Which producer surface a state slice / widget belongs to.
///
/// `to_label` powers the screen header; `system_prompt_context_id`
/// is the placeholder for the server-resolved system prompt id we'll
/// wire into the turn create RPC when generation lands (W07 follow-up).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProducerKind {
    Studio,
    Slides,
    Sites,
}

impl ProducerKind {
    /// Screen-header label. Kept for the producer screens (their DSL
    /// templates are removed in this build; `draw_producer` is the only
    /// non-test caller).
    #[allow(dead_code)]
    pub fn to_label(self) -> &'static str {
        match self {
            Self::Studio => "Studio",
            Self::Slides => "Slides",
            Self::Sites => "Sites",
        }
    }

    /// Server-side system-prompt context id. **Placeholder** — wire this
    /// into `system_prompt_id` on turn create once the producer tools
    /// are reachable (W07 brief § "Where this differs from chat").
    /// Only caller today is the unwired `draw_producer` header pill.
    #[allow(dead_code)]
    pub fn system_prompt_context_id(self) -> &'static str {
        match self {
            Self::Studio => "studio.system_prompt.v1",
            Self::Slides => "slides.system_prompt.v1",
            Self::Sites => "sites.system_prompt.v1",
        }
    }

    /// Map a Producer (navigation axis) to its UI kind. Mirrors the
    /// `OpenProject` → `CurrentScreen::*` dispatch in `navigation.rs:79`.
    #[allow(dead_code)]
    pub fn from_producer(p: Producer) -> Self {
        match p {
            Producer::Studio => Self::Studio,
            Producer::Slides => Self::Slides,
            Producer::Sites => Self::Sites,
        }
    }

}

/// Per-kind project metadata. M3 stub — `id`, `title`, optional chat
/// session id (a future hookup point for the embedded ChatList).
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ProjectMeta {
    pub id: ProjectId,
    pub title: String,
    pub session_id: Option<String>,
}

/// One generation result. M3 stub: a free-form title + kind tag + an
/// optional URL to hand off via `robius_open`. The store-side
/// `StudioOutput` / slide manifest / site preview-url shapes live in the
/// W04 store crate; we don't duplicate them here for the M3 pass.
/// Fields are read by `draw_producer` / `handle_producer_event`, which are
/// kept but unwired in this build (screen DSL templates removed).
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct GenerationOutput {
    pub title: String,
    /// e.g. "summary" / "podcast" / "slides" / "site".
    pub kind: String,
    /// `None` = not yet exported / inline-only.
    pub open_url: Option<String>,
}

/// Per-kind UI state slice. Three separate slices (one per kind) so the
/// screens don't trample each other.
#[derive(Debug, Default)]
pub struct ProducerState {
    /// Reserved — populated by `OpenProject` once project list hydrate lands.
    #[allow(dead_code)]
    pub current: Option<ProjectId>,
    /// Reserved — populated by the producer-projects REST hydrate.
    #[allow(dead_code)]
    pub projects: Vec<ProjectMeta>,
    /// User-pasted source rows (URL or text). Local-only until upload lands.
    pub sources: Vec<String>,
    /// Latest first; rendered via PortalList. Read by the (currently
    /// unwired) producer screen draw/event path — kept for W07.
    #[allow(dead_code)]
    pub generation_history: Vec<GenerationOutput>,
    /// Stash for the source TextInput so typing survives redraws.
    pub source_input_buffer: String,
}

pub static STUDIO_STATE: LazyLock<RwLock<ProducerState>> =
    LazyLock::new(|| RwLock::new(ProducerState::default()));
pub static SLIDES_STATE: LazyLock<RwLock<ProducerState>> =
    LazyLock::new(|| RwLock::new(ProducerState::default()));
pub static SITES_STATE: LazyLock<RwLock<ProducerState>> =
    LazyLock::new(|| RwLock::new(ProducerState::default()));

/// Borrow the per-kind slice. Match arms each call into the kind's
/// `LazyLock` so callers don't have to import three statics.
/// Only called from `draw_producer` / `handle_producer_event` (kept but
/// unwired in this build; screen DSL templates removed).
#[allow(dead_code)]
fn with_state<R>(kind: ProducerKind, f: impl FnOnce(&ProducerState) -> R) -> Option<R> {
    let g = match kind {
        ProducerKind::Studio => STUDIO_STATE.read(),
        ProducerKind::Slides => SLIDES_STATE.read(),
        ProducerKind::Sites => SITES_STATE.read(),
    }
    .ok()?;
    Some(f(&g))
}

fn with_state_mut<R>(kind: ProducerKind, f: impl FnOnce(&mut ProducerState) -> R) -> Option<R> {
    let mut g = match kind {
        ProducerKind::Studio => STUDIO_STATE.write(),
        ProducerKind::Slides => SLIDES_STATE.write(),
        ProducerKind::Sites => SITES_STATE.write(),
    }
    .ok()?;
    Some(f(&mut g))
}

/// Click on a generation history "Open" button. Empty `open_url` —
/// nothing to do; full URL — hand off via `robius_open`.
pub fn open_generation_externally(url: &str) {
    if url.is_empty() {
        return;
    }
    if let Err(e) = robius_open::Uri::new(url).open() {
        log::warn!("producers open_in_os {url}: {e:?}");
    }
}

/// Append a typed source row, clearing the buffer. Called from
/// `App::handle_actions` on "Add Source" click.
pub fn fold_add_source(kind: ProducerKind, raw: String) {
    let trimmed = raw.trim().to_owned();
    if trimmed.is_empty() {
        return;
    }
    let _ = with_state_mut(kind, |s| {
        s.sources.push(trimmed);
        s.source_input_buffer.clear();
    });
}

/// Mirror the source TextInput buffer into state so it survives redraws.
pub fn fold_source_input_changed(kind: ProducerKind, text: String) {
    let _ = with_state_mut(kind, |s| {
        s.source_input_buffer = text;
    });
}

/// UI-thread action posted by per-screen "Add Source" / "Open" clicks.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ProducerUiAction {
    AddSource { kind: ProducerKind, text: String },
    OpenGeneration { kind: ProducerKind, url: String },
    SourceInputChanged { kind: ProducerKind, text: String },
}

// W07: the live-DSL prototypes for the three producer screens
// (`mod.widgets.StudioScreen` / `SlidesScreen` / `SitesScreen` plus the
// inner `mod.widgets.GenerationCard`) live in `app/src/main.rs`'s
// `script_mod!` block alongside the rest of the chat shell. Mirrors the
// `let SessionList = #(...)` / `let TaskDock = #(...)` pattern (Rust
// impl lives in this module; DSL body inlined in `main.rs`). Putting
// the DSL there is what lets the chat pane embed `ChatList` directly
// without producers.rs having to re-publish a fresh `mod.widgets.ChatList`.

/// Thin row holder for the generation-history card. Logic lives in
/// `populate_generation_card`; this struct only exists so the live-DSL
/// prototype name is uniquely registered.
#[derive(Script, ScriptHook, Widget)]
pub struct GenerationCardWidget {
    #[deref]
    view: View,
}

impl Widget for GenerationCardWidget {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
}

/// Triptych screen draw helper. Three thin per-kind wrappers below
/// share this body so the DSL prototype name dispatch (Studio / Slides
/// / Sites) maps to the right state slice.
/// Kept but unwired: the Studio/Slides/Sites DSL templates are removed in
/// this build (`main.rs` script_mod! — "templates removed — unsupported in
/// this build"), so the screen widgets below are never constructed.
#[allow(dead_code)]
fn draw_producer(view: &mut View, kind: ProducerKind, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
    let (sources, history) = with_state(kind, |s| {
        (s.sources.clone(), s.generation_history.clone())
    })
    .unwrap_or_default();

    // Header strip — title + system-prompt-context-id pill (M3 stub
    // surface so devs can eyeball which prompt would be sent).
    view.label(cx, ids!(producer_title))
        .set_text(cx, kind.to_label());
    view.label(cx, ids!(producer_subtitle))
        .set_text(cx, &format!("system_prompt: {}", kind.system_prompt_context_id()));

    // Empty-state toggles for the source list and output history.
    view.label(cx, ids!(source_empty))
        .set_visible(cx, sources.is_empty());
    view.view(cx, ids!(output_empty))
        .set_visible(cx, history.is_empty());

    let source_uid = view.portal_list(cx, ids!(source_list)).widget_uid();
    let output_uid = view.portal_list(cx, ids!(output_list)).widget_uid();

    while let Some(item) = view.draw_walk(cx, scope, walk).step() {
        let item_uid = item.widget_uid();
        if let Some(mut list) = item.as_portal_list().borrow_mut() {
            if item_uid == source_uid {
                list.set_item_range(cx, 0, sources.len());
                while let Some(item_id) = list.next_visible_item(cx) {
                    let Some(text) = sources.get(item_id) else { continue };
                    let row = list.item(cx, item_id, id!(SourceRow));
                    row.label(cx, ids!(source_text_label)).set_text(cx, text);
                    row.draw_all_unscoped(cx);
                }
            } else if item_uid == output_uid {
                list.set_item_range(cx, 0, history.len());
                while let Some(item_id) = list.next_visible_item(cx) {
                    let Some(out) = history.get(item_id) else { continue };
                    let card = list.item(cx, item_id, id!(GenRow));
                    card.label(cx, ids!(gen_kind_label))
                        .set_text(cx, &out.kind);
                    card.label(cx, ids!(gen_title_label))
                        .set_text(cx, &out.title);
                    // Disable the open button when there's no URL —
                    // visual hint that nothing happens on click.
                    let open_btn = card.button(cx, ids!(gen_open_button));
                    open_btn.set_visible(cx, out.open_url.is_some());
                    card.draw_all_unscoped(cx);
                }
            }
        }
    }
    DrawStep::done()
}

/// Kept but unwired in this build — see `draw_producer` above.
#[allow(dead_code)]
fn handle_producer_event(
    view: &mut View,
    kind: ProducerKind,
    cx: &mut Cx,
    event: &Event,
    scope: &mut Scope,
) {
    view.handle_event(cx, event, scope);
    if let Event::Actions(actions) = event {
        // Source input — mirror the buffer so it survives redraws.
        if let Some(text) = view
            .text_input(cx, ids!(source_input))
            .changed(actions)
        {
            Cx::post_action(ProducerUiAction::SourceInputChanged { kind, text });
        }

        // Add-source button — flush the buffer into `sources`.
        if view
            .button(cx, ids!(add_source_button))
            .clicked(actions)
        {
            let text = with_state(kind, |s| s.source_input_buffer.clone())
                .unwrap_or_default();
            if !text.trim().is_empty() {
                Cx::post_action(ProducerUiAction::AddSource { kind, text });
                // Clear the text input UI side too — fold also
                // resets the buffer, but the visible widget retains
                // its rendering until we explicitly clear.
                view.text_input(cx, ids!(source_input))
                    .set_text(cx, "");
            }
        }

        // Generation card "Open" buttons — route to robius_open via
        // ProducerUiAction. We need the per-row history snapshot to
        // pull `open_url`.
        let output_list = view.portal_list(cx, ids!(output_list));
        if output_list.any_items_with_actions(actions) {
            let history: Vec<GenerationOutput> =
                with_state(kind, |s| s.generation_history.clone()).unwrap_or_default();
            for (item_id, item) in output_list.items_with_actions(actions) {
                let Some(row) = history.get(item_id) else { continue };
                if item.button(cx, ids!(gen_open_button)).clicked(actions) {
                    if let Some(url) = row.open_url.as_deref() {
                        Cx::post_action(ProducerUiAction::OpenGeneration {
                            kind,
                            url: url.to_owned(),
                        });
                    }
                }
            }
        }
    }
}

/// One Rust struct per kind so each lives-DSL prototype name is unique
/// (`mod.widgets.StudioScreen`, `mod.widgets.SlidesScreen`,
/// `mod.widgets.SitesScreen`). All three delegate to the same
/// `draw_producer` / `handle_producer_event` helpers.
macro_rules! decl_producer_screen {
    ($name:ident, $kind:expr) => {
        // Kept but never constructed in this build: the matching DSL
        // templates were removed from `main.rs`'s script_mod! (W07
        // follow-up will re-register them).
        #[allow(dead_code)]
        #[derive(Script, ScriptHook, Widget)]
        pub struct $name {
            #[deref]
            view: View,
        }
        impl Widget for $name {
            fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
                draw_producer(&mut self.view, $kind, cx, scope, walk)
            }
            fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
                handle_producer_event(&mut self.view, $kind, cx, event, scope);
            }
        }
    };
}
decl_producer_screen!(StudioScreenWidget, ProducerKind::Studio);
decl_producer_screen!(SlidesScreenWidget, ProducerKind::Slides);
decl_producer_screen!(SitesScreenWidget, ProducerKind::Sites);

// TODO(W07.session): when `OpenProject` lands a real project hydrate,
// look up the project's `chatSessionId` and write
// `APP_STATE.current_session = Some(session_id)` so the embedded
// ChatList re-mounts on its thread. M3 keeps the globally-active session.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_labels_round_trip() {
        assert_eq!(ProducerKind::Studio.to_label(), "Studio");
        assert_eq!(ProducerKind::Slides.to_label(), "Slides");
        assert_eq!(ProducerKind::Sites.to_label(), "Sites");
    }

    #[test]
    fn kind_from_navigation_producer() {
        assert_eq!(
            ProducerKind::from_producer(Producer::Studio),
            ProducerKind::Studio
        );
        assert_eq!(
            ProducerKind::from_producer(Producer::Slides),
            ProducerKind::Slides
        );
        assert_eq!(
            ProducerKind::from_producer(Producer::Sites),
            ProducerKind::Sites
        );
    }

    #[test]
    fn fold_add_source_trims_and_skips_empty() {
        // Use Sites slice so the assertion isn't polluted by other tests.
        // (Tests in this module run sequentially per crate, but we play
        // safe by clearing the slice we touch.)
        if let Ok(mut s) = SITES_STATE.write() {
            s.sources.clear();
            s.source_input_buffer.clear();
        }
        fold_add_source(ProducerKind::Sites, "  ".to_owned());
        assert!(SITES_STATE
            .read()
            .map(|s| s.sources.is_empty())
            .unwrap_or(false));
        fold_add_source(ProducerKind::Sites, "  https://example.com  ".to_owned());
        let snap = SITES_STATE.read().unwrap();
        assert_eq!(snap.sources, vec!["https://example.com".to_string()]);
        assert!(snap.source_input_buffer.is_empty());
    }

    #[test]
    fn slices_are_independent_per_kind() {
        if let Ok(mut s) = STUDIO_STATE.write() {
            s.sources.clear();
        }
        if let Ok(mut s) = SLIDES_STATE.write() {
            s.sources.clear();
        }
        fold_add_source(ProducerKind::Studio, "studio-source".to_owned());
        let studio = STUDIO_STATE.read().unwrap().sources.clone();
        let slides = SLIDES_STATE.read().unwrap().sources.clone();
        assert_eq!(studio, vec!["studio-source".to_string()]);
        assert!(!slides.iter().any(|s| s == "studio-source"));
    }
}
