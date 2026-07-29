//! Low-level execution helpers shared by `steps/{ollama,gpu,webui}.rs`:
//! turning a `core::install` plan into real `Command`s, and running them.
//! Deliberately split into "build the commands" (pure, unit-testable
//! without spawning anything - same `Command`-returning convention as
//! `super::build_privileged_phase_command`) and "run them" (real I/O),
//! mirroring `application::privileged`'s own top-level split.
//!
//! Every command here inherits this process's stdio (`Stdio::inherit()`)
//! rather than being captured and manually re-printed: this process is
//! itself already running with its stdout/stderr piped back to the
//! unprivileged parent (see `super::spawn_privileged_phase`), so a real
//! package manager's own progress output (or lack of it - see CLAUDE.md's
//! Phase 2 note: pacman/dnf/zypper's network phase can go 30-100+s with no
//! output, and that is normal, not a hang) reaches the parent exactly as it
//! would in a real terminal, with zero extra buffering/relaying code and no
//! timeout of any kind layered on top.

use core::install::DistroFamily;
use std::process::{Command, Stdio};

/// `command -v NAME`'s Rust equivalent: a manual search of `PATH`'s
/// directories for an executable file named `name`, rather than shelling
/// out to `command`/`which` (a shell builtin and an extra process
/// respectively) - also means a test can control this purely by setting
/// `PATH`, without needing a fake `which` binary on it.
pub fn command_exists(name: &str) -> bool {
    resolve_on_path(name).is_some()
}

/// Like [`command_exists`], but returns the first matching absolute path
/// instead of just whether one exists - `command -v NAME`'s actual output,
/// not just its exit status. Needed wherever a plan wants the resolved
/// path itself, not just a yes/no (`rocminfo`'s location, for
/// `core::detect::gpu::resolve_rocminfo_path`'s `path_lookup` argument).
pub fn resolve_on_path(name: &str) -> Option<String> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find_map(|dir| {
        let candidate = dir.join(name);
        (candidate.is_file() && is_executable(&candidate)).then(|| candidate.to_string_lossy().into_owned())
    })
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Port of `pkg_install()` (`scripts/linux/lib/common.sh`), minus the
/// `sudo` prefix every call there needs: this process already runs as root
/// (inside the single `pkexec`'d worker), so there is nothing to elevate
/// further. Returns the ordered list of commands to run (`apt-get` needs
/// two: `update` then `install`) - pure, does not run anything, same
/// convention as `build_privileged_phase_command`. An empty `packages`
/// list or an unrecognized distro both correctly produce no commands at
/// all (mirrors the Bash `pkg_install`'s `*)` branch, which only logs a
/// warning and installs nothing automatically).
pub fn pkg_install_commands(distro: DistroFamily, packages: &[&str]) -> Vec<Command> {
    if packages.is_empty() {
        return Vec::new();
    }
    match distro {
        DistroFamily::Arch => {
            let mut cmd = Command::new("pacman");
            cmd.args(["-Sy", "--noconfirm", "--needed"]).args(packages);
            vec![cmd]
        }
        DistroFamily::Debian => {
            let mut update = Command::new("apt-get");
            update.args(["update", "-qq"]);
            let mut install = Command::new("apt-get");
            install.args(["install", "-y"]).args(packages);
            vec![update, install]
        }
        DistroFamily::Fedora => {
            let mut cmd = Command::new("dnf");
            cmd.args(["install", "-y"]).args(packages);
            vec![cmd]
        }
        DistroFamily::OpenSuse => {
            let mut cmd = Command::new("zypper");
            cmd.args(["--non-interactive", "install"]).args(packages);
            vec![cmd]
        }
        DistroFamily::Unknown => Vec::new(),
    }
}

/// Runs `commands` in order with inherited stdio, stopping at the first
/// non-zero exit (mirrors every `pkg_install`/script call in Bash running
/// under `set -e`: a failed package install aborts the step, it never
/// silently continues to the next command).
pub fn run_commands(commands: Vec<Command>) -> Result<(), String> {
    for mut cmd in commands {
        let program = cmd.get_program().to_string_lossy().into_owned();
        cmd.stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
        let status = cmd.status().map_err(|e| format!("failed to start {program}: {e}"))?;
        if !status.success() {
            return Err(format!("{program} exited with {status}"));
        }
    }
    Ok(())
}

/// `01-install-ollama.sh`'s official install command is a shell pipeline
/// (`curl ... | sh`), not a single executable + args - needs a real shell
/// to interpret the `|`, same as the Bash script itself running it.
pub fn run_shell(command: &str) -> Result<(), String> {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command).stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let status = cmd.status().map_err(|e| format!("failed to run '{command}': {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("'{command}' exited with {status}"))
    }
}

/// `curl -fs URL >/dev/null 2>&1` - the exact check `01-install-ollama.sh`
/// polls with. Shelling out to `curl` rather than adding an HTTP client
/// dependency: this crate has none today (`core`/`serde`/`serde_json`/
/// `libc` only), and the Bash script already made `curl` a hard
/// requirement (see CLAUDE.md's Requirements section), so this adds no new
/// one.
pub fn curl_ok(url: &str) -> bool {
    Command::new("curl")
        .args(["-fs", url])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Runs `program` with `args` and returns its captured stdout (lossily
/// decoded) - for the handful of cases that need to *read* a command's
/// output rather than just check its exit status (`lspci -nnk`, `cat
/// /etc/os-release`, `rocminfo`, `nvidia-smi --query-gpu=name`). `None` on
/// any failure to start or non-zero exit (mirrors the Bash scripts' own
/// `2>/dev/null || true` pattern: a missing tool degrades to "nothing
/// detected", never a hard error).
pub fn capture_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Reads `$OS_RELEASE_FILE` (defaulting to `/etc/os-release`) - same
/// testability override as `scripts/linux/lib/common.sh`'s
/// `OS_RELEASE_FILE`/`02-configure-gpu.sh`'s `ROCM_OPT_DIR`, so a test can
/// point this at a throwaway file instead of the real `/etc/os-release`.
pub fn read_os_release() -> Option<String> {
    let path = std::env::var("OS_RELEASE_FILE").unwrap_or_else(|_| "/etc/os-release".to_string());
    std::fs::read_to_string(path).ok()
}

/// Gathers the two facts `core::detect::gpu::resolve_rocminfo_path` needs
/// beyond a plain `PATH` lookup: whether `$ROCM_OPT_DIR/rocm/bin/rocminfo`
/// is executable, and the sorted list of executable
/// `$ROCM_OPT_DIR/rocm-*/bin/rocminfo` matches - `ROCM_OPT_DIR` defaults to
/// `/opt`, same override as `02-configure-gpu.sh`'s `find_rocminfo`, so a
/// test can point this at a throwaway directory instead of the real
/// `/opt`.
pub fn rocm_opt_candidates() -> (bool, Vec<String>) {
    let opt_dir = std::env::var("ROCM_OPT_DIR").unwrap_or_else(|_| "/opt".to_string());
    let plain = std::path::Path::new(&opt_dir).join("rocm/bin/rocminfo");
    let plain_executable = plain.is_file() && is_executable(&plain);

    let mut versioned: Vec<String> = std::fs::read_dir(&opt_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("rocm-"))
        .map(|entry| entry.path().join("bin/rocminfo"))
        .filter(|path| path.is_file() && is_executable(path))
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    versioned.sort();

    (plain_executable, versioned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_uses_a_single_pacman_call() {
        let commands = pkg_install_commands(DistroFamily::Arch, &["vulkan-radeon", "rocminfo"]);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].get_program(), "pacman");
        let args: Vec<String> = commands[0].get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, vec!["-Sy", "--noconfirm", "--needed", "vulkan-radeon", "rocminfo"]);
    }

    #[test]
    fn debian_updates_before_installing_two_separate_commands() {
        let commands = pkg_install_commands(DistroFamily::Debian, &["mesa-vulkan-drivers"]);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].get_program(), "apt-get");
        assert_eq!(
            commands[0].get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            vec!["update", "-qq"]
        );
        assert_eq!(commands[1].get_program(), "apt-get");
        assert_eq!(
            commands[1].get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            vec!["install", "-y", "mesa-vulkan-drivers"]
        );
    }

    #[test]
    fn fedora_uses_dnf_install_y() {
        let commands = pkg_install_commands(DistroFamily::Fedora, &["rocminfo"]);
        assert_eq!(commands[0].get_program(), "dnf");
        assert_eq!(
            commands[0].get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            vec!["install", "-y", "rocminfo"]
        );
    }

    #[test]
    fn opensuse_uses_zypper_non_interactive() {
        let commands = pkg_install_commands(DistroFamily::OpenSuse, &["vulkan-tools"]);
        assert_eq!(commands[0].get_program(), "zypper");
        assert_eq!(
            commands[0].get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            vec!["--non-interactive", "install", "vulkan-tools"]
        );
    }

    #[test]
    fn no_packages_produces_no_commands() {
        assert_eq!(pkg_install_commands(DistroFamily::Arch, &[]).len(), 0);
    }

    #[test]
    fn unknown_distro_produces_no_commands_matching_the_bash_manual_install_warning() {
        assert_eq!(pkg_install_commands(DistroFamily::Unknown, &["rocminfo"]).len(), 0);
    }

    #[test]
    fn command_exists_finds_a_real_binary_that_is_definitely_on_path() {
        // "sh" is a hard requirement of run_shell itself and of every CI
        // runner/dev machine this suite runs on.
        assert!(command_exists("sh"));
    }

    #[test]
    fn command_exists_is_false_for_something_that_cannot_plausibly_be_installed() {
        assert!(!command_exists("selfllama-installer-definitely-not-a-real-command-xyz"));
    }
}
