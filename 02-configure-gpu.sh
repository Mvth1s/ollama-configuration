#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# 02-configure-gpu.sh
# Detects ALL graphics controllers present (a laptop often has an integrated
# Intel GPU + a dedicated Nvidia or AMD GPU at the same time), picks the most
# powerful one, and configures the appropriate acceleration backend:
#   - Nvidia  : CUDA, works the same way across all Nvidia cards supported
#               by the installed driver, no per-card special casing needed.
#   - AMD     : ROCm/HIP by default. For chips too recent to be in ROCm's
#               officially supported list, automatically falls back to the
#               Vulkan backend with the HSA_OVERRIDE_GFX_VERSION workaround
#               known for that generation (detected via the real gfx code of
#               the chip, not the commercial name, so it covers the entire
#               RDNA4 lineup at once rather than just 3-4 specific models).
#   - Intel   : no dedicated backend in Ollama, but Mesa provides a Vulkan
#               driver (ANV) for all Xe/Iris iGPUs and Arc GPUs. We enable
#               Ollama's Vulkan backend to take advantage of it. This is
#               best effort — Intel via Vulkan is more recent and less
#               battle-tested than CUDA/ROCm: if it doesn't activate, Ollama
#               simply falls back to CPU without crashing.
#   - No dedicated GPU: nothing to do, CPU.
# =============================================================================

load_state
detect_distro

# ---------------------------------------------------------------------------
# Detect all graphics controllers, pick the "best" one
# (priority: dedicated Nvidia > dedicated AMD > Intel) for hybrid configs
# (typical on laptops: Intel iGPU + Nvidia/AMD dGPU).
# ---------------------------------------------------------------------------
GPU_VENDOR="none"
GPU_NAME=""

detect_all_gpus() {
  local lines
  lines=$(lspci -nnk 2>/dev/null | grep -Ei 'vga|3d|display' || true)

  if [ -z "$lines" ]; then
    log_warn "No graphics controller detected via lspci."
    return
  fi

  log_info "Detected graphics controllers:"
  echo "$lines" | sed -E 's/^/  - /'

  if echo "$lines" | grep -qi 'nvidia'; then
    GPU_VENDOR="nvidia"
    GPU_NAME=$(echo "$lines" | grep -i 'nvidia' | head -n1 | sed -E 's/.*: //')
  elif echo "$lines" | grep -qi 'amd\|ati\|radeon'; then
    GPU_VENDOR="amd"
    GPU_NAME=$(echo "$lines" | grep -i 'amd\|ati\|radeon' | head -n1 | sed -E 's/.*: //')
  elif echo "$lines" | grep -qi 'intel'; then
    GPU_VENDOR="intel"
    GPU_NAME=$(echo "$lines" | grep -i 'intel' | head -n1 | sed -E 's/.*: //')
  fi

  log_info "GPU selected for acceleration: ${GPU_NAME:-none} ($GPU_VENDOR)"
  save_state GPU_VENDOR GPU_NAME
}

# ---------------------------------------------------------------------------
# AMD: detect the real gfx code via rocminfo rather than the commercial card
# name. This covers any AMD chip, present or future, without maintaining a
# list of card names by hand.
# ---------------------------------------------------------------------------

# Generations known to NOT yet be in the officially supported list by ROCm
# at the time this script was written (therefore requiring the Vulkan fallback
# + HSA_OVERRIDE_GFX_VERSION workaround). Update this list over time if ROCm
# adds official support or if a new generation ships before being supported.
declare -A AMD_GFX_OVERRIDE=(
  ["gfx1200"]="11.5.0"   # RDNA4 (RX 9060 / 9060 XT)
  ["gfx1201"]="11.5.0"   # RDNA4 (RX 9070 / 9070 XT)
)

configure_amd() {
  log_info "Configuring AMD GPU..."
  case "$DISTRO_FAMILY" in
    arch)     pkg_install vulkan-radeon vulkan-icd-loader vulkan-tools rocminfo ;;
    debian)   pkg_install mesa-vulkan-drivers vulkan-tools rocminfo ;;
    fedora)   pkg_install mesa-vulkan-drivers vulkan-tools rocminfo ;;
    opensuse) pkg_install vulkan-tools rocminfo ;;
  esac

  local gfx=""
  if command -v rocminfo >/dev/null 2>&1; then
    gfx=$(rocminfo 2>/dev/null | grep -oE 'gfx[0-9]+' | head -n1 || true)
  fi

  if [ -z "$gfx" ]; then
    log_warn "GFX code not detected (rocminfo missing or chip not recognized)."
    log_warn "Enabling Vulkan backend by default, without a specific workaround."
    write_amd_override ""
    return
  fi

  log_info "AMD chip identified: $gfx"

  if [ -n "${AMD_GFX_OVERRIDE[$gfx]:-}" ]; then
    local override="${AMD_GFX_OVERRIDE[$gfx]}"
    log_warn "$gfx is not yet officially supported by ROCm."
    log_warn "Applying workaround: OLLAMA_VULKAN=1 + HSA_OVERRIDE_GFX_VERSION=$override"
    write_amd_override "$override"
  else
    log_ok "$gfx is officially supported by ROCm, using default config (HIP)."
    clear_ollama_override
  fi

  save_state
}

write_amd_override() {
  local override="$1"
  sudo mkdir -p /etc/systemd/system/ollama.service.d
  {
    echo "[Service]"
    echo 'Environment="OLLAMA_VULKAN=1"'
    [ -n "$override" ] && echo "Environment=\"HSA_OVERRIDE_GFX_VERSION=$override\""
  } | sudo tee /etc/systemd/system/ollama.service.d/override.conf >/dev/null
  sudo systemctl daemon-reload
}

clear_ollama_override() {
  sudo rm -f /etc/systemd/system/ollama.service.d/override.conf
  sudo systemctl daemon-reload
}

# ---------------------------------------------------------------------------
# Nvidia: CUDA works the same way across the entire supported card range,
# no per-card configuration is needed here, unlike AMD.
# ---------------------------------------------------------------------------
configure_nvidia() {
  log_info "Configuring Nvidia GPU (CUDA)..."
  clear_ollama_override

  if command -v nvidia-smi >/dev/null 2>&1; then
    log_ok "Nvidia driver already present: $(nvidia-smi --query-gpu=name --format=csv,noheader | head -n1)"
    return
  fi

  log_warn "No Nvidia driver detected, Ollama will use the CPU until one is installed."
  read -r -p "Install the Nvidia driver now? (requires a reboot afterwards) [y/N] " reply
  if [[ "$reply" =~ ^[yY]$ ]]; then
    case "$DISTRO_FAMILY" in
      arch)     pkg_install nvidia nvidia-utils ;;
      debian)   pkg_install nvidia-driver ;;
      fedora)   pkg_install akmod-nvidia ;;
      opensuse) log_warn "On openSUSE, add the official NVIDIA repository then install x11-video-nvidiaG06." ;;
    esac
    log_warn "Reboot the machine then re-run this script to finish the configuration."
    exit 0
  fi
}

# ---------------------------------------------------------------------------
# Intel: no dedicated backend in Ollama, we go through Vulkan (Mesa ANV
# driver), which covers both iGPUs (Xe, Iris Xe, UHD) and dedicated Arc GPUs.
# Best effort: if Ollama cannot use it, falls back to CPU automatically,
# without a blocking error.
# ---------------------------------------------------------------------------
configure_intel() {
  log_info "Configuring Intel GPU (Vulkan, best effort)..."
  case "$DISTRO_FAMILY" in
    arch)     pkg_install vulkan-intel vulkan-icd-loader vulkan-tools ;;
    debian)   pkg_install mesa-vulkan-drivers vulkan-tools ;;
    fedora)   pkg_install mesa-vulkan-drivers vulkan-tools ;;
    opensuse) pkg_install vulkan-tools ;;
  esac

  sudo mkdir -p /etc/systemd/system/ollama.service.d
  printf '[Service]\nEnvironment="OLLAMA_VULKAN=1"\n' | sudo tee /etc/systemd/system/ollama.service.d/override.conf >/dev/null
  sudo systemctl daemon-reload

  log_warn "Intel via Vulkan is more recent than CUDA/ROCm: verify real-world performance,"
  log_warn "and disable (rm /etc/systemd/system/ollama.service.d/override.conf) if slower than CPU alone."
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
detect_all_gpus

case "$GPU_VENDOR" in
  amd)    configure_amd ;;
  nvidia) configure_nvidia ;;
  intel)  configure_intel ;;
  none)   log_info "No dedicated GPU, Ollama will run on CPU." ; clear_ollama_override ;;
esac

if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet ollama 2>/dev/null; then
  sudo systemctl restart ollama
fi

log_ok "GPU configuration complete."
