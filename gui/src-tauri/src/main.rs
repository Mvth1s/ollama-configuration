// Orchestration only: this GUI never re-implements GPU/RAM/distro detection
// or model-tier selection. It locates the existing setup.sh / setup.ps1
// scripts next to the running executable, spawns them (splitting privileged
// from unprivileged steps on Linux, since a single pkexec over the whole
// setup.sh would run the unprivileged steps as root too and misplace
// per-user state such as ~/.config and the pipx install), and streams their
// stdout/stderr back to the frontend as Tauri events.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const WEBUI_URL: &str = "http://127.0.0.1:8080";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallOptions {
    tier: Option<String>,
    skip_models: bool,
    skip_webui: bool,
}

#[derive(Debug, Clone, Serialize)]
struct LogLine {
    stream: String, // "stdout" | "stderr" | "meta"
    text: String,
}

// Drives the frontend's step indicator. Linux gets one event per script
// (ollama/gpu/models/webui) since each runs as its own child process; setup.ps1
// runs as a single opaque elevated process on Windows, so it only ever gets
// one combined "windows" step — a real 4-step breakdown there would just be
// guesswork, since we can't see inside that process's own progress.
#[derive(Debug, Clone, Serialize)]
struct InstallStep {
    id: String,
    label: String,
    status: String, // "pending" | "running" | "done" | "failed" | "skipped"
}

fn emit_step(app: &AppHandle, id: &str, label: &str, status: &str) {
    let _ = app.emit("install-step", InstallStep { id: id.into(), label: label.into(), status: status.into() });
}

#[derive(Debug, Clone, Serialize)]
struct InstallDone {
    success: bool,
    message: String,
}

// Walks up from the running executable's directory looking for setup.sh
// (Linux) / setup.ps1 (Windows), so both `cargo run` (target/debug/ nested
// a few levels under gui/) and a future bundled app placed at the repo root
// resolve the scripts the same way.
fn find_repo_root() -> Result<PathBuf, String> {
    let start =
        std::env::current_exe().map_err(|e| format!("cannot resolve current executable path: {e}"))?;
    let start_dir = start.parent().map(Path::to_path_buf).ok_or_else(|| {
        format!("executable path {} has no parent directory", start.display())
    })?;

    let marker_name = if cfg!(target_os = "windows") { "setup.ps1" } else { "setup.sh" };
    find_marker_upwards(&start_dir, marker_name)
}

// Split out from find_repo_root so the walk-up logic can be exercised with a
// throwaway directory tree instead of the test binary's own real location.
fn find_marker_upwards(start_dir: &Path, marker_name: &str) -> Result<PathBuf, String> {
    let mut dir = Some(start_dir.to_path_buf());

    for _ in 0..8 {
        let Some(candidate) = dir.clone() else { break };
        if candidate.join(marker_name).is_file() {
            return Ok(candidate);
        }
        dir = candidate.parent().map(Path::to_path_buf);
    }

    Err(format!(
        "could not locate {marker_name} in any parent directory of the running executable"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_marker_in_starting_directory() {
        let tmp = std::env::temp_dir().join(format!("ollama-stack-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("setup.sh"), "").unwrap();

        let found = find_marker_upwards(&tmp, "setup.sh").unwrap();
        assert_eq!(found, tmp);

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn finds_marker_several_levels_up() {
        let base = std::env::temp_dir().join(format!("ollama-stack-test-nested-{}", std::process::id()));
        let nested = base.join("gui/src-tauri/target/debug");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(base.join("setup.sh"), "").unwrap();

        let found = find_marker_upwards(&nested, "setup.sh").unwrap();
        assert_eq!(found, base);

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn errors_when_marker_is_never_found() {
        let tmp = std::env::temp_dir().join(format!("ollama-stack-test-missing-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        let result = find_marker_upwards(&tmp, "setup.sh");
        assert!(result.is_err());

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}

fn emit_log(app: &AppHandle, stream: &str, text: String) {
    let _ = app.emit("install-log", LogLine { stream: stream.into(), text });
}

fn stream_child(app: &AppHandle, mut child: std::process::Child) -> Result<bool, String> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let app_out = app.clone();
    let out_handle = stdout.map(|s| {
        std::thread::spawn(move || {
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                emit_log(&app_out, "stdout", line);
            }
        })
    });

    let app_err = app.clone();
    let err_handle = stderr.map(|s| {
        std::thread::spawn(move || {
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                emit_log(&app_err, "stderr", line);
            }
        })
    });

    let status = child.wait().map_err(|e| format!("failed to wait for child process: {e}"))?;
    if let Some(h) = out_handle {
        let _ = h.join();
    }
    if let Some(h) = err_handle {
        let _ = h.join();
    }

    Ok(status.success())
}

// pkexec normally shows a graphical polkit-agent prompt, but if no agent is
// registered for the session it silently falls back to reading the
// password from /dev/tty — bypassing our own Stdio::null() (which only
// covers stdin, not /dev/tty) and leaking a password prompt into whatever
// terminal launched the GUI. Detaching the child into its own session
// before exec removes its controlling terminal entirely, so that fallback
// can't happen: pkexec then either uses the graphical agent or fails with
// a clear error we surface in the log, instead of silently prompting.
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

fn run_step(app: &AppHandle, id: &str, label: &str, mut cmd: Command) -> Result<bool, String> {
    emit_log(app, "meta", format!("--- {label} ---"));
    emit_step(app, id, label, "running");
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start {label}: {e}"))?;
    let ok = stream_child(app, child)?;
    emit_step(app, id, label, if ok { "done" } else { "failed" });
    Ok(ok)
}

#[cfg(not(target_os = "windows"))]
fn run_linux(app: &AppHandle, repo_root: &Path, opts: &InstallOptions) -> Result<(), String> {
    // Emitted up front so the stepper shows the full picture (including
    // skipped steps) before anything actually starts.
    emit_step(app, "ollama", "Installing Ollama", "pending");
    emit_step(app, "gpu", "Configuring GPU", "pending");
    emit_step(app, "models", "Downloading models", if opts.skip_models { "skipped" } else { "pending" });
    emit_step(app, "webui", "Installing Open WebUI", if opts.skip_webui { "skipped" } else { "pending" });

    // Only 01/02 touch the system (packages, systemd units) and need root;
    // 03/04 install models and Open WebUI for the invoking user and must
    // stay unprivileged, exactly like running the scripts by hand.
    let privileged: [(&str, &str, &str, Vec<String>); 2] = [
        ("ollama", "Installing Ollama", "01-install-ollama.sh", vec![]),
        ("gpu", "Configuring GPU", "02-configure-gpu.sh", vec!["--no-tui".to_string()]),
    ];

    for (id, label, script, extra_args) in privileged {
        emit_log(
            app,
            "meta",
            "This step needs administrator privileges — you may be prompted for your password.".into(),
        );
        let mut cmd = Command::new("pkexec");
        cmd.current_dir(repo_root).arg(repo_root.join(script)).args(&extra_args);
        detach_from_tty(&mut cmd);
        let ok = run_step(app, id, label, cmd)?;
        if !ok {
            return Err(format!("{label} failed, see the log above."));
        }
    }

    if !opts.skip_models {
        let mut cmd = Command::new(repo_root.join("03-pull-models.sh"));
        cmd.current_dir(repo_root).arg("--no-tui");
        if let Some(tier) = &opts.tier {
            cmd.arg(format!("--tier={tier}"));
        }
        let ok = run_step(app, "models", "Downloading models", cmd)?;
        if !ok {
            return Err("Model download failed, see the log above.".into());
        }
    }

    if !opts.skip_webui {
        let mut cmd = Command::new(repo_root.join("04-install-webui.sh"));
        cmd.current_dir(repo_root);
        let ok = run_step(app, "webui", "Installing Open WebUI", cmd)?;
        if !ok {
            return Err("Open WebUI installation failed, see the log above.".into());
        }
    }

    Ok(())
}

// setup.ps1 is a single script covering both privileged (winget/Ollama
// install) and unprivileged (model pull, Open WebUI) steps. Unlike pkexec on
// Linux, Windows UAC elevation keeps the same user account and just raises
// the integration level, so running the whole script elevated does not
// misplace per-user state the way running everything as root would on
// Linux, and a single elevation prompt is enough.
#[cfg(target_os = "windows")]
fn run_windows(app: &AppHandle, repo_root: &Path, opts: &InstallOptions) -> Result<(), String> {
    emit_step(app, "windows", "Running setup.ps1 (elevated)", "pending");

    let mut ps_args = vec![
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        repo_root.join("setup.ps1").to_string_lossy().to_string(),
    ];
    if opts.skip_models {
        ps_args.push("-SkipModels".to_string());
    }
    if opts.skip_webui {
        ps_args.push("-SkipWebui".to_string());
    }
    if let Some(tier) = &opts.tier {
        ps_args.push("-Tier".to_string());
        ps_args.push(tier.clone());
    }

    let arg_list = ps_args
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");

    let mut cmd = Command::new("powershell.exe");
    cmd.current_dir(repo_root).arg("-Command").arg(format!(
        "Start-Process -FilePath powershell.exe -Verb RunAs -Wait -ArgumentList {arg_list}"
    ));

    let ok = run_step(app, "windows", "Running setup.ps1 (elevated)", cmd)?;
    if !ok {
        return Err("setup.ps1 failed, see the log above.".into());
    }
    Ok(())
}

#[tauri::command]
fn run_install(app: AppHandle, options: InstallOptions) -> Result<InstallDone, String> {
    let repo_root = find_repo_root()?;
    emit_log(&app, "meta", format!("Repository root: {}", repo_root.display()));

    #[cfg(target_os = "windows")]
    let result = run_windows(&app, &repo_root, &options);
    #[cfg(not(target_os = "windows"))]
    let result = run_linux(&app, &repo_root, &options);

    match result {
        Ok(()) => {
            let done = InstallDone { success: true, message: "Installation complete.".into() };
            let _ = app.emit("install-done", done.clone());
            Ok(done)
        }
        Err(e) => {
            let done = InstallDone { success: false, message: e.clone() };
            let _ = app.emit("install-done", done);
            Err(e)
        }
    }
}

// Reuses the same Tauri app/window infrastructure rather than a separate
// webview tech (e.g. pywebview): just a second window pointed directly at
// Open WebUI, with no browser chrome (Tauri windows never have an address
// bar), identical on Linux and Windows since it only uses Tauri's own
// cross-platform window APIs.
#[tauri::command]
fn open_webui_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("webui") {
        return window.set_focus().map_err(|e| e.to_string());
    }

    let url = WEBUI_URL.parse().map_err(|e: url::ParseError| e.to_string())?;
    WebviewWindowBuilder::new(&app, "webui", WebviewUrl::External(url))
        .title("Open WebUI")
        .inner_size(1100.0, 800.0)
        .min_inner_size(480.0, 360.0)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_install, open_webui_window])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
