# Ollama Stack GUI

A [Tauri](https://tauri.app) desktop GUI over `setup.sh` / `setup.ps1`, presented as a 4-step wizard (Detection → Models → Installation → Done). It does not duplicate any GPU/RAM/CPU detection or model-tier logic: the detection screen and per-usage model picker are backed by real `--detect-only`/`-DetectOnly` calls into the same scripts (see [the repo-root `CLAUDE.md`](../CLAUDE.md#detection-only-mode-and-non-interactive-model-overrides-for-the-gui-wizard) for the JSON protocol), the install screen spawns the appropriate script for the running OS as a child process and streams its stdout/stderr into a log pane in real time. An "Open Web UI" button on the final screen opens Open WebUI (`http://127.0.0.1:8080`) in a second borderless-chrome window, reusing the same Tauri app instead of a separate webview technology.

## Architecture

- `src-tauri/` — Rust backend (`src/main.rs`). No JS framework, no npm dependency for the frontend: `dist/` is plain HTML/CSS/JS served directly by Tauri (`app.withGlobalTauri = true` in `tauri.conf.json` exposes `window.__TAURI__` without an `@tauri-apps/api` import).
- `dist/` — the frontend: `index.html`, `main.js`, `style.css`.
- At startup, the backend walks up from the running executable's directory looking for `setup.sh` (Linux) / `setup.ps1` (Windows), so it finds the scripts whether run via `cargo run` (nested under `gui/src-tauri/target/...`) or as a standalone binary placed at the repository root.

### Wizard steps

1. **Detection**: calls the `detect_system` command on mount (unprivileged — no `pkexec`/UAC prompt, since reading GPU/CPU/RAM/distro needs no elevated rights), and reveals the GPU/CPU/RAM/distro rows with a short staggered animation over the already-fetched real result.
2. **Models**: one card per usage (text/code/reasoning/embeddings), pre-filled with the tier's default model; a "Change" dropdown offers the script's real alternative candidates on Linux (hidden on Windows, which has no candidate list — see the CLAUDE.md link above).
3. **Installation**: real progress bar and log panel, driven by the same `install-step`/`install-log`/`install-done` events as before.
4. **Done**: summary of what was actually installed/skipped, and the "Open Web UI" button.

### Privilege elevation

- **Linux**: only `01-install-ollama.sh` and `02-configure-gpu.sh` touch the system (packages, systemd units) and are run through `pkexec`. `03-pull-models.sh` and `04-install-webui.sh` install per-user state (`~/.ollama` data via the daemon, `pipx`, `~/.config/systemd/user/`) and are run unprivileged, exactly like running them by hand — wrapping the *whole* `setup.sh` in `pkexec` would run those as root too and misplace that per-user state. The `pkexec` child is detached into its own session (`setsid`, in `detach_from_tty`) before it's spawned, so it has no controlling terminal: without an agent, `pkexec` normally falls back to prompting for the password on `/dev/tty`, which would otherwise leak into whatever terminal launched the GUI (e.g. via `cargo run`) instead of showing polkit's graphical prompt. With no controlling terminal, `pkexec` either uses the graphical agent or fails with a clear error surfaced in the log.
- **Windows**: `setup.ps1` is a single script mixing both kinds of steps, but Windows UAC elevation (`Start-Process -Verb RunAs`) keeps the same user account and just raises the integration level, so running the whole script elevated does not have the same problem as Linux's `pkexec` (which switches to a different user, root). One elevation prompt is enough.

### Open WebUI window

`open_webui_window` creates (or focuses, if already open) a second Tauri window pointed directly at `http://127.0.0.1:8080` via `WebviewUrl::External`. It works identically on Linux and Windows since it only uses Tauri's own cross-platform window APIs, with no OS-specific code. The button is always enabled — if Open WebUI isn't running yet, the window just shows a connection error, the same as opening the URL in any browser; there's no separate "is it up" check.

## Build & run

Requires a Rust toolchain (`cargo`, `rustc`). On Linux you also need the WebKitGTK development packages (Debian/Ubuntu: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`; equivalents on other distros). No Node.js/npm is required since the frontend has no build step.

```bash
cd gui/src-tauri
cargo run
```

For a distributable build (installer/AppImage/MSI), install the [Tauri CLI](https://tauri.app/reference/cli/) (`cargo install tauri-cli --version "^2"`) and run `cargo tauri build` from `gui/src-tauri`. This is not required for development; a plain `cargo run`/`cargo build` already produces a working binary.

## Known limitations

- Tested on Linux: the app was actually launched (not just compiled) against a real machine, and the Detection/Models screens were confirmed rendering real GPU/CPU/RAM/tier/candidate data end-to-end. The Windows elevation/streaming path (`Start-Process -Verb RunAs` combined with output capture) has not been verified on real Windows hardware — see the caveats already noted for `setup.ps1` itself.
- The Linux install path requires a polkit authentication agent running in the session (`polkit-gnome-authentication-agent-1`, `polkit-kde-authentication-agent-1`, `lxqt-policykit-agent`, ...; usually already running on GNOME/KDE/most desktop environments). Without one, `01-install-ollama.sh`/`02-configure-gpu.sh` will fail at the `pkexec` step with a clear error in the log rather than prompting anywhere — install an agent or run those two scripts directly from a terminal instead.
- The Installation and Done screens (steps 3-4) were reviewed by reading the code — they're driven by the same `install-step`/`install-log`/`install-done` events the previous flat-form UI already used — but were not interactively clicked through end-to-end (no click-simulation tool available in the environment this was built in).
- No automated UI tests: `src-tauri`'s pure logic (repo-root discovery, `__DETECT__` JSON parsing) has unit tests (`cargo test`); the frontend itself is verified by building, launching the app, and reading through its event handling.
