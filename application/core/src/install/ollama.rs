//! Ported from `scripts/linux/01-install-ollama.sh`: installs Ollama via
//! the official install script (identical on every distro, so unlike
//! `gpu.rs`/`webui.rs` there is no per-`DistroFamily` branching here at
//! all), then ensures the service is running.

use std::time::Duration;

/// The exact command `01-install-ollama.sh` runs when `ollama` isn't
/// already on `PATH` - identical across every distro. Kept as a single
/// shell pipeline (not split into `curl` args + `sh` args) because that's
/// literally how the script invokes it (`curl -fsSL ... | sh`), not a
/// two-step process a plan could meaningfully separate.
pub const OFFICIAL_INSTALL_COMMAND: &str = "curl -fsSL https://ollama.com/install.sh | sh";

/// `01-install-ollama.sh` polls `http://127.0.0.1:11434` up to 30 times,
/// sleeping 1s between attempts, before giving up with "Ollama did not
/// start in time." This is a real, measured wait (network/service startup
/// time), not evaluable ahead of time - the plan just carries the strategy
/// (URL, attempt count, interval), execution is `application::privileged`'s
/// job, not `core::install`'s.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadinessCheck {
    pub url: &'static str,
    pub max_attempts: u32,
    pub interval: Duration,
}

/// What to do about the Ollama systemd service once installed. Mirrors
/// `if command -v systemctl >/dev/null 2>&1; then ... else log_warn ...`:
/// on a systemd-less machine, the script does not attempt a fallback
/// service manager, it just tells the user to run `ollama serve` by hand.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStartPlan {
    SystemdEnableNow { unit: &'static str },
    ManualStartRequired { hint: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
pub struct OllamaInstallPlan {
    /// `Some(OFFICIAL_INSTALL_COMMAND)` when `ollama` isn't already on
    /// `PATH` (`command -v ollama` failed); `None` when it's already
    /// installed - matching `command -v ollama >/dev/null 2>&1 && log_ok
    /// "Ollama already installed" || (install)`. Idempotent by construction:
    /// re-running this plan builder against an already-installed machine
    /// never queues a reinstall.
    pub install_command: Option<&'static str>,
    pub service: ServiceStartPlan,
    pub readiness_check: ReadinessCheck,
}

/// `ollama_on_path`/`systemd_available` are facts the caller already has
/// (a real caller would get them via `Command::new("which").arg("ollama")`/
/// checking for `systemctl` on `PATH`) - this function makes no process or
/// filesystem call itself, same convention as `detect/`.
pub fn ollama_install_plan(ollama_on_path: bool, systemd_available: bool) -> OllamaInstallPlan {
    OllamaInstallPlan {
        install_command: if ollama_on_path { None } else { Some(OFFICIAL_INSTALL_COMMAND) },
        service: if systemd_available {
            ServiceStartPlan::SystemdEnableNow { unit: "ollama" }
        } else {
            ServiceStartPlan::ManualStartRequired { hint: "ollama serve" }
        },
        readiness_check: ReadinessCheck {
            url: "http://127.0.0.1:11434",
            max_attempts: 30,
            interval: Duration::from_secs(1),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_installed_skips_the_install_command() {
        let plan = ollama_install_plan(true, true);
        assert_eq!(plan.install_command, None);
    }

    #[test]
    fn missing_queues_the_official_install_script_verbatim() {
        let plan = ollama_install_plan(false, true);
        assert_eq!(plan.install_command, Some("curl -fsSL https://ollama.com/install.sh | sh"));
    }

    #[test]
    fn systemd_present_enables_and_starts_the_unit() {
        let plan = ollama_install_plan(false, true);
        assert_eq!(plan.service, ServiceStartPlan::SystemdEnableNow { unit: "ollama" });
    }

    #[test]
    fn no_systemd_falls_back_to_a_manual_start_hint_not_a_crash() {
        let plan = ollama_install_plan(false, false);
        assert_eq!(plan.service, ServiceStartPlan::ManualStartRequired { hint: "ollama serve" });
    }

    #[test]
    fn readiness_check_matches_the_scripts_30_attempts_1s_apart() {
        let plan = ollama_install_plan(true, true);
        assert_eq!(
            plan.readiness_check,
            ReadinessCheck {
                url: "http://127.0.0.1:11434",
                max_attempts: 30,
                interval: Duration::from_secs(1),
            }
        );
    }
}
