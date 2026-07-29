//! The sequence of privileged steps run inside the single `pkexec`'d worker
//! process, and the runner that executes them one at a time, stopping at
//! the first failure (or unresolved confirmation - see `StepRunError`
//! below).
//!
//! **Phase 6 of the SelfLlama restructure**: each step now consumes the
//! real plans from `core::install::{ollama,gpu,webui}` (Phase 5) and
//! actually executes them (`ollama.rs`/`gpu.rs`/`webui.rs` in this module,
//! using `super::exec`'s command-building/running helpers) - no longer the
//! `println!`-only placeholders Phase 4 shipped to prove the sequencing
//! mechanism alone. Step order is unchanged from Phase 4 and still
//! load-bearing: ollama, then gpu, then (unless `--skip-webui`)
//! webui-deps - `02-configure-gpu.sh`'s last step restarts the `ollama`
//! service if already active, which assumes Ollama is already installed.

mod gpu;
mod ollama;
mod webui;

use super::protocol::{format_step_event, ConfirmationRequest, StepEvent, StepStatus};
use super::PrivilegedPhaseOptions;

/// What a step's `run` closure can report besides success. Kept distinct
/// from a single `String` error (Phase 4's shape) specifically so the
/// Nvidia GPU step's "driver missing, confirmation required" case (see
/// `gpu::run`, consuming `core::install::gpu::NvidiaPlan.confirmation`)
/// can be told apart from a real failure, both in this Rust type and in
/// the `__STEP__` event reported to the parent - see this phase's report
/// for why this is exposed rather than resolved one way or the other here.
#[derive(Debug, Clone, PartialEq)]
pub enum StepRunError {
    Failed(String),
    NeedsConfirmation(ConfirmationRequest),
}

pub struct PrivilegedStep {
    pub id: &'static str,
    pub label: &'static str,
    pub run: Box<dyn Fn() -> Result<(), StepRunError>>,
}

/// The real step sequence for a live run: ollama, then gpu, then (unless
/// `--skip-webui`) webui-deps.
pub fn default_privileged_steps(opts: &PrivilegedPhaseOptions) -> Vec<PrivilegedStep> {
    let mut steps: Vec<PrivilegedStep> = vec![
        PrivilegedStep { id: "ollama", label: "Installing Ollama", run: Box::new(ollama::run) },
        PrivilegedStep { id: "gpu", label: "Configuring GPU", run: Box::new(gpu::run) },
    ];
    if !opts.skip_webui {
        steps.push(PrivilegedStep {
            id: "webui-deps",
            label: "Installing Open WebUI prerequisites",
            run: Box::new(webui::run),
        });
    }
    steps
}

/// Failure (or unresolved confirmation) of a single step: which one, and
/// why - `confirmation` is `Some` exactly when the step returned
/// `StepRunError::NeedsConfirmation`, never alongside a "real" failure
/// message describing something that went wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct StepFailure {
    pub step_id: String,
    pub message: String,
    pub confirmation: Option<ConfirmationRequest>,
}

impl std::fmt::Display for StepFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.confirmation {
            Some(c) => write!(f, "step '{}' needs confirmation: {} ({})", self.step_id, c.prompt_message, c.action_description),
            None => write!(f, "step '{}' failed: {}", self.step_id, self.message),
        }
    }
}

/// Runs `steps` in order, printing a `__STEP__` line (see `protocol`)
/// before and after each one. Stops at the first failure or unresolved
/// confirmation - later steps never run - and returns which step stopped
/// the sequence and why, rather than continuing silently. A caller that
/// reads this process's stdout line-by-line (the parent, via
/// `super::spawn_privileged_phase`, or a test) sees every transition in
/// real time, not just the final `Ok`/`Err`.
pub fn run_steps(steps: &[PrivilegedStep]) -> Result<(), StepFailure> {
    for step in steps {
        emit(StepEvent {
            id: step.id.into(),
            label: step.label.into(),
            status: StepStatus::Running,
            error: None,
            confirmation: None,
        });

        match (step.run)() {
            Ok(()) => {
                emit(StepEvent {
                    id: step.id.into(),
                    label: step.label.into(),
                    status: StepStatus::Done,
                    error: None,
                    confirmation: None,
                });
            }
            Err(StepRunError::Failed(message)) => {
                emit(StepEvent {
                    id: step.id.into(),
                    label: step.label.into(),
                    status: StepStatus::Failed,
                    error: Some(message.clone()),
                    confirmation: None,
                });
                return Err(StepFailure { step_id: step.id.to_string(), message, confirmation: None });
            }
            Err(StepRunError::NeedsConfirmation(confirmation)) => {
                emit(StepEvent {
                    id: step.id.into(),
                    label: step.label.into(),
                    status: StepStatus::NeedsConfirmation,
                    error: None,
                    confirmation: Some(confirmation.clone()),
                });
                return Err(StepFailure {
                    step_id: step.id.to_string(),
                    message: format!("{}: {}", confirmation.prompt_title, confirmation.prompt_message),
                    confirmation: Some(confirmation),
                });
            }
        }
    }
    Ok(())
}

fn emit(event: StepEvent) {
    println!("{}", format_step_event(&event));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn recording_step(
        id: &'static str,
        label: &'static str,
        log: Rc<RefCell<Vec<&'static str>>>,
        outcome: Result<(), StepRunError>,
    ) -> PrivilegedStep {
        PrivilegedStep {
            id,
            label,
            run: Box::new(move || {
                log.borrow_mut().push(id);
                outcome.clone()
            }),
        }
    }

    #[test]
    fn runs_every_step_in_order_when_all_succeed() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let steps = vec![
            recording_step("ollama", "Installing Ollama", log.clone(), Ok(())),
            recording_step("gpu", "Configuring GPU", log.clone(), Ok(())),
            recording_step("webui-deps", "Installing Open WebUI prerequisites", log.clone(), Ok(())),
        ];

        assert_eq!(run_steps(&steps), Ok(()));
        assert_eq!(*log.borrow(), vec!["ollama", "gpu", "webui-deps"]);
    }

    #[test]
    fn stops_at_the_first_failure_and_never_runs_later_steps() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let steps = vec![
            recording_step("ollama", "Installing Ollama", log.clone(), Ok(())),
            recording_step(
                "gpu",
                "Configuring GPU",
                log.clone(),
                Err(StepRunError::Failed("pacman exited with status 1".to_string())),
            ),
            recording_step("webui-deps", "Installing Open WebUI prerequisites", log.clone(), Ok(())),
        ];

        let result = run_steps(&steps);

        assert_eq!(
            result,
            Err(StepFailure {
                step_id: "gpu".to_string(),
                message: "pacman exited with status 1".to_string(),
                confirmation: None,
            })
        );
        // "webui-deps" must never appear: the GPU failure must not be
        // silently swallowed and followed by the next step anyway.
        assert_eq!(*log.borrow(), vec!["ollama", "gpu"]);
    }

    #[test]
    fn a_needs_confirmation_step_also_stops_the_sequence_but_is_reported_distinctly() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let confirmation = ConfirmationRequest {
            prompt_title: "Nvidia driver".to_string(),
            prompt_message: "Install the Nvidia driver now? (requires a reboot afterwards)".to_string(),
            action_description: "install packages: nvidia, nvidia-utils".to_string(),
        };
        let steps = vec![
            recording_step("ollama", "Installing Ollama", log.clone(), Ok(())),
            recording_step(
                "gpu",
                "Configuring GPU",
                log.clone(),
                Err(StepRunError::NeedsConfirmation(confirmation.clone())),
            ),
            recording_step("webui-deps", "Installing Open WebUI prerequisites", log.clone(), Ok(())),
        ];

        let result = run_steps(&steps);

        let failure = result.expect_err("a needs-confirmation step must stop the sequence");
        assert_eq!(failure.step_id, "gpu");
        assert_eq!(failure.confirmation, Some(confirmation));
        assert_eq!(*log.borrow(), vec!["ollama", "gpu"]);
    }

    #[test]
    fn default_steps_skip_webui_deps_when_requested() {
        let opts = PrivilegedPhaseOptions { skip_webui: true };
        let ids: Vec<&str> = default_privileged_steps(&opts).iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["ollama", "gpu"]);
    }

    #[test]
    fn default_steps_include_webui_deps_by_default() {
        let opts = PrivilegedPhaseOptions::default();
        let ids: Vec<&str> = default_privileged_steps(&opts).iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["ollama", "gpu", "webui-deps"]);
    }

    #[test]
    fn step_order_matches_run_linuxs_current_order_ollama_then_gpu() {
        // Load-bearing, not arbitrary - see the module doc comment:
        // 02-configure-gpu.sh's last step restarts the ollama service if
        // already active, which assumes Ollama is already installed.
        let opts = PrivilegedPhaseOptions::default();
        let ids: Vec<&str> = default_privileged_steps(&opts).iter().map(|s| s.id).collect();
        assert_eq!(ids[0], "ollama");
        assert_eq!(ids[1], "gpu");
    }
}
