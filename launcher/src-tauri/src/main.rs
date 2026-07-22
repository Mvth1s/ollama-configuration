// Day-to-day companion app, separate from gui/ (which only handles the
// one-off install). Talks directly to Ollama's local HTTP API to list,
// pull, and delete models, and opens Open WebUI in its own window. Does
// not go through Open WebUI's own admin panel for model management, and
// does not touch setup.sh/setup.ps1 at all.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const OLLAMA_URL: &str = "http://127.0.0.1:11434";
const WEBUI_URL: &str = "http://127.0.0.1:8080";

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<RawModel>,
}

#[derive(Debug, Deserialize)]
struct RawModel {
    name: String,
    size: u64,
    modified_at: String,
    #[serde(default)]
    details: Option<RawDetails>,
}

#[derive(Debug, Default, Deserialize)]
struct RawDetails {
    parameter_size: Option<String>,
    quantization_level: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelInfo {
    name: String,
    size: u64,
    modified_at: String,
    parameter_size: String,
    quantization_level: String,
}

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

// Split out from list_models so the RawModel -> ModelInfo mapping (in
// particular the empty-string default when `details` is missing, which
// Ollama does for some model types) can be unit-tested without a live
// Ollama server.
fn to_model_info(m: RawModel) -> ModelInfo {
    let (parameter_size, quantization_level) = m
        .details
        .map(|d| (d.parameter_size.unwrap_or_default(), d.quantization_level.unwrap_or_default()))
        .unwrap_or_default();
    ModelInfo { name: m.name, size: m.size, modified_at: m.modified_at, parameter_size, quantization_level }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_full_details_from_ollamas_tags_response() {
        let json = r#"{
            "models": [{
                "name": "llama3.1:8b",
                "size": 4920753328,
                "modified_at": "2026-01-01T00:00:00Z",
                "details": { "parameter_size": "8B", "quantization_level": "Q4_0" }
            }]
        }"#;
        let parsed: TagsResponse = serde_json::from_str(json).unwrap();
        let info = to_model_info(parsed.models.into_iter().next().unwrap());

        assert_eq!(info.name, "llama3.1:8b");
        assert_eq!(info.size, 4920753328);
        assert_eq!(info.parameter_size, "8B");
        assert_eq!(info.quantization_level, "Q4_0");
    }

    #[test]
    fn defaults_to_empty_strings_when_details_is_missing() {
        let json = r#"{
            "models": [{
                "name": "custom-model:latest",
                "size": 123,
                "modified_at": "2026-01-01T00:00:00Z"
            }]
        }"#;
        let parsed: TagsResponse = serde_json::from_str(json).unwrap();
        let info = to_model_info(parsed.models.into_iter().next().unwrap());

        assert_eq!(info.parameter_size, "");
        assert_eq!(info.quantization_level, "");
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
// progress updates; we forward each as a "pull-progress" event rather than
// waiting for the whole download, the same streaming idiom used by the
// installer GUI for child-process output.
#[tauri::command]
fn pull_model(app: AppHandle, model: String) -> Result<(), String> {
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
            let _ = app.emit(
                "pull-progress",
                PullProgress { model: model.clone(), status: format!("error: {err}"), completed: None, total: None },
            );
            return Err(err);
        }

        let _ = app.emit(
            "pull-progress",
            PullProgress {
                model: model.clone(),
                status: parsed.status.unwrap_or_default(),
                completed: parsed.completed,
                total: parsed.total,
            },
        );
    }

    Ok(())
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

// Open WebUI's own `serve` command has no environment variable for its bind
// address, only a --host CLI flag, so toggling LAN access means touching
// whatever actually drives that flag: the small $HOME/.config/ollama-stack/
// webui.env file 04-install-webui.sh's systemd unit reads via
// EnvironmentFile= on Linux, and the 'OpenWebUI' scheduled task's own
// -Argument string on Windows (rebuilt via powershell.exe, mirroring
// toggle-webui-lan.ps1 — there is no equivalent to EnvironmentFile=
// substitution in Task Scheduler). Same read/write pair the standalone
// toggle-webui-lan.sh/.ps1 scripts use, reimplemented here rather than
// shelled out to, since the launcher is meant to keep working as a
// standalone packaged binary without the repo scripts nearby.

#[cfg(not(target_os = "windows"))]
fn webui_env_path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/ollama-stack/webui.env")
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn webui_lan_status() -> bool {
    std::fs::read_to_string(webui_env_path())
        .ok()
        .and_then(|content| content.lines().find_map(|line| line.strip_prefix("WEBUI_HOST=").map(str::trim)).map(str::to_string))
        .map(|host| host == "0.0.0.0")
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn set_webui_lan(enabled: bool) -> Result<String, String> {
    let host = if enabled { "0.0.0.0" } else { "127.0.0.1" };
    let path = webui_env_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, format!("WEBUI_HOST={host}\n")).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;

    let unit_installed = PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join(".config/systemd/user/open-webui.service")
        .exists();
    if !unit_installed {
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
$StateDir = Join-Path $env:APPDATA 'ollama-stack'
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
            set_webui_lan
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
