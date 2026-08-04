//! W06 / M3 — `CodingScreen`: two-pane approvals workspace.
//!
//! Layout per `04-IA-AND-NAVIGATION.md` § "CodingScreen" and
//! `workstreams/W06-coding-workspace.md`:
//!
//! ```text
//! ┌─ approvals queue (380px) ─┬─ preview pane (PageFlip) ─┐
//! │  pending PortalList       │  Diff/Command/Network/FS  │
//! │  history PortalList       │  /OutputTail (5 sub-views)│
//! └────────────────────────────┴───────────────────────────┘
//! ```
//!
//! Mirrors `octos-web/src/coding/coding-workspace-page.tsx` (right
//! `PageFlip` consolidation per W06 brief § "Layout"). Reuses W05's
//! `ApprovalCard` typed-payload rendering for the per-card preview;
//! this file owns the queue + PageFlip dispatch + output-tail buffer.
//!
//! Cited octos-core ui_protocol.rs types: `ApprovalId` :85,
//! `OutputCursor` :117, `approval_kinds` :34, `ApprovalCommandDetails`
//! :1346, `ApprovalDiffDetails` :1372, `ApprovalFilesystemDetails` :1387,
//! `ApprovalNetworkDetails` :1397, `ApprovalTypedDetails` :1432,
//! `ApprovalRequestedEvent` :1480, `TaskOutputReadParams` :634.
//!
//! Patterns lifted: read-only widget over `APP_STATE`
//! (`app/src/app/sessions.rs:185-230`, `task_dock.rs:205-230`);
//! per-instance `CodeView` font override (`aichat:514-542`); empty
//! state centered card (`aichat:966–984`); PageFlip dispatch
//! (`aichat/studio/desktop/src/desktop_file_tree.rs:91`).
//!
//! Deferred per the W06 brief: real diff hunk rendering via
//! `diff/preview/get` (M3.5), batch-approve pill, dynamic key-bindings.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use makepad_widgets::*;
use crate::fpath;
use octos_app_store::approvals::ApprovalState;
use octos_core::ui_protocol::{
    approval_kinds, ApprovalDecision, ApprovalId, ApprovalRequestedEvent,
    ApprovalTypedDetails, OutputCursor, TaskOutputReadParams, TaskOutputReadResult,
};
use octos_core::{SessionKey, TaskId};

use crate::app::sessions::APP_STATE;

/// Rolling buffer cap for the output tail (matches `octos-web`
/// `use-coding-app-ui.ts:131`, W06 brief § "Task output drill-down").
const OUTPUT_TAIL_CAP_BYTES: usize = 12 * 1024;

/// Per-task output buffer + last server cursor.
#[derive(Clone, Debug, Default)]
pub struct TaskOutputBuffer {
    pub text: String,
    pub last_cursor: Option<OutputCursor>,
}

impl TaskOutputBuffer {
    /// Append + trim to last `OUTPUT_TAIL_CAP_BYTES`. UTF-8-aware: we
    /// walk forward to the next char boundary so a multi-byte glyph
    /// doesn't get split across the trim.
    pub fn append(&mut self, chunk: &str) {
        self.text.push_str(chunk);
        if self.text.len() <= OUTPUT_TAIL_CAP_BYTES {
            return;
        }
        let cut_target = self.text.len().saturating_sub(OUTPUT_TAIL_CAP_BYTES);
        let mut idx = cut_target;
        while idx < self.text.len() && !self.text.is_char_boundary(idx) {
            idx += 1;
        }
        self.text.replace_range(0..idx, "");
    }

    pub fn set_cursor(&mut self, cursor: OutputCursor) {
        self.last_cursor = Some(cursor);
    }
}

/// W06 view-state slice — ephemeral, non-protocol per W06 brief.
#[derive(Debug, Default)]
pub struct CodingViewState {
    pub selected_approval: Option<ApprovalId>,
    pub output_buffers: HashMap<TaskId, TaskOutputBuffer>,
    pub selected_task: Option<TaskId>,
}

pub static CODING_VIEW_STATE: LazyLock<RwLock<CodingViewState>> =
    LazyLock::new(|| RwLock::new(CodingViewState::default()));

/// Cross-thread action posted when a `task/output/read` reply lands.
/// App folds via `fold_task_output`.
#[derive(Debug)]
pub struct TaskOutputAction {
    pub task_id: TaskId,
    pub session_id: SessionKey,
    pub outcome: TaskOutputOutcome,
}

/// `Loaded`/`Failed` variants are produced by the M3.5 transport
/// handle (TODO `main.rs::fire_task_output_read`); the public shape is
/// stable now so the call sites don't churn.
#[derive(Debug)]
#[allow(dead_code)]
// `Loaded` carries the full protocol result inline; boxing the enum is not
// worth it for a two-variant UI action (matches the workspace lint policy).
#[allow(clippy::large_enum_variant)]
pub enum TaskOutputOutcome {
    Loaded(TaskOutputReadResult),
    Failed(String),
}

/// UI-thread action emitted by row clicks. `SelectTask` is wired
/// in `App::handle_actions` for the future TaskDock-on-CodingScreen
/// pivot (W06 § "Task output drill-down"); empty in M3.
#[derive(Debug, Clone)]
#[allow(dead_code)]
// The shared `Select` prefix reads better at the `Cx::post_action` call
// sites than the clippy-suggested renames; these are UI actions, not a
// wire protocol.
#[allow(clippy::enum_variant_names)]
pub enum CodingUiAction {
    SelectApproval(ApprovalId),
    SelectHistory(ApprovalId),
    SelectTask(TaskId),
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    let CodingHeading = Label {
        width: Fill height: Fit margin: Inset{top: 0 bottom: 4 left: 2 right: 2}
        draw_text.color: #xCDBF9FA0 draw_text.text_style.font_size: 11
    }
    let CodingDim = Label {
        width: Fill height: Fit
        draw_text.color: #xCDBF9FCC draw_text.text_style.font_size: 11
    }
    let CodingSub = Label {
        width: Fill height: Fit
        draw_text.color: #x72E4FF draw_text.text_style.font_size: 11
    }
    let CodingTitle = Label {
        width: Fill height: Fit
        draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 14
    }
    let CodingRiskBadge = RoundedView {
        width: Fit height: Fit show_bg: true
        padding: Inset{left: 6 right: 6 top: 1 bottom: 1} margin: Inset{right: 6}
        draw_bg +: { color: #x42330F radius: 6.0 }
        risk_label := Label { text: "" draw_text.color: #xF6BE63 draw_text.text_style.font_size: 9 }
    }
    // Per-instance CodeView font override (aichat:514-542) — defends
    // against `theme.font_code` being baked at CodeView expansion.
    let CodingCodeView = CodeView {
        keep_cursor_at_end: false
        editor +: {
            height: Fit
            draw_bg +: { color: #x031510EE }
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

    // Pending queue row: focus-only card. Tighter than W05's
    // `ApprovalCardView` — Approve/Deny live on the W05 ApprovalsPane
    // (chat) for now; the queue is for selection.
    mod.widgets.ApprovalQueueRow = #(crate::app::coding::ApprovalQueueRowWidget::register_widget(vm)) {
        width: Fill height: Fit flow: Down spacing: 4 show_bg: true
        margin: Inset{top: 3 bottom: 3 left: 4 right: 4}
        padding: Inset{left: 10 top: 8 right: 10 bottom: 8}
        draw_bg +: { color: #x0A2A22DD radius: 10.0 }

        select_button := ButtonFlat {
            width: Fill height: Fit text: ""
            padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
            flow: Down spacing: 4
            draw_text +: { color: #00000000 }
            draw_bg +: {
                color: #00000000 color_hover: #00000000
                border_size: 0.0 border_radius: 0.0
            }
            row_header := View {
                width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 4
                row_risk_badge := CodingRiskBadge {}
                row_tool_label := Label {
                    text: "" draw_text.color: #x72E4FF draw_text.text_style.font_size: 11
                }
                View { width: Fill height: 1 }
                row_age_label := Label {
                    text: "" draw_text.color: #xCDBF9F77 draw_text.text_style.font_size: 10
                }
            }
            row_title_label := Label {
                width: Fill height: Fit text: ""
                draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 12
            }
        }
        // Highlight-when-selected stripe along the bottom; toggled by
        // the parent widget in `populate_queue_row`.
        row_selected_marker := SolidView {
            width: Fill height: 2 visible: false
            draw_bg.color: #x72E4FF
        }
    }

    // History row — compact, single-line summary with a status pip.
    mod.widgets.ApprovalHistoryRow = #(crate::app::coding::ApprovalHistoryRowWidget::register_widget(vm)) {
        width: Fill height: Fit flow: Down show_bg: true
        margin: Inset{top: 1 bottom: 1 left: 4 right: 4}
        padding: Inset{left: 10 top: 4 right: 10 bottom: 4}
        draw_bg +: { color: #x06231CCC radius: 6.0 }
        history_button := ButtonFlat {
            width: Fill height: Fit text: ""
            padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
            flow: Right align: Align{y: 0.5} spacing: 6
            draw_text +: { color: #00000000 }
            draw_bg +: {
                color: #00000000 color_hover: #00000000
                border_size: 0.0 border_radius: 0.0
            }
            history_pip := Label {
                width: Fit text: "•"
                draw_text.color: #x72E4FF draw_text.text_style.font_size: 13
            }
            history_summary := Label {
                width: Fill height: Fit text: ""
                draw_text.color: #xCDBF9FCC draw_text.text_style.font_size: 11
            }
        }
    }

    mod.widgets.CodingScreen = #(crate::app::coding::CodingScreen::register_widget(vm)) {
        width: Fill height: Fill flow: Right spacing: 12
        padding: Inset{left: 8 right: 8 top: 4 bottom: 4}

        queue_pane := View {
            width: 380 height: Fill flow: Down spacing: 6
            queue_heading := CodingHeading { text: "Pending approvals" }
            queue_empty := View {
                width: Fill height: Fit flow: Down align: Align{x: 0.5 y: 0.5}
                margin: Inset{top: 60} visible: true
                Label {
                    text: "Nothing to review."
                    draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 14
                }
                Label {
                    text: "Approvals from the agent will queue here."
                    draw_text.color: #xCDBF9FAA draw_text.text_style.font_size: 11
                    margin: Inset{top: 6}
                }
            }
            pending_list := PortalList {
                width: Fill height: Fill flow: Down
                drag_scrolling: true auto_tail: false
                Row := mod.widgets.ApprovalQueueRow {}
            }
            history_divider := SolidView {
                width: Fill height: 1 margin: Inset{top: 8 bottom: 8}
                draw_bg.color: #xEAD8B81C
            }
            history_heading := CodingHeading { text: "History" }
            history_list := PortalList {
                width: Fill height: Fit flow: Down
                drag_scrolling: false auto_tail: false
                HistoryRow := mod.widgets.ApprovalHistoryRow {}
            }
        }

        preview_pane := View {
            width: Fill height: Fill flow: Down spacing: 6
            preview_heading := CodingHeading { text: "Preview" }
            preview_flip := PageFlip {
                active_page: @empty_page
                width: Fill height: Fill

                empty_page := View {
                    width: Fill height: Fill flow: Down align: Align{x: 0.5 y: 0.5}
                    Label {
                        text: "Select an approval to preview."
                        draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 13
                    }
                    Label {
                        text: "Diff / command / network / filesystem details land here."
                        draw_text.color: #xCDBF9FAA draw_text.text_style.font_size: 11
                        margin: Inset{top: 6}
                    }
                }

                // Diff: M3 ships summary + body; per-file hunks via
                // FetchDiffPreview is the M3.5 follow-up.
                diff_page := View {
                    width: Fill height: Fill flow: Right spacing: 12
                    diff_files_pane := View {
                        width: 220 height: Fill flow: Down spacing: 4
                        CodingHeading { text: "Files" }
                        diff_files_label := CodingDim { text: "(no parsed preview yet)" }
                    }
                    diff_hunks_pane := View {
                        width: Fill height: Fill flow: Down spacing: 4
                        diff_summary_label := CodingTitle {}
                        diff_op_label := CodingSub {}
                        diff_hunks_view := CodingCodeView {}
                    }
                }

                command_page := View {
                    width: Fill height: Fit flow: Down spacing: 6
                    command_title := CodingTitle {}
                    command_line_view := CodingCodeView {}
                    command_cwd_label := CodingDim {}
                    command_env_label := CodingDim {}
                }

                network_page := View {
                    width: Fill height: Fit flow: Down spacing: 6
                    network_title := CodingTitle {}
                    network_op_label := CodingSub {}
                    network_hosts_label := CodingDim {}
                    network_urls_view := CodingCodeView {}
                }

                filesystem_page := View {
                    width: Fill height: Fit flow: Down spacing: 6
                    filesystem_title := CodingTitle {}
                    filesystem_op_label := CodingSub {}
                    filesystem_paths_view := CodingCodeView {}
                    filesystem_warn_label := Label {
                        width: Fill height: Fit text: ""
                        draw_text.color: #xF6BE63 draw_text.text_style.font_size: 11
                    }
                }

                output_tail_page := View {
                    width: Fill height: Fill flow: Down spacing: 6
                    output_tail_heading := CodingHeading { text: "Task output" }
                    output_tail_scroll := ScrollYView {
                        width: Fill height: Fill flow: Down
                        output_tail_view := CodingCodeView {}
                    }
                    output_tail_meta := Label {
                        width: Fill height: Fit text: ""
                        draw_text.color: #xCDBF9F77 draw_text.text_style.font_size: 10
                    }
                }
            }
        }
    }
}

// Thin row holders — logic lives in `populate_queue_row` /
// `populate_history_row`. Two struct types so the live-DSL prototype
// names stay distinct.
macro_rules! decl_row_widget {
    ($name:ident) => {
        #[derive(Script, ScriptHook, Widget)]
        pub struct $name { #[deref] view: View }
        impl Widget for $name {
            fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
                self.view.draw_walk(cx, scope, walk)
            }
            fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
                self.view.handle_event(cx, event, scope);
            }
        }
    };
}
decl_row_widget!(ApprovalQueueRowWidget);
decl_row_widget!(ApprovalHistoryRowWidget);

#[derive(Clone)]
struct PendingSnapshot {
    /// Reserved for batch-by-kind affordance (W06 § "Batch affordance").
    #[allow(dead_code)]
    approval_id: ApprovalId,
    event: ApprovalRequestedEvent,
    is_selected: bool,
}

#[derive(Clone)]
struct HistorySnapshot {
    approval_id: ApprovalId,
    tool_name: String,
    title: String,
    decision: HistoryDecision,
}

#[derive(Clone, Copy)]
enum HistoryDecision {
    Approved,
    Denied,
    Failed,
    /// `PendingResponse` — server hasn't acked yet ("delegate" badge).
    Delegate,
}

impl HistoryDecision {
    fn glyph(self) -> &'static str {
        match self {
            Self::Approved => "✓",
            Self::Denied => "✗",
            Self::Failed => "!",
            Self::Delegate => "…",
        }
    }
}

/// CodingScreen widget. Reads `APP_STATE` (approvals/tasks) +
/// `CODING_VIEW_STATE` (selection/output buffers); writes via folds.
#[derive(Script, ScriptHook, Widget)]
pub struct CodingScreen {
    #[deref]
    view: View,
}

impl Widget for CodingScreen {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let (pending, history, selected_event, selected_task_buf) = collect_view_data();

        self.view
            .view(cx, ids!(queue_empty))
            .set_visible(cx, pending.is_empty());

        let active_page = active_page_id(selected_event.as_ref(), selected_task_buf.is_some());
        let preview_flip = self.view.page_flip(cx, ids!(preview_flip));
        preview_flip.set_active_page(cx, active_page);

        if let Some(ev) = selected_event.as_ref() {
            populate_preview(cx, &self.view, ev);
        } else if let Some(buf) = selected_task_buf.as_ref() {
            populate_output_tail(cx, &self.view, buf);
        }

        // PortalList has no `has_template` in this fork, so we compare
        // widget uids during step iteration to route queue vs history.
        let pending_uid = self.view.portal_list(cx, ids!(pending_list)).widget_uid();
        let history_uid = self.view.portal_list(cx, ids!(history_list)).widget_uid();

        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            let item_uid = item.widget_uid();
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                if item_uid == pending_uid {
                    list.set_item_range(cx, 0, pending.len());
                    while let Some(item_id) = list.next_visible_item(cx) {
                        let Some(snap) = pending.get(item_id) else { continue };
                        let item_widget = list.item(cx, item_id, id!(Row));
                        populate_queue_row(cx, &item_widget, snap);
                        item_widget.draw_all_unscoped(cx);
                    }
                } else if item_uid == history_uid {
                    list.set_item_range(cx, 0, history.len());
                    while let Some(item_id) = list.next_visible_item(cx) {
                        let Some(snap) = history.get(item_id) else { continue };
                        let item_widget = list.item(cx, item_id, id!(HistoryRow));
                        populate_history_row(cx, &item_widget, snap);
                        item_widget.draw_all_unscoped(cx);
                    }
                }
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            let pending_list = self.view.portal_list(cx, ids!(pending_list));
            if pending_list.any_items_with_actions(actions) {
                let pending_ids = collect_pending_ids();
                for (item_id, item) in pending_list.items_with_actions(actions) {
                    let Some(id) = pending_ids.get(item_id).cloned() else { continue };
                    if item.button(cx, ids!(select_button)).clicked(actions) {
                        Cx::post_action(CodingUiAction::SelectApproval(id));
                    }
                }
            }
            let history_list = self.view.portal_list(cx, ids!(history_list));
            if history_list.any_items_with_actions(actions) {
                let history_ids = collect_history_ids();
                for (item_id, item) in history_list.items_with_actions(actions) {
                    let Some(id) = history_ids.get(item_id).cloned() else { continue };
                    if item.button(cx, ids!(history_button)).clicked(actions) {
                        Cx::post_action(CodingUiAction::SelectHistory(id));
                    }
                }
            }
        }
    }
}

fn collect_view_data() -> (
    Vec<PendingSnapshot>,
    Vec<HistorySnapshot>,
    Option<ApprovalRequestedEvent>,
    Option<TaskOutputBuffer>,
) {
    let Ok(state) = APP_STATE.read() else {
        return (Vec::new(), Vec::new(), None, None);
    };
    let view_state = CODING_VIEW_STATE.read().ok();
    let selected_id = view_state.as_ref().and_then(|v| v.selected_approval.clone());
    let selected_task = view_state.as_ref().and_then(|v| v.selected_task.clone());
    let selected_buf = selected_task
        .as_ref()
        .and_then(|t| view_state.as_ref().and_then(|v| v.output_buffers.get(t).cloned()));

    let mut pending = Vec::new();
    for id in &state.approvals.pending_order {
        let Some(ev) = state.approvals.by_id.get(id) else { continue };
        pending.push(PendingSnapshot {
            approval_id: id.clone(),
            event: ev.clone(),
            is_selected: selected_id.as_ref() == Some(id),
        });
    }

    let mut history = Vec::new();
    for (id, st) in state.approvals.state.iter() {
        let (decision, want) = match st {
            ApprovalState::Decided { decision: ApprovalDecision::Approve } => {
                (HistoryDecision::Approved, true)
            }
            ApprovalState::Decided { decision: ApprovalDecision::Deny } => {
                (HistoryDecision::Denied, true)
            }
            // Forward-compat per spec § 4.1: an unrecognised decision string
            // fails closed — render it in the history as Denied so the user
            // doesn't see a permissive "approved" for an unknown variant.
            ApprovalState::Decided { decision: ApprovalDecision::Unknown(_) } => {
                (HistoryDecision::Denied, true)
            }
            ApprovalState::Failed(_) => (HistoryDecision::Failed, true),
            ApprovalState::PendingResponse { .. } => (
                HistoryDecision::Delegate,
                !state.approvals.pending_order.iter().any(|p| p == id),
            ),
            ApprovalState::Awaiting => (HistoryDecision::Approved, false),
        };
        if !want {
            continue;
        }
        let Some(ev) = state.approvals.by_id.get(id) else { continue };
        history.push(HistorySnapshot {
            approval_id: id.clone(),
            tool_name: ev.tool_name.clone(),
            title: ev.title.clone(),
            decision,
        });
    }
    // HashMap iteration order — fine for M3; W06 brief flags ordering
    // as an open question (no timestamps yet on `by_id`).

    let selected_event = selected_id
        .as_ref()
        .and_then(|id| state.approvals.by_id.get(id).cloned());

    (pending, history, selected_event, selected_buf)
}

fn collect_pending_ids() -> Vec<ApprovalId> {
    let Ok(state) = APP_STATE.read() else { return Vec::new() };
    state.approvals.pending_order.clone()
}

fn collect_history_ids() -> Vec<ApprovalId> {
    let (_, history, _, _) = collect_view_data();
    history.into_iter().map(|h| h.approval_id).collect()
}

/// PageFlip key for the right pane. Falls back to `empty_page` when
/// nothing is selected, and `output_tail_page` when only a task is
/// focused.
fn active_page_id(ev: Option<&ApprovalRequestedEvent>, has_task: bool) -> LiveId {
    if let Some(ev) = ev {
        let kind = ev
            .typed_details
            .as_ref()
            .map(|td| td.kind.as_str())
            .or(ev.approval_kind.as_deref())
            .unwrap_or("");
        return match kind {
            approval_kinds::DIFF => live_id!(diff_page),
            approval_kinds::COMMAND => live_id!(command_page),
            approval_kinds::NETWORK => live_id!(network_page),
            approval_kinds::FILESYSTEM => live_id!(filesystem_page),
            _ => live_id!(empty_page),
        };
    }
    if has_task {
        return live_id!(output_tail_page);
    }
    live_id!(empty_page)
}

fn populate_queue_row(cx: &mut Cx, item: &WidgetRef, snap: &PendingSnapshot) {
    let risk = snap.event.risk.as_deref().unwrap_or("");
    item.label(cx, ids!(row_risk_badge.risk_label))
        .set_text(cx, &risk.to_uppercase());
    item.view(cx, ids!(row_risk_badge))
        .set_visible(cx, !risk.is_empty());
    item.label(cx, ids!(row_tool_label))
        .set_text(cx, &snap.event.tool_name);
    item.label(cx, ids!(row_title_label))
        .set_text(cx, &snap.event.title);
    // Age placeholder — ApprovalRequestedEvent has no timestamp, so we
    // surface the kind tag for at-a-glance scanning instead.
    let kind_tag = snap
        .event
        .typed_details
        .as_ref()
        .map(|td| td.kind.clone())
        .or_else(|| snap.event.approval_kind.clone())
        .unwrap_or_default();
    item.label(cx, ids!(row_age_label)).set_text(cx, &kind_tag);
    item.view(cx, ids!(row_selected_marker))
        .set_visible(cx, snap.is_selected);
}

fn populate_history_row(cx: &mut Cx, item: &WidgetRef, snap: &HistorySnapshot) {
    let summary = format!(
        "{} {} · {}",
        snap.decision.glyph(),
        snap.tool_name,
        snap.title
    );
    item.label(cx, ids!(history_summary)).set_text(cx, &summary);
}

fn populate_preview(cx: &mut Cx, view: &View, ev: &ApprovalRequestedEvent) {
    let Some(td) = ev.typed_details.as_ref() else { return };
    match td.kind.as_str() {
        approval_kinds::COMMAND => populate_command_pane(cx, view, ev, td),
        approval_kinds::DIFF => populate_diff_pane(cx, view, ev, td),
        approval_kinds::NETWORK => populate_network_pane(cx, view, ev, td),
        approval_kinds::FILESYSTEM => populate_filesystem_pane(cx, view, ev, td),
        // Unknown kind — sub-view stays empty (forward-compat per
        // `03-PROTOCOL-CONTRACT.md` § Capability negotiation).
        _ => {}
    }
}

fn populate_command_pane(
    cx: &mut Cx, view: &View, ev: &ApprovalRequestedEvent, td: &ApprovalTypedDetails,
) {
    let Some(cmd) = td.command.as_ref() else { return };
    view.label(cx, ids!(command_title)).set_text(cx, &ev.title);
    let line = cmd.command_line.clone().unwrap_or_else(|| cmd.argv.join(" "));
    view.widget(cx, ids!(command_line_view)).set_text(cx, &line);
    let cwd = cmd.cwd.as_deref().unwrap_or("");
    view.label(cx, ids!(command_cwd_label))
        .set_text(cx, &if cwd.is_empty() { String::new() } else { format!("cwd: {cwd}") });
    let env = if cmd.env_keys.is_empty() {
        String::new()
    } else {
        format!("env: {}", cmd.env_keys.join(", "))
    };
    view.label(cx, ids!(command_env_label)).set_text(cx, &env);
}

fn populate_diff_pane(
    cx: &mut Cx, view: &View, ev: &ApprovalRequestedEvent, td: &ApprovalTypedDetails,
) {
    let Some(d) = td.diff.as_ref() else { return };
    let summary = d.summary.clone().unwrap_or_else(|| {
        let files = d.file_count.unwrap_or(0);
        let adds = d.additions.unwrap_or(0);
        let dels = d.deletions.unwrap_or(0);
        format!("{files} file(s), +{adds} / -{dels}")
    });
    view.label(cx, ids!(diff_summary_label)).set_text(cx, &summary);
    let op = d.operation.clone().unwrap_or_default();
    view.label(cx, ids!(diff_op_label))
        .set_text(cx, &if op.is_empty() { String::new() } else { format!("op: {op}") });
    // Per-file hunks via `FetchDiffPreview` (ui_protocol.rs:628) is M3.5.
    let body = format!("preview_id: {}\n\n{}", d.preview_id.0, ev.body);
    view.widget(cx, ids!(diff_hunks_view)).set_text(cx, &body);
    view.label(cx, ids!(diff_files_label))
        .set_text(cx, "(diff hunks land in M3.5 — wire FetchDiffPreview)");
}

fn populate_network_pane(
    cx: &mut Cx, view: &View, ev: &ApprovalRequestedEvent, td: &ApprovalTypedDetails,
) {
    let Some(n) = td.network.as_ref() else { return };
    view.label(cx, ids!(network_title)).set_text(cx, &ev.title);
    view.label(cx, ids!(network_op_label)).set_text(cx, &format!("op: {}", n.operation));
    let mut hosts = n.hosts.join(", ");
    if !n.ports.is_empty() {
        let ports = n.ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ");
        hosts = if hosts.is_empty() { format!("ports: {ports}") } else { format!("{hosts}  ports: {ports}") };
    }
    view.label(cx, ids!(network_hosts_label)).set_text(cx, &hosts);
    view.widget(cx, ids!(network_urls_view)).set_text(cx, &n.urls.join("\n"));
}

fn populate_filesystem_pane(
    cx: &mut Cx, view: &View, ev: &ApprovalRequestedEvent, td: &ApprovalTypedDetails,
) {
    let Some(fs) = td.filesystem.as_ref() else { return };
    view.label(cx, ids!(filesystem_title)).set_text(cx, &ev.title);
    view.label(cx, ids!(filesystem_op_label)).set_text(cx, &format!("op: {}", fs.operation));
    view.widget(cx, ids!(filesystem_paths_view)).set_text(cx, &fs.paths.join("\n"));
    let warn = if fs.outside_workspace { "writes outside workspace" } else { "" };
    view.label(cx, ids!(filesystem_warn_label)).set_text(cx, warn);
}

fn populate_output_tail(cx: &mut Cx, view: &View, buf: &TaskOutputBuffer) {
    view.widget(cx, ids!(output_tail_view))
        .set_text(cx, &buf.text);
    let cursor_msg = match buf.last_cursor {
        Some(c) => format!("offset: {} · {} bytes buffered", c.offset, buf.text.len()),
        None => format!("{} bytes buffered", buf.text.len()),
    };
    view.label(cx, ids!(output_tail_meta))
        .set_text(cx, &cursor_msg);
}

// ---- App-side folds (called from `handle_actions`) --------------------

/// Update selection; clear any focused task so the PageFlip leaves the
/// output tail and routes by the approval kind.
pub fn fold_select_approval(approval_id: ApprovalId) {
    if let Ok(mut s) = CODING_VIEW_STATE.write() {
        s.selected_approval = Some(approval_id);
        s.selected_task = None;
    }
}

/// Switch to task output drill-down (clears the approval focus).
pub fn fold_select_task(task_id: TaskId) {
    if let Ok(mut s) = CODING_VIEW_STATE.write() {
        s.selected_task = Some(task_id);
        s.selected_approval = None;
    }
}

/// Append the loaded chunk to the rolling buffer + advance the cursor.
pub fn fold_task_output(action: TaskOutputAction) {
    let TaskOutputAction { task_id, outcome, .. } = action;
    let TaskOutputOutcome::Loaded(res) = outcome else { return };
    if let Ok(mut s) = CODING_VIEW_STATE.write() {
        let buf = s.output_buffers.entry(task_id).or_default();
        buf.append(&res.text);
        buf.set_cursor(res.next_cursor);
    }
}

/// `task/output/read` params resuming from the last cursor. 4 KB limit
/// per W06 brief § "Task output drill-down".
pub fn build_output_read_params(
    session_id: SessionKey,
    task_id: TaskId,
) -> TaskOutputReadParams {
    let cursor = CODING_VIEW_STATE
        .read()
        .ok()
        .and_then(|s| s.output_buffers.get(&task_id).and_then(|b| b.last_cursor));
    TaskOutputReadParams {
        session_id,
        task_id,
        cursor,
        limit_bytes: Some(4_000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_buffer_caps_at_12kb() {
        let mut buf = TaskOutputBuffer::default();
        buf.append(&"x".repeat(20_000));
        assert!(buf.text.len() <= OUTPUT_TAIL_CAP_BYTES);
    }

    #[test]
    fn output_buffer_preserves_utf8_after_trim() {
        let mut buf = TaskOutputBuffer::default();
        // 2-byte UTF-8 char repeated past the cap.
        let s = "ñ".repeat(OUTPUT_TAIL_CAP_BYTES);
        buf.append(&s);
        // No invalid char split — text is still valid UTF-8 (rust's
        // `String` enforces this via the char-boundary trim).
        assert!(buf.text.len() <= OUTPUT_TAIL_CAP_BYTES);
        // Round-trip stays OK.
        assert!(buf.text.chars().all(|c| c == 'ñ'));
    }

    #[test]
    fn active_page_falls_back_to_empty_or_output() {
        // We don't construct an `ApprovalRequestedEvent` here (Uuid is
        // not a direct dep of `octos-app`). The two cheap branches
        // exercised below cover the no-approval case and the
        // task-output drill-down route.
        assert_eq!(active_page_id(None, false), live_id!(empty_page));
        assert_eq!(active_page_id(None, true), live_id!(output_tail_page));
    }
}
