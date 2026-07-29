//! Demonstrates, by actually spawning a process (not just inspecting a
//! `Command` value), that the whole privileged phase now costs exactly
//! **one** `pkexec` invocation - the concrete claim this phase exists to
//! deliver, replacing `gui/src-tauri/src/main.rs::run_linux`'s current
//! three separate `pkexec` calls (`01-install-ollama.sh`,
//! `02-configure-gpu.sh`, `04-install-webui.sh --install-deps`).
//!
//! Uses a fake `pkexec` script (logs each invocation's full argument list
//! to a file, prints a couple of lines to stdout so streaming is exercised
//! too, exits 0) resolved via `spawn_privileged_phase`'s `path_override`
//! parameter - scoped to the one spawned child, never by mutating this
//! test process's own `PATH` (which would race against other tests running
//! in parallel threads). Runs with no real privileges: this never touches
//! a real `pkexec`/polkit.

use selfllama_installer::privileged::{spawn_privileged_phase, PhaseLine, PrivilegedPhaseOptions};
use std::fs;
use std::path::Path;

/// Writes a fake, executable `pkexec` into `dir`, logging every invocation
/// (one line per call: the full argument list, space-joined) to `log_path`,
/// and returns `dir` formatted as a `PATH` value - the fake is the only
/// entry, so there is no ambiguity with a real `pkexec` that might also be
/// installed on the machine running this test (unlike `PATH` prepending,
/// which would still search the rest of the real `PATH` if the fake
/// somehow failed to match).
fn fake_pkexec_path(dir: &Path, log_path: &Path) -> String {
    let script = format!(
        "#!/usr/bin/env bash\n\
         echo \"$*\" >> {log}\n\
         echo 'privileged worker stdout, line one'\n\
         echo 'privileged worker stdout, line two'\n\
         exit 0\n",
        log = shell_quote(log_path),
    );
    let script_path = dir.join("pkexec");
    fs::write(&script_path, script).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    fs::set_permissions(&script_path, perms).unwrap();

    // The fake dir goes first (so `pkexec` itself resolves to the fake, not
    // a real one this machine might also have installed), followed by the
    // real system dirs - needed for the script's own `#!/usr/bin/env bash`
    // shebang to resolve `env`/`bash`, not to find a real `pkexec`.
    format!("{}:/usr/bin:/bin", dir.to_string_lossy())
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[test]
fn exactly_one_pkexec_call_covers_the_whole_privileged_phase() {
    let tmp = std::env::temp_dir().join(format!("selfllama-pkexec-count-{}", std::process::id()));
    fs::create_dir_all(&tmp).unwrap();
    let log_path = tmp.join("pkexec-calls.log");
    let fake_bin_dir = tmp.join("bin");
    fs::create_dir_all(&fake_bin_dir).unwrap();
    let fake_path = fake_pkexec_path(&fake_bin_dir, &log_path);

    let current_exe = Path::new("/opt/selfllama/selfllama-installer");
    let handle = spawn_privileged_phase(current_exe, PrivilegedPhaseOptions::default(), Some(&fake_path))
        .expect("failed to spawn the (fake) privileged phase");

    let lines: Vec<PhaseLine> = handle.lines.iter().collect();
    let status = handle.wait();

    // Read the log before cleaning up the tmp dir it lives in.
    let log_contents = fs::read_to_string(&log_path).unwrap_or_default();
    fs::remove_dir_all(&tmp).ok();

    assert_eq!(status, Ok(()));

    // The actual claim this phase makes: one pkexec call, not three. Each
    // line in the log is one invocation; historically (gui/src-tauri's
    // current run_linux) this same overall install would have produced
    // three separate lines here, one per script.
    let invocations: Vec<&str> = log_contents.lines().collect();
    assert_eq!(
        invocations.len(),
        1,
        "expected exactly one pkexec invocation for the whole privileged phase, got {}: {:?}",
        invocations.len(),
        invocations
    );

    // And that one invocation targets this same binary with the sentinel
    // arg - not, say, three different scripts.
    assert_eq!(invocations[0], "/opt/selfllama/selfllama-installer --run-privileged-phase");

    // Streaming works end to end too, not just the process count: the fake
    // worker's stdout arrived back through the same mpsc channel a real
    // caller would drain to show live progress.
    assert_eq!(
        lines,
        vec![
            PhaseLine::Stdout("privileged worker stdout, line one".to_string()),
            PhaseLine::Stdout("privileged worker stdout, line two".to_string()),
        ]
    );
}

#[test]
fn skip_webui_is_forwarded_through_the_single_call_too() {
    let tmp = std::env::temp_dir().join(format!("selfllama-pkexec-skipwebui-{}", std::process::id()));
    fs::create_dir_all(&tmp).unwrap();
    let log_path = tmp.join("pkexec-calls.log");
    let fake_bin_dir = tmp.join("bin");
    fs::create_dir_all(&fake_bin_dir).unwrap();
    let fake_path = fake_pkexec_path(&fake_bin_dir, &log_path);

    let current_exe = Path::new("/opt/selfllama/selfllama-installer");
    let handle = spawn_privileged_phase(
        current_exe,
        PrivilegedPhaseOptions { skip_webui: true },
        Some(&fake_path),
    )
    .unwrap();
    let _ = handle.lines.iter().count();
    let status = handle.wait();

    let log_contents = fs::read_to_string(&log_path).unwrap_or_default();
    fs::remove_dir_all(&tmp).ok();

    assert_eq!(status, Ok(()));
    assert_eq!(
        log_contents.lines().collect::<Vec<_>>(),
        vec!["/opt/selfllama/selfllama-installer --run-privileged-phase --skip-webui"]
    );
}
