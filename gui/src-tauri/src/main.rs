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
use tauri::{AppHandle, Emitter};

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
    let mut dir = start.parent().map(Path::to_path_buf);

    let marker_name = if cfg!(target_os = "windows") { "setup.ps1" } else { "setup.sh" };

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

fn run_step(app: &AppHandle, label: &str, mut cmd: Command) -> Result<bool, String> {
    emit_log(app, "meta", format!("--- {label} ---"));
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start {label}: {e}"))?;
    stream_child(app, child)
}

#[cfg(not(target_os = "windows"))]
fn run_linux(app: &AppHandle, repo_root: &Path, opts: &InstallOptions) -> Result<(), String> {
    // Only 01/02 touch the system (packages, systemd units) and need root;
    // 03/04 install models and Open WebUI for the invoking user and must
    // stay unprivileged, exactly like running the scripts by hand.
    let privileged: [(&str, &str, Vec<String>); 2] = [
        ("Installing Ollama (privileged)", "01-install-ollama.sh", vec![]),
        ("Configuring GPU (privileged)", "02-configure-gpu.sh", vec!["--no-tui".to_string()]),
    ];

    for (label, script, extra_args) in privileged {
        let mut cmd = Command::new("pkexec");
        cmd.current_dir(repo_root).arg(repo_root.join(script)).args(&extra_args);
        let ok = run_step(app, label, cmd)?;
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
        let ok = run_step(app, "Downloading models", cmd)?;
        if !ok {
            return Err("Model download failed, see the log above.".into());
        }
    }

    if !opts.skip_webui {
        let mut cmd = Command::new(repo_root.join("04-install-webui.sh"));
        cmd.current_dir(repo_root);
        let ok = run_step(app, "Installing Open WebUI", cmd)?;
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

    let ok = run_step(app, "Running setup.ps1 (elevated)", cmd)?;
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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_install])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
