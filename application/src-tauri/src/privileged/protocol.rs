//! Line-based protocol the privileged worker (child, running as root under
//! `pkexec`) uses to report step boundaries back to the unprivileged parent
//! over the same stdout pipe carrying its regular human-readable log output
//! - same idiom as `02-configure-gpu.sh`'s `__DETECT__{...}` line, just for
//! step transitions instead of a one-shot detection result. Kept in
//! `application/src-tauri` rather than `application/core` for this phase
//! only because touching `application/core` is out of scope here; it is a
//! pure parse/format pair like everything in `core::progress`, and would be
//! a reasonable candidate to move there once this design is proven out.
//!
//! Collapsing gui/src-tauri's three separate `pkexec` calls into one (see
//! `super::build_privileged_phase_command`) means the parent can no longer
//! tell which of the three privileged scripts is currently running just by
//! knowing which `pkexec` call is in flight - it needs the child to say so
//! explicitly. `__STEP__` lines carry that: `on_line` on the parent side
//! (see `super::run_privileged_phase_and_collect`) checks every line
//! against `parse_step_event` before treating it as plain log text.

use serde::{Deserialize, Serialize};

pub const STEP_MARKER: &str = "__STEP__";

/// `NeedsConfirmation` is distinct from `Failed` on purpose: it is not an
/// error, it is a real, unresolved decision the Nvidia GPU step can hit
/// (`core::install::gpu::NvidiaPlan.confirmation` - see
/// `steps/gpu.rs::run`), and this phase's task explicitly asked that it be
/// exposed clearly rather than silently resolved as either a yes or a no.
/// A step reporting this status still stops the sequence (see
/// `steps::run_steps`) - there is nothing sensible to run afterwards
/// without an answer - but a caller that only checked for `Failed` before
/// deciding "something went wrong" would be wrong to do so here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Running,
    Done,
    Failed,
    NeedsConfirmation,
}

/// The data a future interactive caller (gui/, out of scope for this
/// phase) would need to actually ask the user and re-invoke with a
/// decision - carries `core::install::gpu::NvidiaDriverConfirmation`'s
/// prompt text plus a human-readable summary of what confirming would do,
/// since `DistroAction` itself isn't serialized as structured data here
/// (a plain description is enough for a prompt; this phase does not invent
/// a re-invocation protocol for the eventual "yes" path, since resolving
/// this is explicitly not this phase's job).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationRequest {
    pub prompt_title: String,
    pub prompt_message: String,
    pub action_description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepEvent {
    pub id: String,
    pub label: String,
    pub status: StepStatus,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub confirmation: Option<ConfirmationRequest>,
}

/// `__STEP__{"id":"ollama","label":"Installing Ollama","status":"running"}`.
pub fn format_step_event(event: &StepEvent) -> String {
    format!("{STEP_MARKER}{}", serde_json::to_string(event).expect("StepEvent always serializes"))
}

/// `None` for any line that isn't a `__STEP__` line (the common case: most
/// child output is plain log text meant to be shown as-is) or whose JSON
/// fails to parse (defensive - a malformed line is treated as ordinary log
/// output rather than silently dropped or crashing the parent).
pub fn parse_step_event(line: &str) -> Option<StepEvent> {
    let json_str = line.strip_prefix(STEP_MARKER)?;
    serde_json::from_str(json_str).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_running_event() {
        let event = StepEvent {
            id: "ollama".into(),
            label: "Installing Ollama".into(),
            status: StepStatus::Running,
            error: None,
            confirmation: None,
        };
        let line = format_step_event(&event);
        assert_eq!(parse_step_event(&line), Some(event));
    }

    #[test]
    fn round_trips_a_failed_event_with_an_error_message() {
        let event = StepEvent {
            id: "gpu".into(),
            label: "Configuring GPU".into(),
            status: StepStatus::Failed,
            error: Some("pacman exited with status 1".into()),
            confirmation: None,
        };
        let line = format_step_event(&event);
        assert_eq!(parse_step_event(&line), Some(event));
    }

    #[test]
    fn round_trips_a_needs_confirmation_event() {
        let event = StepEvent {
            id: "gpu".into(),
            label: "Configuring GPU".into(),
            status: StepStatus::NeedsConfirmation,
            error: None,
            confirmation: Some(ConfirmationRequest {
                prompt_title: "Nvidia driver".into(),
                prompt_message: "Install the Nvidia driver now? (requires a reboot afterwards)".into(),
                action_description: "install packages: nvidia, nvidia-utils".into(),
            }),
        };
        let line = format_step_event(&event);
        assert_eq!(parse_step_event(&line), Some(event));
    }

    #[test]
    fn ordinary_log_lines_are_not_mistaken_for_step_events() {
        assert_eq!(parse_step_event("Installing Ollama..."), None);
        assert_eq!(parse_step_event("[INFO] some regular log output"), None);
    }

    #[test]
    fn malformed_step_line_is_treated_as_ordinary_text_not_a_crash() {
        assert_eq!(parse_step_event("__STEP__{not valid json"), None);
    }
}
