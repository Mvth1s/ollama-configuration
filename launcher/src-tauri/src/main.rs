// Day-to-day companion app, separate from gui/ (which only handles the
// one-off install). Talks directly to Ollama's local HTTP API to list,
// pull, and delete models, and opens Open WebUI in its own window. Does
// not go through Open WebUI's own admin panel for model management, and
// does not touch setup.sh/setup.ps1 at all.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
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

    Ok(parsed
        .models
        .into_iter()
        .map(|m| {
            let (parameter_size, quantization_level) = m
                .details
                .map(|d| (d.parameter_size.unwrap_or_default(), d.quantization_level.unwrap_or_default()))
                .unwrap_or_default();
            ModelInfo { name: m.name, size: m.size, modified_at: m.modified_at, parameter_size, quantization_level }
        })
        .collect())
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
        .invoke_handler(tauri::generate_handler![list_models, pull_model, delete_model, open_webui_window])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
