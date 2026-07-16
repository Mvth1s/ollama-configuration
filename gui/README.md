# Ollama Stack GUI

A thin [Tauri](https://tauri.app) desktop GUI over `setup.sh` / `setup.ps1`. It does not duplicate any GPU/RAM detection or model-tier logic: it only shows the same options as the CLI flags (`-Tier`/`--tier=`, skip models, skip Open WebUI), spawns the appropriate script for the running OS as a child process, and streams its stdout/stderr into a log pane in real time.

## Architecture

- `src-tauri/` — Rust backend (`src/main.rs`). No JS framework, no npm dependency for the frontend: `dist/` is plain HTML/CSS/JS served directly by Tauri (`app.withGlobalTauri = true` in `tauri.conf.json` exposes `window.__TAURI__` without an `@tauri-apps/api` import).
- `dist/` — the frontend: `index.html`, `main.js`, `style.css`.
- At startup, the backend walks up from the running executable's directory looking for `setup.sh` (Linux) / `setup.ps1` (Windows), so it finds the scripts whether run via `cargo run` (nested under `gui/src-tauri/target/...`) or as a standalone binary placed at the repository root.

### Privilege elevation

- **Linux**: only `01-install-ollama.sh` and `02-configure-gpu.sh` touch the system (packages, systemd units) and are run through `pkexec`. `03-pull-models.sh` and `04-install-webui.sh` install per-user state (`~/.ollama` data via the daemon, `pipx`, `~/.config/systemd/user/`) and are run unprivileged, exactly like running them by hand — wrapping the *whole* `setup.sh` in `pkexec` would run those as root too and misplace that per-user state.
- **Windows**: `setup.ps1` is a single script mixing both kinds of steps, but Windows UAC elevation (`Start-Process -Verb RunAs`) keeps the same user account and just raises the integration level, so running the whole script elevated does not have the same problem as Linux's `pkexec` (which switches to a different user, root). One elevation prompt is enough.

## Build & run

Requires a Rust toolchain (`cargo`, `rustc`). On Linux you also need the WebKitGTK development packages (Debian/Ubuntu: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`; equivalents on other distros). No Node.js/npm is required since the frontend has no build step.

```bash
cd gui/src-tauri
cargo run
```

For a distributable build (installer/AppImage/MSI), install the [Tauri CLI](https://tauri.app/reference/cli/) (`cargo install tauri-cli --version "^2"`) and run `cargo tauri build` from `gui/src-tauri`. This is not required for development; a plain `cargo run`/`cargo build` already produces a working binary.

## Known limitations

- Tested on Linux (window renders, backend compiles and runs); the Windows elevation/streaming path (`Start-Process -Verb RunAs` combined with output capture) has not been verified on real Windows hardware — see the caveats already noted for `setup.ps1` itself.
- No automated tests: this is a thin orchestration layer, verified by building and launching the app, and by reading through `setup.sh`/`setup.ps1`'s own behavior.
