# Ollama Launcher

A small day-to-day companion app, separate from [`gui/`](../gui/README.md) (which only handles the one-off install). It does not touch `setup.sh`/`setup.ps1` at all. Jobs:

- **Open Web UI**: opens (or focuses) Open WebUI (`http://127.0.0.1:8080`) in its own window, same mechanism as `gui/`'s button.
- **Service control**: shows whether the Open WebUI service is active and lets you start/stop it, independently of the LAN setting below.
- **LAN access**: the same on/off toggle as the standalone `toggle-webui-lan.sh`/`.ps1` scripts, plus (when on) a shareable `http://<lan-ip>:8080` URL with a copy button and a QR code for scanning from a phone.
- **Models**: lists installed Ollama models, pulls new ones, deletes old ones — talking directly to Ollama's local HTTP API (`http://127.0.0.1:11434`), not through Open WebUI's own admin panel.

## Architecture

- `src-tauri/` — Rust backend (`src/main.rs`), a separate Tauri app/binary from `gui/` (its own `Cargo.toml`, `tauri.conf.json`). No JS framework, no npm dependency: `dist/` is plain HTML/CSS/JS, `app.withGlobalTauri = true` exposes `window.__TAURI__` directly.
- `dist/` — the frontend: `index.html`, `main.js`, `style.css`, and `qrcode.js` (a vendored copy of the MIT-licensed [`qrcode-generator`](https://github.com/kazuhikoarase/qrcode-generator) library, so the QR code renders with no network/CDN dependency).
- `list_models` / `pull_model` / `delete_model` call Ollama's REST API directly (`GET /api/tags`, `POST /api/pull`, `DELETE /api/delete`) using `reqwest::blocking` (plain HTTP to localhost only — TLS support is disabled at the Cargo feature level since it's never needed).
- `pull_model` streams Ollama's NDJSON progress response line-by-line and forwards each line as a `pull-progress` event, the same idiom `gui/` uses for streaming child-process output.
- Deleting a model asks for confirmation in the UI (`confirm()`) before calling `delete_model`, since it's irreversible.
- `webui_service_status` / `set_webui_service`: query/toggle the Open WebUI service's running state (`systemctl --user` on Linux, the `OpenWebUI` scheduled task's state on Windows) without touching the LAN host setting.
- `get_lan_url`: the machine's LAN IPv4/IPv6 address (via the `local-ip-address` crate) formatted as a full URL, for the shareable-link/QR feature.
- `open_webui_window` is a copy of `gui/`'s command (small enough that a shared crate wasn't worth the indirection for two call sites).

## Build & run

Same requirements as `gui/`: Rust toolchain, and on Linux the WebKitGTK dev packages (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`). No Node.js/npm.

```bash
cd launcher/src-tauri
cargo run
```

Requires Ollama to be running (`systemctl status ollama` on Linux) for the model list/pull/delete to work; Open WebUI running for the "Open Web UI" button to show anything but a connection error.

## Known limitations

- Verified against a real, already-running Ollama + Open WebUI instance on Linux: the model list, service status, and LAN toggle all rendered correctly against real local state. Pull and delete were **not** exercised for real (would have modified/removed real local models) — reviewed by reading the code and by testing the equivalent streaming pattern already used in `gui/`. The shareable-URL/QR feature was verified with the LAN toggle off; turning it on was reviewed by code but not re-screenshotted.
- The Windows service-status/LAN `powershell.exe` paths have not been verified on real Windows hardware.
- `cargo test` covers the pure logic (`to_model_info`, `parse_systemctl_is_active`, `build_lan_url`); no automated UI tests.
