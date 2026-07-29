//! The sequence of privileged steps run inside the single `pkexec`'d worker
//! process, and the runner that executes them one at a time, stopping at
//! the first failure.
//!
//! **Stubs, deliberately**: what each step actually *does* (which packages,
//! which commands - the equivalent of `01-install-ollama.sh`,
//! `02-configure-gpu.sh`, and `04-install-webui.sh --install-deps`'s real
//! bodies) is `core::install`'s job, which does not exist yet - out of
//! scope for this phase on purpose (see CLAUDE.md). What *is* in scope
//! here, and what these stubs exist to prove, is the sequencing itself:
//! one process, steps run in order, a failing step stops the sequence
//! instead of silently continuing, and every transition is reported to the
//! parent via `protocol::StepEvent`.
//!
//! Step order mirrors `gui/src-tauri/src/main.rs::run_linux`'s *current*
//! order exactly (ollama, then gpu, then webui-deps) rather than the
//! "GPU, Ollama, webui" order this phase's own task prompt listed loosely -
//! that order is load-bearing, not arbitrary: `02-configure-gpu.sh`'s last
//! step (`systemctl restart ollama` if the service is already active)
//! assumes Ollama is already installed by the time it runs.

use super::protocol::{format_step_event, StepEvent, StepStatus};
use super::PrivilegedPhaseOptions;

pub struct PrivilegedStep {
    pub id: &'static str,
    pub label: &'static str,
    pub run: Box<dyn Fn() -> Result<(), String>>,
}

/// The real step sequence for a live run: ollama, then gpu, then
/// (unless `--skip-webui`) webui-deps - see the module doc comment for why
/// this order matters and why each step is a stub for now.
pub fn default_privileged_steps(opts: &PrivilegedPhaseOptions) -> Vec<PrivilegedStep> {
    let mut steps: Vec<PrivilegedStep> = vec![
        PrivilegedStep { id: "ollama", label: "Installing Ollama", run: Box::new(install_ollama_stub) },
        PrivilegedStep { id: "gpu", label: "Configuring GPU", run: Box::new(configure_gpu_stub) },
    ];
    if !opts.skip_webui {
        steps.push(PrivilegedStep {
            id: "webui-deps",
            label: "Installing Open WebUI prerequisites",
            run: Box::new(install_webui_deps_stub),
        });
    }
    steps
}

fn install_ollama_stub() -> Result<(), String> {
    println!("(stub) would install Ollama here - see core::install, not implemented yet");
    Ok(())
}

fn configure_gpu_stub() -> Result<(), String> {
    println!("(stub) would detect/configure the GPU here - see core::install, not implemented yet");
    Ok(())
}

fn install_webui_deps_stub() -> Result<(), String> {
    println!("(stub) would install Open WebUI prerequisites here - see core::install, not implemented yet");
    Ok(())
}

/// Failure of a single step: which one, and why. `Display`ed to the parent
/// as the privileged phase's overall error message.
#[derive(Debug, Clone, PartialEq)]
pub struct StepFailure {
    pub step_id: String,
    pub message: String,
}

impl std::fmt::Display for StepFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "step '{}' failed: {}", self.step_id, self.message)
    }
}

/// Runs `steps` in order, printing a `__STEP__` line (see `protocol`) before
/// and after each one. Stops at the first failure - later steps never run -
/// and returns which step failed and why, rather than continuing silently.
/// A caller that reads this process's stdout line-by-line (the parent, via
/// `super::run_privileged_phase_and_collect`, or a test) sees every
/// transition in real time, not just the final `Ok`/`Err`.
pub fn run_steps(steps: &[PrivilegedStep]) -> Result<(), StepFailure> {
    for step in steps {
        emit(StepEvent { id: step.id.into(), label: step.label.into(), status: StepStatus::Running, error: None });

        match (step.run)() {
            Ok(()) => {
                emit(StepEvent { id: step.id.into(), label: step.label.into(), status: StepStatus::Done, error: None });
            }
            Err(message) => {
                emit(StepEvent {
                    id: step.id.into(),
                    label: step.label.into(),
                    status: StepStatus::Failed,
                    error: Some(message.clone()),
                });
                return Err(StepFailure { step_id: step.id.to_string(), message });
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

    fn recording_step(id: &'static str, label: &'static str, log: Rc<RefCell<Vec<&'static str>>>, outcome: Result<(), &'static str>) -> PrivilegedStep {
        PrivilegedStep {
            id,
            label,
            run: Box::new(move || {
                log.borrow_mut().push(id);
                outcome.map_err(|e| e.to_string())
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
            recording_step("gpu", "Configuring GPU", log.clone(), Err("pacman exited with status 1")),
            recording_step("webui-deps", "Installing Open WebUI prerequisites", log.clone(), Ok(())),
        ];

        let result = run_steps(&steps);

        assert_eq!(
            result,
            Err(StepFailure { step_id: "gpu".to_string(), message: "pacman exited with status 1".to_string() })
        );
        // "webui-deps" must never appear: the GPU failure must not be
        // silently swallowed and followed by the next step anyway.
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
