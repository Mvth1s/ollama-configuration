//! Executes `core::install::gpu`'s plans for real: gathers distro/GPU
//! vendor/gfx-code/Nvidia-driver facts (reusing `core::detect::gpu`/
//! `core::detect::distro`, exactly like the real `--detect-only` path
//! already does), builds the plan for whichever vendor was detected, and
//! runs it. The Nvidia "driver missing, confirmation required" case is
//! surfaced as `StepRunError::NeedsConfirmation` rather than resolved
//! either way - see this phase's report.

use super::super::exec::{
    command_exists, pkg_install_commands, read_os_release, resolve_on_path, rocm_opt_candidates, run_commands,
    capture_stdout,
};
use super::super::protocol::ConfirmationRequest;
use super::StepRunError;
use core::detect::distro::parse_distro;
use core::detect::gpu::{parse_amd_gfx, parse_gpu_from_lspci, resolve_rocminfo_path};
use core::install::gpu::{amd_plan, intel_plan, nvidia_plan, none_plan, DistroAction};
use core::install::{DistroFamily, OllamaServiceOverride};
use std::path::Path;
use std::process::Command;

/// `confirm_nvidia_driver_install`: `false` on a fresh run (the default -
/// see `PrivilegedPhaseOptions`), `true` only on a second, deliberate
/// re-invocation after a real caller (`gui/`, Phase 7) has actually shown
/// the user `core::install::gpu::NvidiaPlan.confirmation`'s prompt and
/// gotten a real "yes". This function never decides that answer itself.
pub fn run(confirm_nvidia_driver_install: bool) -> Result<(), StepRunError> {
    let distro_info = parse_distro(read_os_release().as_deref());
    let distro = DistroFamily::from(distro_info.family.as_str());

    let lspci_output = capture_stdout("lspci", &["-nnk"]).unwrap_or_default();
    let gpu = parse_gpu_from_lspci(&lspci_output);
    println!("GPU selected for acceleration: {} ({})", if gpu.name.is_empty() { "none" } else { &gpu.name }, gpu.vendor);

    let override_action = match gpu.vendor.as_str() {
        "nvidia" => run_nvidia(distro, confirm_nvidia_driver_install)?,
        "amd" => run_amd(distro)?,
        "intel" => run_intel(distro)?,
        _ => {
            println!("No dedicated GPU, Ollama will run on CPU.");
            none_plan()
        }
    };

    apply_override(&override_action)?;
    restart_ollama_if_active()?;

    println!("GPU configuration complete.");
    Ok(())
}

fn run_nvidia(distro: DistroFamily, confirm_nvidia_driver_install: bool) -> Result<OllamaServiceOverride, StepRunError> {
    let driver_present = command_exists("nvidia-smi");
    let driver_name = if driver_present {
        capture_stdout("nvidia-smi", &["--query-gpu=name", "--format=csv,noheader"])
            .map(|s| s.lines().next().unwrap_or("").trim().to_string())
    } else {
        None
    };

    let plan = nvidia_plan(distro, driver_present, driver_name);

    if let Some(name) = &plan.driver_already_present {
        println!("Nvidia driver already present: {name}");
    }

    if let Some(confirmation) = plan.confirmation {
        if !confirm_nvidia_driver_install {
            return Err(StepRunError::NeedsConfirmation(ConfirmationRequest {
                prompt_title: confirmation.prompt_title.to_string(),
                prompt_message: confirmation.prompt_message.to_string(),
                action_description: describe_action(&confirmation.action),
            }));
        }

        // Confirmed by a real caller on a prior attempt (see this
        // function's own doc comment) - actually do it now, matching
        // configure_nvidia's confirmed branch exactly (including
        // openSUSE's manual-step case, which still only logs a message
        // and installs nothing automatically).
        println!("Nvidia driver install confirmed - proceeding.");
        match &confirmation.action {
            DistroAction::InstallPackages(packages) => {
                run_commands(pkg_install_commands(distro, packages)).map_err(StepRunError::Failed)?;
            }
            DistroAction::ManualStepRequired(message) => {
                println!("{message}");
            }
        }
        if confirmation.reboot_required_after_install {
            println!("Reboot the machine then re-run the installer to finish GPU configuration.");
        }
    }

    Ok(plan.override_action)
}

fn run_amd(distro: DistroFamily) -> Result<OllamaServiceOverride, StepRunError> {
    // Package install happens before the gfx-based decision, exactly like
    // configure_amd (rocminfo itself is one of the packages just
    // installed, so it may only become resolvable on PATH after this).
    // amd_plan's `packages` field never depends on `gfx`, so calling it
    // once with `None` just to read the package list (rather than
    // exposing amd_packages() as a separate pub function core::install
    // didn't already choose to expose in Phase 5) is cheap and pure.
    let packages = amd_plan(distro, None).packages;
    run_commands(pkg_install_commands(distro, &packages)).map_err(StepRunError::Failed)?;

    let path_lookup = resolve_on_path("rocminfo");
    let (opt_bin_executable, versioned_candidates) = rocm_opt_candidates();
    let rocminfo_path = resolve_rocminfo_path(path_lookup.as_deref(), opt_bin_executable, &versioned_candidates);

    let gfx = match &rocminfo_path {
        Some(path) => capture_stdout(path, &[]).and_then(|output| parse_amd_gfx(&output)),
        None => None,
    };
    if gfx.is_none() {
        println!("GFX code not detected (rocminfo missing or chip not recognized).");
    } else {
        println!("AMD chip identified: {}", gfx.as_deref().unwrap());
    }

    let plan = amd_plan(distro, gfx.as_deref());
    Ok(plan.override_action)
}

fn run_intel(distro: DistroFamily) -> Result<OllamaServiceOverride, StepRunError> {
    let plan = intel_plan(distro);
    run_commands(pkg_install_commands(distro, &plan.packages)).map_err(StepRunError::Failed)?;
    Ok(plan.override_action)
}

fn describe_action(action: &DistroAction) -> String {
    match action {
        DistroAction::InstallPackages(packages) if packages.is_empty() => {
            "no automatic install available for this distro - install a driver manually".to_string()
        }
        DistroAction::InstallPackages(packages) => format!("install packages: {}", packages.join(", ")),
        DistroAction::ManualStepRequired(message) => message.to_string(),
    }
}

/// `/etc/systemd/system/ollama.service.d/override.conf` by default
/// (`core::install::OLLAMA_SERVICE_OVERRIDE_PATH`), overridable via
/// `$OLLAMA_SERVICE_OVERRIDE_PATH` so a test never needs to write to the
/// real path (which needs root and would affect the real system) - same
/// override-for-testability idiom as `OS_RELEASE_FILE`/`ROCM_OPT_DIR`,
/// applied here in `application/src-tauri` rather than in
/// `core::install` (whose constant stays the plain documented real path,
/// unmodified from Phase 5).
fn override_path() -> String {
    std::env::var("OLLAMA_SERVICE_OVERRIDE_PATH")
        .unwrap_or_else(|_| core::install::OLLAMA_SERVICE_OVERRIDE_PATH.to_string())
}

fn apply_override(action: &OllamaServiceOverride) -> Result<(), StepRunError> {
    let path = override_path();
    let path = Path::new(&path);

    match action {
        OllamaServiceOverride::Write(content) => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| StepRunError::Failed(format!("failed to create {}: {e}", dir.display())))?;
            }
            std::fs::write(path, content)
                .map_err(|e| StepRunError::Failed(format!("failed to write {}: {e}", path.display())))?;
        }
        OllamaServiceOverride::Clear => {
            if path.exists() {
                std::fs::remove_file(path)
                    .map_err(|e| StepRunError::Failed(format!("failed to remove {}: {e}", path.display())))?;
            }
        }
    }

    let mut reload = Command::new("systemctl");
    reload.args(["daemon-reload"]);
    run_commands(vec![reload]).map_err(StepRunError::Failed)
}

/// `if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet
/// ollama; then sudo systemctl restart ollama; fi` (`02-configure-gpu.sh`'s
/// last lines, outside of any vendor branch).
fn restart_ollama_if_active() -> Result<(), StepRunError> {
    if !command_exists("systemctl") {
        return Ok(());
    }
    let is_active = Command::new("systemctl").args(["is-active", "--quiet", "ollama"]).status();
    if matches!(is_active, Ok(status) if status.success()) {
        let mut restart = Command::new("systemctl");
        restart.args(["restart", "ollama"]);
        run_commands(vec![restart]).map_err(StepRunError::Failed)?;
    }
    Ok(())
}
