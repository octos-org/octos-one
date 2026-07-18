//! In-window app module. Splits out non-rendering concerns from the live-DSL
//! shell in `main.rs`.
//!
//! W02 keeps the live-DSL block in `main.rs` itself — separating it out into
//! a dedicated `shell.rs` is invasive (the `script_mod!{...}` block is
//! registered from `AppMain::script_mod`, which lives in `main.rs`) and
//! gains nothing for M1. When W04 / W05 grow, this module will own the
//! per-screen Rust glue.

pub mod approvals;
pub mod card_lint;
pub mod coding;
pub mod content_browser;
pub mod diagram_safety;
pub mod login;
pub mod octo_thinking;
pub mod producers;
pub mod sessions;
pub mod task_dock;
pub mod viewers;
