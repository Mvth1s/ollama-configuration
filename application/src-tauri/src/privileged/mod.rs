//! Single-elevation-prompt privileged install phase.
//!
//! **Why this exists**: `gui/src-tauri/src/main.rs::run_linux` currently
//! spawns `pkexec` three separate times (`01-install-ollama.sh`,
//! `02-configure-gpu.sh`, `04-install-webui.sh --install-deps`), each its
//! own `unix-process` subject as far as polkit is concerned - three
//! authentication prompts in the worst case. Phase 2 of this restructure
//! confirmed empirically (see CLAUDE.md's Phase-2-derived note in the
//! Desktop GUI section, and the `setsid()`/polkit investigation) that
//! `org.freedesktop.policykit.exec` declares plain `auth_admin`, not
//! `auth_admin_keep`, on every distro this project targets - there is no
//! session-level caching trick available to reduce this from the app side.
//! The only reliable fix is architectural: **one** `pkexec` call covering
//! the whole privileged phase.
//!
//! **How**: rather than building and bundling a second binary just for the
//! privileged worker, this same binary re-invokes itself under `pkexec`
//! with a sentinel argument (`RUN_PRIVILEGED_PHASE_ARG`). `main.rs` checks
//! for that argument before doing anything else and, if present, runs
//! [`run_privileged_worker`] instead of starting the (not-yet-wired-up,
//! see the crate-level `application/src-tauri` note in CLAUDE.md) Tauri
//! app. The worker runs [`steps::default_privileged_steps`] in order via
//! [`steps::run_steps`], reporting each transition back to the parent as a
//! `protocol::StepEvent` line on its own stdout - the same pipe `pkexec`
//! already has open back to the unprivileged parent, no new IPC mechanism
//! needed.
//!
//! **Windows is explicitly out of scope for this module**: `setup.ps1` is
//! already invoked as a whole via a single `Start-Process -Verb RunAs` in
//! `gui/src-tauri/src/main.rs::run_windows` - UAC elevation keeps the same
//! user account, so there is no equivalent "three separate elevated
//! processes" problem to fix there. Nothing in this module runs on Windows.

mod exec;
pub mod protocol;
pub mod steps;

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;

pub use protocol::{StepEvent, StepStatus};
pub use steps::{default_privileged_steps, run_steps, PrivilegedStep, StepFailure};

/// Sentinel argument that makes this binary act as the privileged worker
/// instead of starting (once wired up, in a later phase) the Tauri app.
/// Also what `build_privileged_phase_command` passes to the re-invoked,
/// `pkexec`'d copy of this same binary.
pub const RUN_PRIVILEGED_PHASE_ARG: &str = "--run-privileged-phase";
const SKIP_WEBUI_ARG: &str = "--skip-webui";
const CONFIRM_NVIDIA_DRIVER_INSTALL_ARG: &str = "--confirm-nvidia-driver-install";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrivilegedPhaseOptions {
    pub skip_webui: bool,
    /// Added for `gui/`'s integration (Phase 7): a plan can require real
    /// user confirmation before installing an Nvidia driver
    /// (`core::install::gpu::NvidiaPlan.confirmation`, surfaced as
    /// `steps::StepRunError::NeedsConfirmation` - see `steps::gpu::run`).
    /// Since one worker process only ever gets one shot at the whole
    /// sequence, there is no way to "pause mid-process and resume" -  a
    /// caller that already asked the user and got a real "yes" sets this to
    /// `true` on a **second** `spawn_privileged_phase` call so that this
    /// run actually installs the driver instead of stopping at the same
    /// confirmation point again. Defaults to `false`: a fresh run always
    /// asks first, never silently assumes consent.
    pub confirm_nvidia_driver_install: bool,
}

impl PrivilegedPhaseOptions {
    fn to_args(self) -> Vec<String> {
        let mut args = Vec::new();
        if self.skip_webui {
            args.push(SKIP_WEBUI_ARG.to_string());
        }
        if self.confirm_nvidia_driver_install {
            args.push(CONFIRM_NVIDIA_DRIVER_INSTALL_ARG.to_string());
        }
        args
    }

    /// Parses the args a worker process was invoked with (everything after
    /// [`RUN_PRIVILEGED_PHASE_ARG`] itself).
    pub fn from_args(args: &[String]) -> Self {
        PrivilegedPhaseOptions {
            skip_webui: args.iter().any(|a| a == SKIP_WEBUI_ARG),
            confirm_nvidia_driver_install: args.iter().any(|a| a == CONFIRM_NVIDIA_DRIVER_INSTALL_ARG),
        }
    }
}

/// Builds the **one** `pkexec` command covering the whole privileged phase:
/// `pkexec <current_exe> --run-privileged-phase [--skip-webui]`. `pkexec`'s
/// target is `current_exe` itself (this same binary, re-invoked), not a
/// second binary - nothing extra to build, bundle, or code-sign. This
/// function only builds the `Command`; it does not spawn it, so its shape
/// (one program, one set of args) can be asserted on directly in a test
/// without ever touching a real process - see `tests::builds_a_single_*`.
pub fn build_privileged_phase_command(current_exe: &Path, opts: PrivilegedPhaseOptions) -> Command {
    let mut cmd = Command::new("pkexec");
    cmd.arg(current_exe);
    cmd.arg(RUN_PRIVILEGED_PHASE_ARG);
    cmd.args(opts.to_args());
    cmd
}

/// `pkexec` normally shows a graphical polkit-agent prompt, but if no agent
/// is registered for the session it silently falls back to reading the
/// password from `/dev/tty` - bypassing `Stdio::null()` (which only covers
/// stdin, not `/dev/tty`) and leaking a password prompt into whatever
/// terminal launched the app. Detaching the child into its own session
/// before exec removes its controlling terminal entirely, so that fallback
/// can't happen. Direct port of `gui/src-tauri/src/main.rs::detach_from_tty`
/// - Phase 2 of this restructure confirmed empirically (real `pkexec`/
/// polkit runs, `/proc/self/cgroup` and `/proc/self/sessionid` compared
/// with and without `setsid()`) that this has no effect on the
/// logind/cgroup session polkit actually keys its authorization subject on,
/// so it carries over unchanged into the single-`pkexec`-call design here.
#[cfg(unix)]
fn detach_from_tty(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
}

/// A line of the privileged phase's combined output, tagged by which pipe
/// it came from - same distinction `gui/src-tauri`'s `LogLine.stream`
/// already makes for the unprivileged steps, kept here so a future caller
/// can render stdout/stderr differently if desired.
#[derive(Debug, Clone, PartialEq)]
pub enum PhaseLine {
    Stdout(String),
    Stderr(String),
}

/// A spawned privileged phase: the child process, and a channel that
/// receives every line of its output (ANSI-aware, via
/// `core::progress::stream_ansi_aware`) as it's produced, from two reader
/// threads - same two-threads-to-avoid-pipe-buffer-deadlock shape as
/// `gui/src-tauri`'s `stream_child`, adapted to a channel instead of direct
/// Tauri event emission since this crate has no `AppHandle` yet.
pub struct PrivilegedPhaseHandle {
    child: Child,
    pub lines: mpsc::Receiver<PhaseLine>,
    out_thread: thread::JoinHandle<()>,
    err_thread: thread::JoinHandle<()>,
}

impl PrivilegedPhaseHandle {
    /// Waits for the child to exit and for both reader threads to finish
    /// draining their pipe (so no line is ever lost racing the exit
    /// status), then reports success/failure. Mirrors `stream_child`'s
    /// `child.wait()` + thread-join ordering.
    pub fn wait(mut self) -> Result<(), String> {
        let status =
            self.child.wait().map_err(|e| format!("failed to wait for privileged phase: {e}"))?;
        if let Err(e) = self.out_thread.join() {
            eprintln!("privileged phase stdout reader thread panicked: {e:?}");
        }
        if let Err(e) = self.err_thread.join() {
            eprintln!("privileged phase stderr reader thread panicked: {e:?}");
        }
        if status.success() {
            Ok(())
        } else {
            Err(format!("privileged phase exited with {status}"))
        }
    }
}

/// Spawns the single `pkexec` call for the whole privileged phase and
/// starts streaming its output. Returns immediately with a
/// [`PrivilegedPhaseHandle`] whose `lines` receiver yields output as it
/// happens - a caller drains it live (to show progress) and calls
/// [`PrivilegedPhaseHandle::wait`] once done reading, or reads and waits
/// concurrently from another thread.
///
/// `path_override` is `None` in real use (the child inherits this
/// process's real `PATH`, where a real `pkexec` is expected to live) and
/// `Some(dir)` only in tests, which need `Command::new("pkexec")` to
/// resolve to a fake - scoped to this one spawned child via
/// `Command::env`, never by mutating this process's own environment
/// (`std::env::set_var` is process-global and would race against any other
/// test running in a different thread at the same time). `extra_env` is
/// the same idea generalized: empty in real use, and in tests that need
/// the privileged worker to see one of the other testability overrides
/// this crate/`core` already define (`OS_RELEASE_FILE`, `ROCM_OPT_DIR`,
/// `OLLAMA_SERVICE_OVERRIDE_PATH`, ...) without mutating this process's own
/// environment either - a fake `pkexec` that `exec`s its arguments (rather
/// than just logging them, like `tests/single_pkexec_call.rs`'s does)
/// re-invokes this same binary for real, which inherits whatever `Command`
/// set here, scoped to that one process tree.
pub fn spawn_privileged_phase(
    current_exe: &Path,
    opts: PrivilegedPhaseOptions,
    path_override: Option<&str>,
    extra_env: &[(&str, &str)],
) -> std::io::Result<PrivilegedPhaseHandle> {
    let mut cmd = build_privileged_phase_command(current_exe, opts);
    if let Some(path) = path_override {
        cmd.env("PATH", path);
    }
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    #[cfg(unix)]
    detach_from_tty(&mut cmd);

    let mut child =
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

    let (tx, rx) = mpsc::channel();

    let stdout = child.stdout.take().expect("stdout was requested via Stdio::piped()");
    let tx_out = tx.clone();
    let out_thread = thread::spawn(move || {
        stream(stdout, |line| {
            let _ = tx_out.send(PhaseLine::Stdout(line));
        });
    });

    let stderr = child.stderr.take().expect("stderr was requested via Stdio::piped()");
    let err_thread = thread::spawn(move || {
        stream(stderr, |line| {
            let _ = tx.send(PhaseLine::Stderr(line));
        });
    });

    Ok(PrivilegedPhaseHandle { child, lines: rx, out_thread, err_thread })
}

fn stream<R: Read>(reader: R, on_line: impl FnMut(String)) {
    core::progress::stream_ansi_aware(reader, on_line);
}

/// The privileged worker's own entry point, run when this binary is
/// re-invoked (by [`build_privileged_phase_command`], via `pkexec`) with
/// [`RUN_PRIVILEGED_PHASE_ARG`]. Runs the real step sequence and returns the
/// process exit code `main.rs` should use.
pub fn run_privileged_worker(args: &[String]) -> i32 {
    let opts = PrivilegedPhaseOptions::from_args(args);
    let steps = default_privileged_steps(&opts);
    match run_steps(&steps) {
        Ok(()) => 0,
        Err(failure) => {
            eprintln!("privileged phase failed: {failure}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_single_pkexec_command_targeting_self_with_the_sentinel_arg() {
        let cmd = build_privileged_phase_command(Path::new("/usr/bin/selfllama-installer"), PrivilegedPhaseOptions::default());

        assert_eq!(cmd.get_program(), "pkexec");
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        // Exactly one program (pkexec) and one target (this binary) -
        // there is nothing here that could become three separate
        // invocations, unlike gui/src-tauri's current run_linux.
        assert_eq!(args, vec!["/usr/bin/selfllama-installer", "--run-privileged-phase"]);
    }

    #[test]
    fn passes_skip_webui_through_to_the_worker() {
        let cmd = build_privileged_phase_command(
            Path::new("/usr/bin/selfllama-installer"),
            PrivilegedPhaseOptions { skip_webui: true, ..Default::default() },
        );
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, vec!["/usr/bin/selfllama-installer", "--run-privileged-phase", "--skip-webui"]);
    }

    #[test]
    fn worker_side_parses_skip_webui_back_out_of_its_own_args() {
        let args = vec!["--skip-webui".to_string()];
        assert_eq!(PrivilegedPhaseOptions::from_args(&args), PrivilegedPhaseOptions { skip_webui: true, ..Default::default() });
        assert_eq!(PrivilegedPhaseOptions::from_args(&[]), PrivilegedPhaseOptions::default());
    }

    #[test]
    fn confirm_nvidia_driver_install_defaults_to_false_and_round_trips_through_args() {
        // A fresh run must never silently assume consent - see gui/'s
        // Phase 7 integration report and PrivilegedPhaseOptions's own doc
        // comment.
        assert!(!PrivilegedPhaseOptions::default().confirm_nvidia_driver_install);

        let opts = PrivilegedPhaseOptions { confirm_nvidia_driver_install: true, ..Default::default() };
        let cmd = build_privileged_phase_command(Path::new("/usr/bin/selfllama-installer"), opts);
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(
            args,
            vec!["/usr/bin/selfllama-installer", "--run-privileged-phase", "--confirm-nvidia-driver-install"]
        );
        // args[0] is the re-invoked binary's own path, args[1] is the
        // RUN_PRIVILEGED_PHASE_ARG sentinel itself - from_args only ever
        // sees what comes after both, same as main.rs's real dispatch.
        assert_eq!(PrivilegedPhaseOptions::from_args(&args[2..].to_vec()), opts);
    }

    #[test]
    fn run_privileged_worker_propagates_a_real_steps_failure_as_a_nonzero_exit() {
        // Phase 6 replaced the placeholder steps with real ones (see
        // steps/{ollama,gpu,webui}.rs) that really touch the system -
        // running this under `cargo test` (no root, no real pkexec)
        // reliably fails partway through (this machine's real Ollama is
        // already installed and running, so the "ollama" step succeeds
        // for real; the "gpu" step then really detects this machine's
        // real GPU and tries a real package-manager install, which fails
        // without root - confirmed by inspecting this test's own output).
        // The "every step succeeds end to end" scenario is what
        // tests/single_pkexec_call.rs's fake-pkexec-and-fake-tools harness
        // covers instead, without needing real root or touching the real
        // system.
        assert_eq!(run_privileged_worker(&[]), 1);
    }
}
