//! Executes `core::install::ollama`'s plan for real: gathers the facts it
//! needs (`ollama`/`systemctl` on `PATH`), builds the plan, then runs it.

use super::super::exec::{command_exists, curl_ok, run_commands, run_shell};
use super::StepRunError;
use core::install::ollama::{ollama_install_plan, ServiceStartPlan};
use std::process::Command;
use std::thread;

pub fn run() -> Result<(), StepRunError> {
    let ollama_on_path = command_exists("ollama");
    let systemd_available = command_exists("systemctl");
    let plan = ollama_install_plan(ollama_on_path, systemd_available);

    if let Some(install_command) = plan.install_command {
        println!("Installing Ollama...");
        run_shell(install_command).map_err(StepRunError::Failed)?;
    } else {
        println!("Ollama already installed.");
    }

    match plan.service {
        ServiceStartPlan::SystemdEnableNow { unit } => {
            let mut reload = Command::new("systemctl");
            reload.args(["daemon-reload"]);
            let mut enable = Command::new("systemctl");
            enable.args(["enable", "--now", unit]);
            run_commands(vec![reload, enable]).map_err(StepRunError::Failed)?;
        }
        ServiceStartPlan::ManualStartRequired { hint } => {
            println!("systemd not detected, start Ollama manually with: {hint}");
            // No readiness check makes sense without a service manager to
            // have started anything - matches 01-install-ollama.sh, which
            // only polls after the systemctl branch.
            return Ok(());
        }
    }

    println!("Waiting for Ollama service to start...");
    for _ in 0..plan.readiness_check.max_attempts {
        if curl_ok(plan.readiness_check.url) {
            println!("Ollama is ready at {}", plan.readiness_check.url);
            return Ok(());
        }
        thread::sleep(plan.readiness_check.interval);
    }

    Err(StepRunError::Failed(format!(
        "Ollama did not start in time (checked {} {} times, {:?} apart)",
        plan.readiness_check.url, plan.readiness_check.max_attempts, plan.readiness_check.interval
    )))
}
