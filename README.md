# ollama-configuration

Bash scripts to install and configure a local [Ollama](https://ollama.com) stack on Linux, with automatic GPU and RAM detection. Installs Ollama, configures GPU acceleration, downloads a set of models suited to the machine, and deploys [Open WebUI](https://github.com/open-webui/open-webui) as a web interface.

## Requirements

- Linux (Arch, Debian/Ubuntu, Fedora, openSUSE — other distros require manual GPU driver installation)
- `curl`, `bash` ≥ 4.0 (for associative arrays)
- `sudo` privileges for package installation and systemd configuration

## Quick start

```bash
git clone https://github.com/Mvth1s/ollama-configuration.git
cd ollama-configuration
./setup.sh
```

The web interface is then available at **http://localhost:8080**.

## Options

```bash
./setup.sh                   # full install, auto-detection
./setup.sh --tier=M          # force a specific model tier (XS / S / M / L)
./setup.sh --skip-models     # install Ollama + GPU + WebUI without models
./setup.sh --skip-webui      # skip Open WebUI installation
./setup.sh --no-tui          # disable the interactive dialog/whiptail menus
```

## Interactive mode (TUI)

When run in an interactive terminal with `dialog` or `whiptail` installed, and no explicit `--tier=`, `02-configure-gpu.sh` and `03-pull-models.sh` show interactive menus instead of the plain text prompts: a yes/no dialog to confirm the Nvidia driver install, and a per-usage menu (text/code/reasoning/embeddings) to pick among 2-3 candidate models instead of the tier's single default.

Neither `dialog` nor `whiptail` is a hard requirement. If neither is installed, or the script is not run in an interactive terminal (chained through `setup.sh` unattended, or in a script/CI), the scripts fall back to the existing plain prompts and default models automatically. Pass `--no-tui` to force that fallback even in an interactive terminal.

## Individual steps

Each script can be re-run on its own without reinstalling everything:

```bash
./01-install-ollama.sh                          # install Ollama and start the service
./02-configure-gpu.sh [--no-tui]                # detect GPU and configure acceleration
./03-pull-models.sh [--tier=XS|S|M|L] [--no-tui] # download models
./04-install-webui.sh                           # install Open WebUI
```

## Model tiers

The tier is chosen automatically based on available RAM and can be overridden with `--tier=`.

| Tier | RAM | Text | Code | Reasoning | Embeddings |
|------|-----|------|------|-----------|------------|
| XS | ≤ 8 GB | llama3.2:3b | qwen2.5-coder:3b | deepseek-r1:1.5b | nomic-embed-text |
| S | ≤ 16 GB | llama3.1:8b | qwen2.5-coder:7b | deepseek-r1:7b | nomic-embed-text |
| M | ≤ 32 GB | gemma3:12b | devstral:24b | deepseek-r1:14b | nomic-embed-text |
| L | > 32 GB | gemma3:27b | qwen2.5-coder:32b | deepseek-r1:32b | nomic-embed-text |

On CPU-only machines (no dedicated GPU), the tier is capped at S regardless of RAM.

## GPU support

| Vendor | Backend | Notes |
|--------|---------|-------|
| Nvidia | CUDA | No per-card configuration needed. |
| AMD | ROCm/HIP | Automatically falls back to Vulkan + `HSA_OVERRIDE_GFX_VERSION` for RDNA4 chips not yet officially supported by ROCm. |
| Intel | Vulkan (Mesa ANV) | Best effort — covers Xe/Iris iGPUs and Arc dedicated GPUs. Falls back to CPU silently if unsupported. |
| None | CPU | No configuration needed. |

## Useful commands after installation

```bash
ollama list                            # list installed models
ollama run llama3.1:8b                 # run a model from the terminal
systemctl status ollama                # Ollama service status
systemctl --user status open-webui     # Open WebUI service status
```

To start Open WebUI without an active user session:
```bash
sudo loginctl enable-linger $USER
```

## Windows installation

A separate, native PowerShell implementation (`setup.ps1` + `lib\common.ps1`), not a port of the Bash scripts and not meant to run under WSL:

```powershell
git clone https://github.com/Mvth1s/ollama-configuration.git
cd ollama-configuration
.\setup.ps1
```

Options:

```powershell
.\setup.ps1                     # full install, auto-detection
.\setup.ps1 -Tier M             # force a specific model tier (XS / S / M / L)
.\setup.ps1 -SkipModels         # install Ollama + Open WebUI without models
.\setup.ps1 -SkipWebui          # skip Open WebUI installation
```

Same RAM-based tier auto-selection and model tables as the table above. GPU handling is intentionally minimal: the official Ollama Windows installer already detects CUDA and ROCm natively, so the script only detects the GPU vendor (same PCI vendor IDs as the Linux scripts) to log it, and warns if an AMD GPU may fall outside ROCm's officially supported list on Windows.

Open WebUI is installed via `pip`/`pipx` and run through a per-user scheduled task (`OpenWebUI`, triggered at logon) instead of a systemd service, at the same `http://localhost:8080`.

```powershell
ollama list                    # list installed models
Get-ScheduledTask OpenWebUI    # Open WebUI task status
```

## Security note

Open WebUI is installed with **no login** (`WEBUI_AUTH=False`) — anyone who can reach it can chat and manage models without authenticating. To limit the blast radius of that, it also listens on **`127.0.0.1` only by default**: nothing outside the machine itself can reach it, regardless of `WEBUI_AUTH`.

If you want to reach it from another device on your network (e.g. a phone), enable LAN access at any time, on or off, without reinstalling anything:

```bash
./toggle-webui-lan.sh on       # reachable from your local network
./toggle-webui-lan.sh off      # back to this machine only (default)
./toggle-webui-lan.sh status   # show the current setting
```

```powershell
.\toggle-webui-lan.ps1 on
.\toggle-webui-lan.ps1 off
.\toggle-webui-lan.ps1 status
```

Turning LAN access **on** prints a warning every time, because `WEBUI_AUTH` stays `False`: once reachable from the network, anyone on it can use Open WebUI, including pulling/deleting models, without logging in. If that's not acceptable for your network, enable login instead of (or in addition to) LAN access:

- Linux: edit `Environment="WEBUI_AUTH=False"` to `"True"` in `~/.config/systemd/user/open-webui.service`, then `systemctl --user daemon-reload && systemctl --user restart open-webui`.
- Windows: `[Environment]::SetEnvironmentVariable('WEBUI_AUTH', 'True', 'User')`, then restart the `OpenWebUI` scheduled task.
- Create an account on your next visit to `http://localhost:8080` — the first account created becomes the admin.

## Linting

Bash scripts are checked with [ShellCheck](https://www.shellcheck.net/) on every push and pull request ([`.github/workflows/lint.yml`](.github/workflows/lint.yml)). Run the same check locally from the repo root:

```bash
shellcheck -x *.sh lib/*.sh
```

`setup.ps1`/`lib/common.ps1`/`toggle-webui-lan.ps1` are checked the same way with [PSScriptAnalyzer](https://github.com/PowerShell/PSScriptAnalyzer):

```powershell
Install-Module -Name PSScriptAnalyzer -Scope CurrentUser
Invoke-ScriptAnalyzer -Path setup.ps1, lib/common.ps1, toggle-webui-lan.ps1
```

## Tests

[`tests/`](tests/) holds a [bats](https://github.com/bats-core/bats-core) suite covering `lib/common.sh` and the tier/GPU-vendor/LAN-toggle logic in the numbered scripts. Every test runs against a throwaway `$HOME` and a stubbed `PATH` (see [`tests/test_helper.bash`](tests/test_helper.bash)), so it never touches the real package manager, systemd, or network — safe to run on your own machine, not just in CI:

```bash
bats tests/*.bats
```

`gui/` and `launcher/` each have a handful of `#[cfg(test)]` unit tests (repo-root discovery, Ollama API response mapping):

```bash
cd gui/src-tauri && cargo test       # or launcher/src-tauri
```

Both suites run on every push and pull request via [`.github/workflows/test.yml`](.github/workflows/test.yml); [`.github/workflows/rust-ci.yml`](.github/workflows/rust-ci.yml) additionally runs `cargo clippy`/`cargo build` for `gui/`/`launcher/` on every push and PR, so a broken build is no longer only caught at release time.

## Desktop GUI

A thin [Tauri](https://tauri.app) GUI wrapping `setup.sh`/`setup.ps1` (same options, streams the scripts' output live, plus a button to open Open WebUI in its own window) is available in [`gui/`](gui/README.md), for the one-off install.

For day-to-day use afterwards, [`launcher/`](launcher/README.md) is a separate, smaller Tauri app: opens Open WebUI in its own window, and lists/pulls/deletes Ollama models directly via Ollama's local API.

Packaged installers (`.deb`/`.rpm`/`.AppImage`/`.msi`/`.exe`) for both are attached to [GitHub Releases](https://github.com/Mvth1s/ollama-configuration/releases) — built and published automatically by CI on every release.

A showcase site for the project (`docs/index.html`) will be deployed at **[ollama-configuration.vercel.app](https://ollama-configuration.vercel.app)** once the first release of the desktop apps is published; `vercel.json` at the repo root already points Vercel at the `docs/` folder for that deploy.

## License

[MIT](LICENSE)
