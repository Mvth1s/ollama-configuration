# Ollama Launcher

A small day-to-day companion app, separate from [`gui/`](../gui/README.md) (which only handles the one-off install). It does not touch `setup.sh`/`setup.ps1` at all. Two jobs:

- **Open Web UI**: opens (or focuses) Open WebUI (`http://127.0.0.1:8080`) in its own window, same mechanism as `gui/`'s button.
- **Models**: lists installed Ollama models, pulls new ones, deletes old ones — talking directly to Ollama's local HTTP API (`http://127.0.0.1:11434`), not through Open WebUI's own admin panel.

## Architecture

- `src-tauri/` — Rust backend (`src/main.rs`), a separate Tauri app/binary from `gui/` (its own `Cargo.toml`, `tauri.conf.json`). No JS framework, no npm dependency: `dist/` is plain HTML/CSS/JS, `app.withGlobalTauri = true` exposes `window.__TAURI__` directly.
- `dist/` — the frontend: `index.html`, `main.js`, `style.css`.
- `list_models` / `pull_model` / `delete_model` call Ollama's REST API directly (`GET /api/tags`, `POST /api/pull`, `DELETE /api/delete`) using `reqwest::blocking` (plain HTTP to localhost only — TLS support is disabled at the Cargo feature level since it's never needed).
- `pull_model` streams Ollama's NDJSON progress response line-by-line and forwards each line as a `pull-progress` event, the same idiom `gui/` uses for streaming child-process output.
- Deleting a model asks for confirmation in the UI (`confirm()`) before calling `delete_model`, since it's irreversible.
- `open_webui_window` is a copy of `gui/`'s command (small enough that a shared crate wasn't worth the indirection for two call sites).

## Build & run

Same requirements as `gui/`: Rust toolchain, and on Linux the WebKitGTK dev packages (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`). No Node.js/npm.

```bash
cd launcher/src-tauri
cargo run
```

Requires Ollama to be running (`systemctl status ollama` on Linux) for the model list/pull/delete to work; Open WebUI running for the "Open Web UI" button to show anything but a connection error.

## Known limitations

- Verified against a real, already-running Ollama instance: the model list renders real installed models correctly. Pull and delete were **not** exercised for real (would have modified/removed real local models) — reviewed by reading the code and by testing the equivalent streaming pattern already used in `gui/`.
- No automated tests.
