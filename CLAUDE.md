# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project does

A set of Bash scripts that install and configure a local Ollama stack (LLM inference + Open WebUI) on Linux. The four numbered scripts can be run individually or chained via `setup.sh`.

## Requirements

- Linux (Arch, Debian/Ubuntu, Fedora, openSUSE — other distros require manual GPU driver installation)
- bash ≥ 4.0 (associative arrays used throughout)
- `curl`, `sudo` privileges for package installation and systemd configuration

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
```

There are no tests, build steps, or linters — these are operational shell scripts.

## Architecture

All scripts source `lib/common.sh` at startup, which provides:
- **Logging helpers**: `log_info`, `log_ok`, `log_warn`, `log_err`
- **Distro detection**: `detect_distro` sets `DISTRO_FAMILY` (arch / debian / fedora / opensuse / unknown) by reading `/etc/os-release` fields `ID` and `ID_LIKE`
- **Package installation**: `pkg_install <pkg>…` dispatches to the right package manager for the detected distro
- **Shared state**: `load_state` / `save_state VAR…` persist variables (GPU vendor, RAM, tier, distro) to `~/.config/ollama-stack/state.env` so later scripts can skip re-detection when chained together, but still work correctly when run standalone

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

## Conventions

- Every script starts with `set -euo pipefail` and `cd "$(dirname "$0")"`.
- Scripts are idempotent: they check whether something is already installed/running before acting.
- All user-visible strings are in French.
- Open WebUI runs as a user-level service (`systemctl --user`), not system-level. To enable autostart without an active login session: `sudo loginctl enable-linger $USER`.
- The `AMD_GFX_OVERRIDE` map in `02-configure-gpu.sh` must be updated when ROCm adds official support for a GFX generation (remove the entry) or when a new unsupported generation ships (add the entry).
