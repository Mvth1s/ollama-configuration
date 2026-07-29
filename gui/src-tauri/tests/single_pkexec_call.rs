//! Phase 7: proves the "one pkexec, not three" property specifically in
//! `gui/src-tauri`'s own context - not just relying on
//! `application/src-tauri`'s already-thorough proof of the same mechanism
//! for `selfllama-installer` re-invoking itself. What's genuinely new here
//! is `gui/src-tauri/src/main.rs`'s sentinel-argument dispatch (added this
//! phase, mirroring `selfllama-installer`'s own `main.rs`): without it,
//! `pkexec` re-invoking `ollama-stack-gui` would try to launch a second
//! full Tauri/GTK GUI as root instead of running the privileged worker
//! code. This test proves that dispatch actually works, end to end,
//! through a real (fake-tooled) process tree.
//!
//! Same harness idiom as `application/src-tauri/tests/
//! real_plans_end_to_end.rs`: a fake `pkexec` that really `exec`s its
//! arguments, re-invoking the real, freshly-`cargo test`-built
//! `ollama-stack-gui` binary (not this test binary - see
//! `installer_binary_path` below) with `--run-privileged-phase`, against a
//! `PATH` of fakes for every external tool the real steps shell out to. No
//! real root, no real pkexec, never touches the real system.

use ollama_stack_gui::run_privileged_phase_raw;
use selfllama_installer::privileged::PrivilegedPhaseOptions;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn write_script(path: &Path, body: &str) {
    fs::write(path, format!("#!/usr/bin/env bash\n{body}\n")).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

/// The real `ollama-stack-gui` `[[bin]]`, not this test's own binary - see
/// `application/src-tauri/tests/real_plans_end_to_end.rs`'s identical
/// helper for why `std::env::current_exe()` alone isn't enough here.
fn installer_binary_path() -> PathBuf {
    let test_exe = std::env::current_exe().expect("current_exe");
    let deps_dir = test_exe.parent().expect("deps dir");
    let target_debug = deps_dir.parent().expect("target/debug dir");
    let candidate = target_debug.join("ollama-stack-gui");
    assert!(
        candidate.is_file(),
        "expected the ollama-stack-gui binary at {} (built alongside this test binary by `cargo test`)",
        candidate.display()
    );
    candidate
}

#[test]
fn gui_re_invoking_itself_costs_exactly_one_pkexec_call() {
    let tmp = std::env::temp_dir().join(format!("gui-e2e-pkexec-{}", std::process::id()));
    fs::create_dir_all(&tmp).unwrap();
    let fake_bin = tmp.join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let rocm_opt_dir = tmp.join("opt-empty");
    fs::create_dir_all(&rocm_opt_dir).unwrap();

    let pkexec_log = tmp.join("pkexec-calls.log");
    let os_release = tmp.join("os-release");
    fs::write(&os_release, "ID=arch\nPRETTY_NAME=\"Arch Linux\"\n").unwrap();

    // Re-execs its arguments for real, after logging the call - counting
    // invocations and proving the real worker ran both come from this one
    // log/exit-status pair.
    write_script(
        &fake_bin.join("pkexec"),
        &format!("echo \"$*\" >> '{}'\nexec \"$@\"", pkexec_log.display()),
    );
    // No dedicated GPU (empty lspci output) and ollama/pipx already present
    // - the simplest branch through every step, since the point of this
    // test is the re-invocation mechanism itself, not re-proving
    // core::install's individual plan branches (already covered
    // exhaustively in application/src-tauri's own tests).
    write_script(&fake_bin.join("lspci"), "true");
    write_script(&fake_bin.join("ollama"), "exit 0");
    write_script(&fake_bin.join("systemctl"), "exit 0");
    write_script(&fake_bin.join("curl"), "exit 0");
    write_script(&fake_bin.join("pipx"), "exit 0");

    let path = format!("{}:/usr/bin:/bin", fake_bin.display());
    let os_release_str = os_release.to_string_lossy().into_owned();
    let rocm_opt_dir_str = rocm_opt_dir.to_string_lossy().into_owned();
    // The "no dedicated GPU" branch still clears any leftover override
    // file (see core::install::gpu::none_plan) - redirected away from the
    // real, root-only /etc/systemd/system path, same testability override
    // application/src-tauri's own tests already rely on.
    let override_path_str = tmp.join("override.conf").to_string_lossy().into_owned();

    let current_exe = installer_binary_path();
    let mut lines = Vec::new();
    let result = run_privileged_phase_raw(
        &current_exe,
        PrivilegedPhaseOptions::default(),
        Some(&path),
        &[
            ("OS_RELEASE_FILE", &os_release_str),
            ("ROCM_OPT_DIR", &rocm_opt_dir_str),
            ("OLLAMA_SERVICE_OVERRIDE_PATH", &override_path_str),
        ],
        |stream, text| lines.push(format!("[{stream}] {text}")),
    );

    let pkexec_calls = fs::read_to_string(&pkexec_log).unwrap_or_default();
    fs::remove_dir_all(&tmp).ok();

    assert_eq!(result, Ok(None), "full log:\n{}", lines.join("\n"));

    // The actual claim: gui/'s own re-invocation of itself, through its
    // own new main.rs dispatch, costs exactly one pkexec call - the
    // problem this whole restructure exists to fix (three separate
    // pkexec calls, one per script, before this phase).
    let invocations: Vec<&str> = pkexec_calls.lines().collect();
    assert_eq!(invocations.len(), 1, "expected exactly one pkexec invocation, got {}: {:?}", invocations.len(), invocations);
    assert_eq!(invocations[0], format!("{} --run-privileged-phase", current_exe.display()));

    // And it's really this GUI binary's own privileged-worker code that
    // ran (not a stub, not a no-op): every step reached "done".
    let joined = lines.join("\n");
    assert!(joined.contains(r#""id":"ollama","label":"Installing Ollama","status":"done""#), "{joined}");
    assert!(joined.contains(r#""id":"gpu","label":"Configuring GPU","status":"done""#), "{joined}");
    assert!(joined.contains(r#""id":"webui-deps","label":"Installing Open WebUI prerequisites","status":"done""#), "{joined}");
    assert!(joined.contains("No dedicated GPU, Ollama will run on CPU."), "{joined}");
}
