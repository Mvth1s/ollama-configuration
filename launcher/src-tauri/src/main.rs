// Day-to-day companion app, separate from gui/ (which only handles the
// one-off install). Talks directly to Ollama's local HTTP API to list,
// pull, and delete models, and opens Open WebUI in its own window. Does
// not go through Open WebUI's own admin panel for model management, and
// does not touch setup.sh/setup.ps1 at all.

use launcher_core::{build_lan_url, parse_systemctl_is_active, to_model_info, ModelInfo, TagsResponse};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const OLLAMA_URL: &str = "http://127.0.0.1:11434";
const WEBUI_URL: &str = "http://127.0.0.1:8080";

#[tauri::command]
fn list_models() -> Result<Vec<ModelInfo>, String> {
    let resp = reqwest::blocking::get(format!("{OLLAMA_URL}/api/tags"))
        .map_err(|e| format!("Cannot reach Ollama at {OLLAMA_URL}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Ollama returned an error: {e}"))?;

    let parsed: TagsResponse =
        resp.json().map_err(|e| format!("Failed to parse Ollama's response: {e}"))?;

    Ok(parsed.models.into_iter().map(to_model_info).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Everything above (list_models/pull_model/delete_model) is exercised
    // indirectly through launcher-core's own unit tests (to_model_info,
    // parse_systemctl_is_active, build_lan_url, parse_webui_lan_status,
    // format_webui_env - see launcher/launcher-core/src/lib.rs), which run
    // without needing tauri/webkit2gtk at all. This test
    // instead exercises pull_model/list_models/delete_model against a REAL,
    // locally running Ollama daemon and its actual HTTP API - the gap the
    // roadmap flagged ("pull_model / delete_model jamais exercés pour de
    // vrai contre une instance Ollama"). Ignored by default: it needs a live
    // Ollama on 127.0.0.1:11434 and real network access to pull a model, so
    // normal `cargo test` (CI's test.yml, on every push/PR) never runs it.
    // Run explicitly with `cargo test -- --ignored`, which is what
    // .github/workflows/launcher-ollama-integration.yml does (workflow_dispatch
    // only, not on every push, since it downloads a real model). Uses
    // "all-minilm" (~45 MB, already referenced elsewhere in this repo as a
    // lightweight embeddings candidate) to keep the round trip fast, and
    // deletes it again at the end regardless of the machine's prior state.
    #[test]
    #[ignore]
    fn pull_list_and_delete_round_trip_against_live_ollama() {
        const TEST_MODEL: &str = "all-minilm";
        const TEST_MODEL_TAG: &str = "all-minilm:latest";

        let mut saw_progress = false;
        pull_model_inner(TEST_MODEL, |_progress| saw_progress = true)
            .expect("pull_model_inner should succeed against a live Ollama");
        assert!(saw_progress, "expected at least one progress update while pulling");

        let models = list_models().expect("list_models should succeed against a live Ollama");
        assert!(
            models.iter().any(|m| m.name == TEST_MODEL_TAG),
            "expected {TEST_MODEL_TAG} in list_models after pulling, got: {:?}",
            models.iter().map(|m| &m.name).collect::<Vec<_>>()
        );

        delete_model(TEST_MODEL.to_string()).expect("delete_model should succeed against a live Ollama");

        let models_after = list_models().expect("list_models should succeed after delete");
        assert!(
            !models_after.iter().any(|m| m.name == TEST_MODEL_TAG),
            "expected {TEST_MODEL_TAG} to be gone from list_models after delete_model"
        );
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PullProgress {
    model: String,
    status: String,
    completed: Option<u64>,
    total: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct PullLine {
    status: Option<String>,
    completed: Option<u64>,
    total: Option<u64>,
    error: Option<String>,
}

// Ollama's /api/pull streams one JSON object per line (NDJSON) with
// progress updates. The actual HTTP/NDJSON handling lives in
// pull_model_inner, split out from the #[tauri::command] so it can be
// integration-tested against a live Ollama server (see the #[ignore]d test
// below) without needing an AppHandle, which only exists inside a running
// Tauri app.
fn pull_model_inner(model: &str, mut on_progress: impl FnMut(PullProgress)) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("{OLLAMA_URL}/api/pull"))
        .json(&serde_json::json!({ "model": model, "stream": true }))
        .send()
        .map_err(|e| format!("Cannot reach Ollama at {OLLAMA_URL}: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama returned HTTP {}", resp.status()));
    }

    for line in BufReader::new(resp).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<PullLine>(&line) else {
            continue;
        };

        if let Some(err) = parsed.error {
            on_progress(PullProgress {
                model: model.to_string(),
                status: format!("error: {err}"),
                completed: None,
                total: None,
            });
            return Err(err);
        }

        on_progress(PullProgress {
            model: model.to_string(),
            status: parsed.status.unwrap_or_default(),
            completed: parsed.completed,
            total: parsed.total,
        });
    }

    Ok(())
}

// Forwards each progress update as a "pull-progress" event rather than
// waiting for the whole download, the same streaming idiom used by the
// installer GUI for child-process output.
#[tauri::command]
fn pull_model(app: AppHandle, model: String) -> Result<(), String> {
    pull_model_inner(&model, |progress| {
        let _ = app.emit("pull-progress", progress);
    })
}

#[tauri::command]
fn delete_model(model: String) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .delete(format!("{OLLAMA_URL}/api/delete"))
        .json(&serde_json::json!({ "model": model }))
        .send()
        .map_err(|e| format!("Cannot reach Ollama at {OLLAMA_URL}: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama returned HTTP {}", resp.status()));
    }
    Ok(())
}

// Service start/stop, independent of the LAN host setting above: unlike
// set_webui_lan, these never touch webui.env, they only start/stop the unit
// that's already configured.

#[cfg(not(target_os = "windows"))]
fn webui_unit_installed() -> bool {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".config/systemd/user/open-webui.service")
        .exists()
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn webui_service_status() -> Result<String, String> {
    if !webui_unit_installed() {
        return Ok("not-installed".into());
    }

    let output = Command::new("systemctl")
        .args(["--user", "is-active", "open-webui"])
        .output()
        .map_err(|e| format!("Failed to run systemctl --user is-active open-webui: {e}"))?;

    Ok(parse_systemctl_is_active(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn set_webui_service(enabled: bool) -> Result<String, String> {
    if !webui_unit_installed() {
        return Err("Open WebUI is not installed yet.".into());
    }

    let action = if enabled { "start" } else { "stop" };
    let status = Command::new("systemctl")
        .args(["--user", action, "open-webui"])
        .status()
        .map_err(|e| format!("Failed to run systemctl --user {action} open-webui: {e}"))?;

    if status.success() {
        Ok(if enabled { "Open WebUI started.".into() } else { "Open WebUI stopped.".into() })
    } else {
        Err(format!("systemctl --user {action} open-webui failed. Check `systemctl --user status open-webui`."))
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn webui_service_status() -> Result<String, String> {
    let script = r#"
$task = Get-ScheduledTask -TaskName 'OpenWebUI' -ErrorAction SilentlyContinue
if (-not $task) { Write-Output 'not-installed'; exit 0 }
switch ($task.State) {
    'Running'  { Write-Output 'active' }
    'Ready'    { Write-Output 'inactive' }
    'Disabled' { Write-Output 'inactive' }
    default    { Write-Output 'unknown' }
}
"#;
    run_powershell(script)
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn set_webui_service(enabled: bool) -> Result<String, String> {
    let action = if enabled { "Start-ScheduledTask" } else { "Stop-ScheduledTask" };
    let script = format!(
        r#"
$task = Get-ScheduledTask -TaskName 'OpenWebUI' -ErrorAction SilentlyContinue
if (-not $task) {{ Write-Output 'NOT_INSTALLED'; exit 0 }}
{action} -TaskName 'OpenWebUI' -ErrorAction SilentlyContinue
Write-Output 'OK'
"#
    );
    let out = run_powershell(&script)?;
    if out.contains("NOT_INSTALLED") {
        Err("Open WebUI is not installed yet.".into())
    } else {
        Ok(if enabled { "Open WebUI started.".into() } else { "Open WebUI stopped.".into() })
    }
}

#[tauri::command]
fn get_lan_url() -> Result<String, String> {
    let ip = local_ip_address::local_ip()
        .map_err(|e| format!("Could not determine a LAN address: {e}"))?;
    Ok(build_lan_url(ip))
}

// Open WebUI's own `serve` command has no environment variable for its bind
// address, only a --host CLI flag, so toggling LAN access means touching
// whatever actually drives that flag: the small $HOME/.config/selfllama/
// webui.env file 04-install-webui.sh's systemd unit reads via
// EnvironmentFile= on Linux, and the 'OpenWebUI' scheduled task's own
// -Argument string on Windows (rebuilt via powershell.exe, mirroring
// toggle-webui-lan.ps1 — there is no equivalent to EnvironmentFile=
// substitution in Task Scheduler). Same read/write pair the standalone
// toggle-webui-lan.sh/.ps1 scripts use, reimplemented here rather than
// shelled out to, since the launcher is meant to keep working as a
// standalone packaged binary without the repo scripts nearby.

// Pre-rename (ollama-configuration) location of the same directory -
// migrated wholesale the first time this (or any script sourcing
// lib/common.sh) touches the state dir, same reasoning as
// lib/common.sh's migrate_legacy_state_dir. The launcher can be opened
// without ever re-running setup.sh after an upgrade, so it needs this
// fallback itself rather than relying on the Bash scripts to have already
// migrated it.
#[cfg(not(target_os = "windows"))]
fn state_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let new_dir = PathBuf::from(&home).join(".config/selfllama");
    let legacy_dir = PathBuf::from(&home).join(".config/ollama-stack");
    if !new_dir.exists() && legacy_dir.is_dir() {
        let _ = std::fs::rename(&legacy_dir, &new_dir);
    }
    new_dir
}

#[cfg(not(target_os = "windows"))]
fn webui_env_path() -> PathBuf {
    state_dir().join("webui.env")
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn webui_lan_status() -> bool {
    std::fs::read_to_string(webui_env_path())
        .map(|content| launcher_core::parse_webui_lan_status(&content))
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn set_webui_lan(enabled: bool) -> Result<String, String> {
    let path = webui_env_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, launcher_core::format_webui_env(enabled))
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;

    if !webui_unit_installed() {
        return Ok("Saved. Open WebUI is not installed yet; this will apply automatically once you install it.".into());
    }

    Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .map_err(|e| format!("Failed to run systemctl --user daemon-reload: {e}"))?;

    let status = Command::new("systemctl")
        .args(["--user", "restart", "open-webui"])
        .status()
        .map_err(|e| format!("Failed to run systemctl --user restart open-webui: {e}"))?;

    if status.success() {
        Ok("Open WebUI restarted with the new setting.".into())
    } else {
        Err("systemctl --user restart open-webui failed. Check `systemctl --user status open-webui`.".into())
    }
}

#[cfg(target_os = "windows")]
fn run_powershell(script: &str) -> Result<String, String> {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
        .output()
        .map_err(|e| format!("Failed to run powershell.exe: {e}"))?;

    if !output.status.success() {
        return Err(format!("PowerShell command failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn webui_lan_status() -> bool {
    let script = r#"
$task = Get-ScheduledTask -TaskName 'OpenWebUI' -ErrorAction SilentlyContinue
if ($task) {
    $argStr = ($task.Actions | Select-Object -First 1).Arguments
    if ($argStr -match '--host\s+(\S+)') { Write-Output $Matches[1] } else { Write-Output '127.0.0.1' }
} else {
    Write-Output '127.0.0.1'
}
"#;
    run_powershell(script).map(|out| out == "0.0.0.0").unwrap_or(false)
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn set_webui_lan(enabled: bool) -> Result<String, String> {
    let host = if enabled { "0.0.0.0" } else { "127.0.0.1" };
    let script = format!(
        r#"
$StateDir = Join-Path $env:APPDATA 'selfllama'
$LegacyStateDir = Join-Path $env:APPDATA 'ollama-stack'
if (-not (Test-Path $StateDir) -and (Test-Path $LegacyStateDir)) {{
    New-Item -ItemType Directory -Path (Split-Path $StateDir -Parent) -Force | Out-Null
    Move-Item -Path $LegacyStateDir -Destination $StateDir
}}
$StateFile = Join-Path $StateDir 'state.env'
New-Item -ItemType Directory -Path $StateDir -Force | Out-Null
$existing = @()
if (Test-Path $StateFile) {{ $existing = @(Get-Content $StateFile | Where-Object {{ $_ -notmatch '^WebuiHost=' }}) }}
Set-Content -Path $StateFile -Value ($existing + ('WebuiHost="{host}"'))

$task = Get-ScheduledTask -TaskName 'OpenWebUI' -ErrorAction SilentlyContinue
if (-not $task) {{
    Write-Output 'NOT_INSTALLED'
    exit 0
}}
$webuiBin = ($task.Actions | Select-Object -First 1).Execute
$action = New-ScheduledTaskAction -Execute $webuiBin -Argument 'serve --port 8080 --host {host}'
Set-ScheduledTask -TaskName 'OpenWebUI' -Action $action | Out-Null
Stop-ScheduledTask -TaskName 'OpenWebUI' -ErrorAction SilentlyContinue
Start-ScheduledTask -TaskName 'OpenWebUI'
Write-Output 'OK'
"#
    );
    let out = run_powershell(&script)?;
    if out.contains("NOT_INSTALLED") {
        Ok("Saved. Open WebUI is not installed yet; this will apply automatically once you install it.".into())
    } else {
        Ok("Open WebUI restarted with the new setting.".into())
    }
}

// Same mechanism as gui/'s open_webui_window: a second Tauri window pointed
// directly at Open WebUI, no separate webview technology.
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
        .invoke_handler(tauri::generate_handler![
            list_models,
            pull_model,
            delete_model,
            open_webui_window,
            webui_lan_status,
            set_webui_lan,
            webui_service_status,
            set_webui_service,
            get_lan_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
