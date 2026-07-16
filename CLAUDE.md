# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project does

A set of Bash scripts that install and configure a local Ollama stack (LLM inference + Open WebUI) on Linux, plus a separate native PowerShell implementation (`setup.ps1` + `lib/common.ps1`) for Windows, plus two optional Tauri apps: `gui/` (one-off install) and `launcher/` (day-to-day: open Open WebUI, manage models). The four numbered Bash scripts can be run individually or chained via `setup.sh`; `setup.ps1` is the single Windows entry point (see [Windows script](#windows-script-setupps1--libcommonps1) below). The two script implementations are independent: the Windows script does not wrap or require WSL, and is not a literal port — it is adapted to Windows/PowerShell primitives (CIM, winget, scheduled tasks) with the same overall shape (logging, shared state, RAM/GPU detection, model tiers) kept in sync by hand. `gui/` (see [Desktop GUI](#desktop-gui-gui) below) is purely an orchestration layer over `setup.sh`/`setup.ps1`: it never reimplements detection or tier logic itself. `launcher/` (see [Ollama Launcher](#ollama-launcher-launcher) below) is a separate app that never touches the install scripts at all, talking directly to Ollama's and Open WebUI's own local APIs instead.

## Requirements

- Linux (Arch, Debian/Ubuntu, Fedora, openSUSE — other distros require manual GPU driver installation): bash ≥ 4.0 (associative arrays used throughout), `curl`, `sudo` privileges for package installation and systemd configuration
- Windows: PowerShell 5.1+, `winget` recommended (used to install Ollama/Python unattended; falls back to downloading the official installer otherwise)
- Desktop GUI (`gui/`, `launcher/`, both optional): Rust/`cargo`; on Linux, WebKitGTK dev packages (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev` on Debian/Ubuntu). No Node.js/npm needed — both frontends are plain HTML/JS with no build step.

## Running the scripts

```bash
# Full install (auto-detects GPU and RAM)
./setup.sh

# Force a specific model tier
./setup.sh --tier=M

# Skip model download or web UI
./setup.sh --skip-models
./setup.sh --skip-webui

# Run individual steps
./01-install-ollama.sh
./02-configure-gpu.sh
./03-pull-models.sh --tier=S
./04-install-webui.sh

# Disable the interactive dialog/whiptail menus
./setup.sh --no-tui
```

```powershell
# Windows: full install, auto-detection
.\setup.ps1

# Force a specific model tier
.\setup.ps1 -Tier M

# Skip model download or web UI
.\setup.ps1 -SkipModels
.\setup.ps1 -SkipWebui
```

There are no tests, build steps, or linters — these are operational shell/PowerShell scripts.

## Architecture

All scripts source `lib/common.sh` at startup, which provides:
- **Logging helpers**: `log_info`, `log_ok`, `log_warn`, `log_err`
- **Distro detection**: `detect_distro` sets `DISTRO_FAMILY` (arch / debian / fedora / opensuse / unknown) by reading `/etc/os-release` fields `ID` and `ID_LIKE`
- **Package installation**: `pkg_install <pkg>…` dispatches to the right package manager for the detected distro
- **Shared state**: `load_state` / `save_state VAR…` persist variables (GPU vendor, RAM, tier, distro) to `~/.config/ollama-stack/state.env` so later scripts can skip re-detection when chained together, but still work correctly when run standalone
- **TUI helpers**: `tui_available` detects a usable backend (`dialog`, else `whiptail`, else none), `tui_yesno` / `tui_menu` wrap the two; `detect_tui` treats `--no-tui` (sets `NO_TUI=1`) and a non-interactive stdin/stdout as "no backend" so scripted/chained calls never block on a prompt

To force full re-detection on the next run, delete `~/.config/ollama-stack/state.env`.

### Script responsibilities

| Script | Role |
|--------|------|
| `01-install-ollama.sh` | Installs Ollama via the official curl script; enables and waits up to 30 s for the systemd service |
| `02-configure-gpu.sh` | Detects GPU vendor via `lspci` PCI IDs (`10de`=Nvidia, `1002`/`1022`=AMD, `8086`=Intel — not commercial card names, which are unreliable), priority Nvidia > AMD > Intel for hybrid configs; installs drivers/Vulkan packages, writes a systemd drop-in (`/etc/systemd/system/ollama.service.d/override.conf`) for AMD Vulkan workarounds or Intel Vulkan; Nvidia uses CUDA with no override needed |
| `03-pull-models.sh` | Selects a model tier (XS/S/M/L) based on RAM and GPU presence, then pulls four models: texte, code, reflexion, embeddings |
| `04-install-webui.sh` | Installs Open WebUI via `pipx` (fallback: `pip`), creates a **user-level** systemd service (`~/.config/systemd/user/open-webui.service`) on port 8080 |

### AMD GPU handling

`02-configure-gpu.sh` uses `rocminfo` to read the real GFX code (e.g. `gfx1201`) rather than card names. The `AMD_GFX_OVERRIDE` associative array maps unsupported GFX codes to an `HSA_OVERRIDE_GFX_VERSION` value, enabling the Vulkan fallback for RDNA4 and future generations without maintaining a list of card names.

### Nvidia GPU handling

If `nvidia-smi` is not found, `02-configure-gpu.sh` **interactively prompts** the user before installing drivers, then exits asking for a reboot. Re-run after reboot to complete configuration.

### Intel GPU handling

There is no dedicated Ollama backend for Intel, so `02-configure-gpu.sh` enables the Vulkan backend (Mesa ANV driver) via the systemd drop-in, setting both `OLLAMA_VULKAN=1` and `OLLAMA_IGPU_ENABLE=1` (the latter is required for Ollama to actually consider integrated GPUs rather than only discrete ones). This is best effort — covers Xe/Iris iGPUs and Arc GPUs, and falls back to CPU silently if it doesn't activate.

### Model tiers

Tiers are defined as associative arrays `MODEL_XS/S/M/L` in `03-pull-models.sh`. Auto-selection is RAM-based, with a forced downgrade to S on CPU-only machines:

| Tier | RAM | Text | Code | Reasoning | Embeddings |
|------|-----|------|------|-----------|------------|
| XS | ≤ 8 GB | llama3.2:3b | qwen2.5-coder:3b | deepseek-r1:1.5b | nomic-embed-text |
| S | ≤ 16 GB | llama3.1:8b | qwen2.5-coder:7b | deepseek-r1:7b | nomic-embed-text |
| M | ≤ 32 GB | gemma3:12b | devstral:24b | deepseek-r1:14b | nomic-embed-text |
| L | > 32 GB | gemma3:27b | qwen2.5-coder:32b | deepseek-r1:32b | nomic-embed-text |

The tier selection uses `declare -n` (bash nameref) to dynamically reference the right `MODEL_<TIER>` array.

### Interactive model selection (TUI)

`03-pull-models.sh` also defines `CAND_<TIER>_<usage>` arrays (e.g. `CAND_S_texte`), each holding 2-3 `"model|description"` candidates per tier/usage. When no `--tier=` was forced, `tui_available` is true, and the terminal is interactive, `select_models_tui` shows one `tui_menu` per usage and overwrites the tier's default with the user's pick via the `tier_models` nameref; a cancelled/failed menu keeps the tier default. This selection step is skipped entirely (default model kept) when `--tier=` is forced, `--no-tui` is passed, or no `dialog`/`whiptail` backend is available, so `setup.sh` and any scripted call remain fully unattended.

Similarly, `02-configure-gpu.sh`'s Nvidia driver install confirmation uses `tui_yesno` when a backend is available, falling back to the original `read -r -p` prompt otherwise.

## Windows script (`setup.ps1` + `lib/common.ps1`)

A single-file orchestrator (`setup.ps1`) plus a shared library (`lib/common.ps1`), dot-sourced the same way `lib/common.sh` is on Linux — not split into four numbered scripts, since the roadmap that introduced it only called for these two files. `lib/common.ps1` provides:
- **Logging helpers**: `Log-Info`, `Log-Ok`, `Log-Warn`, `Log-Err` (`Write-Host` with `-ForegroundColor`, not raw ANSI codes, for PowerShell 5.1 console compatibility)
- **Shared state**: `Load-State` / `Save-State -VarNames …` persist globals (`RamGb`, `GpuVendor`, `GpuName`, `Tier`) to `%APPDATA%\ollama-stack\state.env`, using the same `VAR="value"` line format as Linux's `state.env` so both files stay easy to read side by side
- **`Get-RamGb`**: RAM rounded up to the next GB via `Get-CimInstance Win32_ComputerSystem`
- **`Get-GpuVendor`**: reads `PNPDeviceID` from `Get-CimInstance Win32_VideoController`, matching the *same* PCI vendor IDs as `02-configure-gpu.sh` (`10DE`=Nvidia, `1002`/`1022`=AMD, `8086`=Intel), priority Nvidia > AMD > Intel

`setup.ps1` itself:
- Defines `$ModelTiers` with the same tags as `MODEL_XS/S/M/L` in `03-pull-models.sh` — **kept in sync by hand**, update both when changing a model
- `Install-OllamaWindows`: installs via `winget` if available, else downloads and runs the official `OllamaSetup.exe` interactively; either way, verifies `ollama` is actually on `PATH` afterwards and errors out asking for a new terminal if not (a fresh PowerShell session is needed to pick up a PATH change made by the installer)
- `Set-GpuConfig`: **intentionally minimal**, unlike `02-configure-gpu.sh` — the official Windows installer already handles CUDA/ROCm natively, so this only logs the detected vendor and warns for AMD that the chip may be outside ROCm's officially supported list on Windows (no Vulkan-override equivalent is attempted)
- `Get-ModelTier` / `Install-Models`: same RAM thresholds and CPU-only downgrade-to-S rule as `03-pull-models.sh`'s `compute_tier`; no TUI/interactive model picker on Windows
- `Install-OpenWebUI`: installs via `pipx` (fallback `pip`), then registers a **per-user Scheduled Task** (`OpenWebUI`, `AtLogOn` trigger) instead of a systemd unit — Windows has no systemd — serving on the same port 8080; `OLLAMA_BASE_URL`/`WEBUI_AUTH` are persisted with `[Environment]::SetEnvironmentVariable(..., 'User')` since scheduled tasks inherit the user's persisted environment rather than accepting an `Environment=` block like systemd

Does not reuse or wrap the Bash scripts, including under WSL — it is a separate, native Windows implementation, so GPU/RAM/tier logic must be updated in both places when it changes.

## Desktop GUI (`gui/`)

A [Tauri](https://tauri.app) app (Rust backend in `gui/src-tauri/`, vanilla HTML/CSS/JS frontend in `gui/dist/`, no npm dependency — `tauri.conf.json` sets `app.withGlobalTauri = true` so the frontend uses the injected `window.__TAURI__` global instead of importing `@tauri-apps/api`). See `gui/README.md` for build/run instructions and full detail. Key points:
- `find_repo_root()` in `src-tauri/src/main.rs` walks up from the running executable looking for `setup.sh`/`setup.ps1`, so it works both under `cargo run` (nested a few levels under `gui/src-tauri/target/`) and as a standalone binary dropped at the repo root.
- The single `run_install` Tauri command spawns the platform script and streams stdout/stderr line-by-line back to the frontend as `install-log` events (plus a final `install-done`), via two reader threads per child process to avoid pipe-buffer deadlocks.
- **Linux**: only `01-install-ollama.sh`/`02-configure-gpu.sh` (system-level: packages, systemd units) run through `pkexec`; `03-pull-models.sh`/`04-install-webui.sh` (per-user state: `pipx`, `~/.config/systemd/user/`) run unprivileged. Wrapping the whole `setup.sh` in `pkexec` would run everything as root and misplace that per-user state — this is why the GUI invokes the four scripts separately instead of calling `setup.sh`.
- **Windows**: `setup.ps1` is invoked as a whole via `Start-Process -Verb RunAs`, since UAC elevation keeps the same user account (unlike `pkexec` switching to root), so there is no equivalent per-user-state problem there.
- The `open_webui_window` command opens (or focuses) a second window pointed directly at `http://127.0.0.1:8080` via `WebviewUrl::External`, reusing the same Tauri app rather than a separate webview technology; identical on Linux/Windows since it only uses Tauri's own window APIs. Always enabled — no "is Open WebUI up" check, a not-yet-running server just shows a connection error in that window.
- `gui/src-tauri/target/` and `gui/src-tauri/gen/schemas/` are gitignored (build artifacts); `gui/src-tauri/icons/icon.png` is a placeholder and `gui/src-tauri/Cargo.lock` is committed intentionally (binary application, not a library).

## Ollama Launcher (`launcher/`)

A second, separate Tauri app (own `Cargo.toml`/`tauri.conf.json`/binary, not a mode of `gui/`) for day-to-day use after the install is done. Unlike `gui/`, it never touches `setup.sh`/`setup.ps1`; it talks directly to local HTTP APIs:
- `list_models` / `pull_model` / `delete_model` in `launcher/src-tauri/src/main.rs` call Ollama's own REST API (`GET /api/tags`, `POST /api/pull`, `DELETE /api/delete` on `127.0.0.1:11434`) via `reqwest::blocking` — TLS is disabled at the Cargo feature level (`default-features = false`) since only plain local HTTP is ever used.
- `pull_model` streams Ollama's NDJSON progress response line-by-line into `pull-progress` events, the same idiom `gui/` uses for child-process stdout.
- Deleting a model requires confirming a JS `confirm()` dialog first, since it's irreversible.
- `open_webui_window` duplicates `gui/`'s command of the same name (opens/focuses a window on `http://127.0.0.1:8080` via `WebviewUrl::External`) — small enough that sharing it via a crate wasn't worth the indirection for two call sites.
- Same gitignore/Cargo.lock conventions as `gui/`, mirrored under `launcher/src-tauri/`.

## Conventions

- Every script starts with `set -euo pipefail` and `cd "$(dirname "$0")"` (Bash) or `$ErrorActionPreference = 'Stop'` (PowerShell).
- Scripts are idempotent: they check whether something is already installed/running before acting.
- All user-visible strings are in English (see `f174bb7`, which translated the originally-French comments/log messages).
- Open WebUI runs as a user-level service, not system-level: `systemctl --user` on Linux, a per-user Scheduled Task on Windows. Linux autostart without an active login session needs `sudo loginctl enable-linger $USER`.
- The `AMD_GFX_OVERRIDE` map in `02-configure-gpu.sh` must be updated when ROCm adds official support for a GFX generation (remove the entry) or when a new unsupported generation ships (add the entry).
- `dialog`/`whiptail` are optional, never auto-installed: `02-configure-gpu.sh` and `03-pull-models.sh` use them when present and the terminal is interactive, and silently fall back to plain `read`/default-model behavior otherwise (or always, with `--no-tui`). Windows has no TUI equivalent.
