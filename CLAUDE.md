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

# Allow/restrict Open WebUI access from other devices on the network, any time
./toggle-webui-lan.sh on|off|status
```

```powershell
# Windows: full install, auto-detection
.\setup.ps1

# Force a specific model tier
.\setup.ps1 -Tier M

# Skip model download or web UI
.\setup.ps1 -SkipModels
.\setup.ps1 -SkipWebui

# Allow/restrict Open WebUI access from other devices on the network, any time
.\toggle-webui-lan.ps1 on|off|status
```

There are no build steps for the Bash/PowerShell side — these are operational scripts, not a compiled project. There is, however, a `tests/` bats suite (see [Tests](#tests) below). Bash scripts are linted with [ShellCheck](https://www.shellcheck.net/) (`.github/workflows/lint.yml`, on every push/PR; run locally with `shellcheck -x *.sh lib/*.sh`); `setup.ps1`/`lib/common.ps1`/`toggle-webui-lan.ps1` are linted the same way with [PSScriptAnalyzer](https://github.com/PowerShell/PSScriptAnalyzer) in the same workflow (`Invoke-ScriptAnalyzer -Path setup.ps1, lib/common.ps1, toggle-webui-lan.ps1`), only failing the job on `Error`-severity findings since the current `Warning`-level baseline hasn't been audited.

## Tests

`tests/*.bats` ([bats](https://github.com/bats-core/bats-core)) exercises `lib/common.sh` (distro/RAM detection, the `state.env` round trip) and the branching logic in `01-install-ollama.sh`/`02-configure-gpu.sh`/`03-pull-models.sh`/`toggle-webui-lan.sh` (GPU vendor priority, model tier selection, the CPU-only downgrade rule, LAN toggle). Run with `bats tests/*.bats` from the repo root (no install needed beyond `bats` itself, e.g. `apt install bats` or `git clone` [bats-core](https://github.com/bats-core/bats-core) and call `bin/bats` directly).

Every test runs the real script (not a reimplementation of its logic) against a throwaway `$HOME` and a stub `PATH` set up by `tests/test_helper.bash`: `sudo` and `systemctl` are always stubbed (so nothing ever touches the real system, regardless of what's actually installed on the machine running the suite), and each test stubs whichever of `lspci`/`rocminfo`/`nvidia-smi`/`free`/`ollama`/`curl` it needs to control. `sudo`'s stub only drains stdin for `sudo tee` (piped into from `write_amd_override`) — draining it unconditionally would also eat the stdin meant for `02-configure-gpu.sh`'s Nvidia-driver `read -r -p` prompt further down the same script. `lib/common.sh`'s `detect_distro` reads `$OS_RELEASE_FILE` (defaulting to `/etc/os-release`) rather than the hardcoded path, purely so tests can point it at a fake file — this is the one non-test-file change made for testability.

This suite caught two real `set -euo pipefail` bugs in `02-configure-gpu.sh` on the first run: `write_amd_override` piping a command group into `sudo tee` where the group's last command could be a false `[ -n "$override" ] &&` test (making the whole pipeline — and the script — exit non-zero even though nothing was wrong), and `configure_nvidia` ending on a false `if [ "$install_confirmed" -eq 1 ]; then ... fi` with no trailing command when the user declined the driver-install prompt. Both are instances of the same class of bug: a conditional whose false branch has no following statement, as the *last* thing executed in a function/script under `set -e`, propagates its test's failure as the function's own exit status. Watch for this pattern (prefer `if`/`fi` with an explicit trailing `return 0`/`true` over a bare `cond && cmd` as the last statement) when adding new branches to any of these scripts.

`gui/src-tauri` and `launcher/src-tauri` each have a small `#[cfg(test)] mod tests` block (`cargo test` from within the crate directory): `find_marker_upwards` (the walk-up-from-executable logic behind `find_repo_root`) in `gui/`, and `to_model_info` (Ollama `/api/tags` response mapping, including the empty-string default when `details` is absent) in `launcher/`.

CI (`.github/workflows/test.yml`) runs both the bats suite and `cargo test` for both apps on every push/PR. `.github/workflows/rust-ci.yml` separately runs `cargo clippy --all-targets --all-features` and `cargo build --all-targets` for `gui/`/`launcher/` on every push/PR — previously these only ever got compiled at release time by `build-desktop.yml` after a version tag was already pushed, so a broken build was invisible until a release was underway. clippy is intentionally not run with `-D warnings` for the same "unaudited baseline" reason as PSScriptAnalyzer above. `.github/dependabot.yml` covers the two `cargo` manifests, the root `npm` one, and GitHub Actions versions, all on a weekly schedule.

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
| `04-install-webui.sh` | Installs Open WebUI via `pipx` (fallback: `pip`), creates a **user-level** systemd service (`~/.config/systemd/user/open-webui.service`) on port 8080, bound to `127.0.0.1` only by default |
| `toggle-webui-lan.sh` | Not part of the chain, run standalone at any time: flips Open WebUI between `127.0.0.1`-only and LAN-reachable (`0.0.0.0`) and restarts the service — see [Open WebUI LAN access toggle](#open-webui-lan-access-toggle) |

### AMD GPU handling

`02-configure-gpu.sh` uses `rocminfo` to read the real GFX code (e.g. `gfx1201`) rather than card names. The `AMD_GFX_OVERRIDE` associative array maps unsupported GFX codes to an `HSA_OVERRIDE_GFX_VERSION` value, enabling the Vulkan fallback for RDNA4 and future generations without maintaining a list of card names.

### Nvidia GPU handling

If `nvidia-smi` is not found, `02-configure-gpu.sh` **interactively prompts** the user before installing drivers, then exits asking for a reboot. Re-run after reboot to complete configuration.

### Intel GPU handling

There is no dedicated Ollama backend for Intel, so `02-configure-gpu.sh` enables the Vulkan backend (Mesa ANV driver) via the systemd drop-in, setting both `OLLAMA_VULKAN=1` and `OLLAMA_IGPU_ENABLE=1` (the latter is required for Ollama to actually consider integrated GPUs rather than only discrete ones). This is best effort — covers Xe/Iris iGPUs and Arc GPUs, and falls back to CPU silently if it doesn't activate.

### Open WebUI LAN access toggle

Open WebUI's `serve` command has no environment variable for its bind address, only a `--host` CLI flag (upstream default `0.0.0.0`) — confirmed by reading `open_webui`'s own `serve()` definition, since this is easy to get wrong silently. `04-install-webui.sh` therefore writes the desired host to its own small file, `~/.config/ollama-stack/webui.env` (`WEBUI_HOST=127.0.0.1` by default, only created if missing so re-running the installer never resets a later choice), and references it from the unit via `EnvironmentFile=`. The `ExecStart=` line reads `--host \${WEBUI_HOST}` with the `$` escaped in the heredoc so it reaches the unit file literally — systemd itself substitutes `${WEBUI_HOST}` from `EnvironmentFile=` into `ExecStart=` at service-start time (verified empirically with a throwaway unit; this is a systemd feature, unrelated to Open WebUI). `toggle-webui-lan.sh on|off|status` only rewrites `webui.env` and reloads/restarts the service — it never regenerates the unit file, so it works instantly without re-running the installer. `WEBUI_AUTH` is intentionally *not* touched by the toggle: enabling LAN access never silently enables auth, and both scripts print a warning explaining that when LAN access is turned on.

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
- `Install-OpenWebUI`: installs via `pipx` (fallback `pip`), then registers a **per-user Scheduled Task** (`OpenWebUI`, `AtLogOn` trigger) instead of a systemd unit — Windows has no systemd — serving on the same port 8080; `OLLAMA_BASE_URL`/`WEBUI_AUTH` are persisted with `[Environment]::SetEnvironmentVariable(..., 'User')` since scheduled tasks inherit the user's persisted environment rather than accepting an `Environment=` block like systemd. Unlike `WEBUI_AUTH`, the bind address has no env-var equivalent on the Open WebUI side (only a `--host` CLI flag, same constraint as Linux — see [Open WebUI LAN access toggle](#open-webui-lan-access-toggle)), so `$Global:WebuiHost` (persisted like `Tier`/`GpuVendor` via `Save-State`, `127.0.0.1` by default) is baked directly into the scheduled task's `-Argument` string at registration time.
- `toggle-webui-lan.ps1`: the Windows counterpart to `toggle-webui-lan.sh`. Since there's no systemd-style `EnvironmentFile=` substitution to lean on, it rebuilds the scheduled task's action with `Set-ScheduledTask -Action` (reusing the already-registered `Execute` path) and restarts the task — no reinstall needed. Also never touches `WEBUI_AUTH`, same reasoning as the Linux script.

Does not reuse or wrap the Bash scripts, including under WSL — it is a separate, native Windows implementation, so GPU/RAM/tier logic must be updated in both places when it changes.

## Desktop GUI (`gui/`)

A [Tauri](https://tauri.app) app (Rust backend in `gui/src-tauri/`, vanilla HTML/CSS/JS frontend in `gui/dist/`, no npm dependency — `tauri.conf.json` sets `app.withGlobalTauri = true` so the frontend uses the injected `window.__TAURI__` global instead of importing `@tauri-apps/api`). See `gui/README.md` for build/run instructions and full detail. Key points:
- `find_repo_root()` in `src-tauri/src/main.rs` walks up from the running executable looking for `setup.sh`/`setup.ps1`, so it works both under `cargo run` (nested a few levels under `gui/src-tauri/target/`) and as a standalone binary dropped at the repo root.
- The single `run_install` Tauri command spawns the platform script and streams stdout/stderr line-by-line back to the frontend as `install-log` events (plus a final `install-done`), via two reader threads per child process to avoid pipe-buffer deadlocks.
- **Linux**: only `01-install-ollama.sh`/`02-configure-gpu.sh` (system-level: packages, systemd units) run through `pkexec`; `03-pull-models.sh`/`04-install-webui.sh` (per-user state: `pipx`, `~/.config/systemd/user/`) run unprivileged. Wrapping the whole `setup.sh` in `pkexec` would run everything as root and misplace that per-user state — this is why the GUI invokes the four scripts separately instead of calling `setup.sh`. The `pkexec` child is detached into its own session via `detach_from_tty` (`libc::setsid()` in a `pre_exec` hook) so it has no controlling terminal, otherwise `pkexec` silently falls back to a text password prompt on `/dev/tty` when no polkit agent is registered — leaking into whatever terminal launched the GUI instead of showing the graphical prompt or failing loudly.
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

## Releases

The root `package.json` is release tooling only (commitlint, husky, semantic-release) — it is not a JS project and has nothing to do with `gui/`/`launcher/`'s frontends, which still have zero npm dependency of their own.

- **Commit messages must follow [Conventional Commits](https://www.conventionalcommits.org/)** (`feat:`, `fix:`, `docs:`, `chore:`, ...), enforced locally by a husky `commit-msg` hook running `commitlint` (`commitlint.config.js`, extends `@commitlint/config-conventional`). Run `npm install` once at the repo root to activate the hook (`prepare` script sets it up).
- **`.github/workflows/release.yml`** runs `semantic-release` on every push to `main` (i.e. whenever `dev` is promoted to `main`). It inspects commits since the last release to decide the version bump (`fix:` → patch, `feat:` → minor, `BREAKING CHANGE:` footer → major), updates `CHANGELOG.md`, and creates both a `vX.Y.Z` git tag and a GitHub Release (`.releaserc.json`). It does **not** publish anything to npm (`package.json` has `"private": true`, and the plugin list in `.releaserc.json` omits `@semantic-release/npm`).
- **`.github/workflows/build-desktop.yml`** triggers on that `vX.Y.Z` tag push (also available via manual `workflow_dispatch` for testing the build step alone) and builds `gui/` and `launcher/` for Linux (`.deb`/`.rpm`/`.AppImage`, on `ubuntu-22.04` for broad glibc compatibility) and Windows (`.msi`/`.exe`) via [`tauri-apps/tauri-action`](https://github.com/tauri-apps/tauri-action), attaching the bundles to the release semantic-release just created.
- The `version` field inside `gui/src-tauri/tauri.conf.json` / `launcher/src-tauri/tauri.conf.json` is **not** automatically synced to the release tag — a deliberate simplification for now, since the two apps aren't independently versioned yet.
- `gui/src-tauri/icons/` and `launcher/src-tauri/icons/` each hold a full icon set (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.ico`, `icon.png`) generated with ImageMagick (`convert ... -define icon:auto-resize=... icon.ico`), required by the Windows bundler (`.ico`) and recommended for the Linux ones; both `bundle.active` are `true` with `bundle.targets: "all"` so the bundler picks whatever formats are valid for the OS it runs on.

## Conventions

- Every script starts with `set -euo pipefail` and `cd "$(dirname "$0")"` (Bash) or `$ErrorActionPreference = 'Stop'` (PowerShell).
- Scripts are idempotent: they check whether something is already installed/running before acting.
- All user-visible strings are in English (see `f174bb7`, which translated the originally-French comments/log messages).
- Open WebUI runs as a user-level service, not system-level: `systemctl --user` on Linux, a per-user Scheduled Task on Windows. Linux autostart without an active login session needs `sudo loginctl enable-linger $USER`.
- Open WebUI is deployed with `WEBUI_AUTH=False` (no login) and, since the LAN-access toggle, listens on `127.0.0.1` only by default — see [Open WebUI LAN access toggle](#open-webui-lan-access-toggle) and [README.md's Security note](README.md#security-note). `WEBUI_AUTH` and the bind address are deliberately independent switches; don't couple them (e.g. don't make enabling LAN access also flip `WEBUI_AUTH`) without asking first, since that's a product decision, not a bug fix.
- The `AMD_GFX_OVERRIDE` map in `02-configure-gpu.sh` must be updated when ROCm adds official support for a GFX generation (remove the entry) or when a new unsupported generation ships (add the entry).
- Commit messages must follow Conventional Commits (see [Releases](#releases) above) — enforced by a husky/commitlint hook once `npm install` has been run at the repo root.
- The two `# shellcheck disable=` directives in `03-pull-models.sh` (file-wide `SC2034` for the nameref-only-read `MODEL_*`/`CAND_*` arrays, and a line-level `SC2004` on `tier_models[$usage]=`) are deliberate, verified false positives, not lint debt — see the comments right above each for why. Don't "fix" the `SC2004` one by dropping the `$`: `tier_models` is a nameref to an associative array, and doing so silently writes to a literal `usage` key instead of the intended tier/usage (verified in bash: `arr[key]=` and `arr[$key]=` are different keys for associative arrays, unlike indexed ones).
- `dialog`/`whiptail` are optional, never auto-installed: `02-configure-gpu.sh` and `03-pull-models.sh` use them when present and the terminal is interactive, and silently fall back to plain `read`/default-model behavior otherwise (or always, with `--no-tui`). Windows has no TUI equivalent.
