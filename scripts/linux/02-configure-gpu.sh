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
#
# Usage: ./02-configure-gpu.sh [--no-tui]
# =============================================================================

DETECT_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --no-tui) NO_TUI=1 ;;
    --detect-only) DETECT_ONLY=1 ;;
    *) log_err "Unknown option: $arg"; exit 1 ;;
  esac
done

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

  if echo "$lines" | grep -qE '\[10de:'; then
    GPU_VENDOR="nvidia"
    GPU_NAME=$(echo "$lines" | grep -E '\[10de:' | head -n1 | sed -E 's/.*: //')
  elif echo "$lines" | grep -qE '\[(1002|1022):'; then
    GPU_VENDOR="amd"
    GPU_NAME=$(echo "$lines" | grep -E '\[(1002|1022):' | head -n1 | sed -E 's/.*: //')
  elif echo "$lines" | grep -qE '\[8086:'; then
    GPU_VENDOR="intel"
    GPU_NAME=$(echo "$lines" | grep -E '\[8086:' | head -n1 | sed -E 's/.*: //')
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

# Real ROCm packages sometimes install rocminfo outside PATH entirely -
# confirmed in production on an Arch/EndeavourOS machine with an AMD RDNA4
# GPU, where the rocminfo package places the binary under /opt/rocm/bin (or
# a version-suffixed /opt/rocm-X.Y.Z/bin on distros that keep multiple ROCm
# releases side by side) without ever adding it to PATH. find_rocminfo checks
# PATH first (`command -v`, the previous - and only - lookup this script
# used), then these two well-known install locations, printing the resolved
# binary's path on stdout and returning 0 on success. Returns 1 (nothing
# printed) if none is found anywhere, letting the caller fall back to plain
# Vulkan without an HSA override, exactly like the pre-existing "chip not
# recognized" case.
#
# ROCM_OPT_DIR defaults to /opt (the real, standard ROCm install root) but
# can be overridden - same testability pattern as lib/common.sh's
# OS_RELEASE_FILE - so tests can point it at a throwaway directory instead
# of the real /opt, which a test must never write to.
ROCM_OPT_DIR="${ROCM_OPT_DIR:-/opt}"

find_rocminfo() {
  if command -v rocminfo >/dev/null 2>&1; then
    command -v rocminfo
    return 0
  fi

  if [ -x "$ROCM_OPT_DIR/rocm/bin/rocminfo" ]; then
    echo "$ROCM_OPT_DIR/rocm/bin/rocminfo"
    return 0
  fi

  local candidate
  for candidate in "$ROCM_OPT_DIR"/rocm-*/bin/rocminfo; do
    if [ -x "$candidate" ]; then
      echo "$candidate"
      return 0
    fi
  done

  return 1
}

configure_amd() {
  log_info "Configuring AMD GPU..."
  case "$DISTRO_FAMILY" in
    arch)     pkg_install vulkan-radeon vulkan-icd-loader vulkan-tools rocminfo ;;
    debian)   pkg_install mesa-vulkan-drivers vulkan-tools rocminfo ;;
    fedora)   pkg_install mesa-vulkan-drivers vulkan-tools rocminfo ;;
    opensuse) pkg_install vulkan-tools rocminfo ;;
  esac

  local gfx="" rocminfo_bin=""
  if rocminfo_bin=$(find_rocminfo); then
    gfx=$("$rocminfo_bin" 2>/dev/null | grep -oE 'gfx[0-9]+' | head -n1 || true)
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
    # Deliberately an `if`, not `[ -n "$override" ] && echo ...`: as the last
    # command in this group, a false `&&` would make the whole group (and
    # therefore the pipe into `sudo tee`) exit non-zero when $override is
    # empty, aborting the script under `set -euo pipefail` even though
    # nothing actually went wrong (caught by tests/configure_gpu_vendor.bats).
    if [ -n "$override" ]; then
      echo "Environment=\"HSA_OVERRIDE_GFX_VERSION=$override\""
    fi
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

  local install_confirmed=0
  if tui_available; then
    if tui_yesno "Nvidia driver" "Install the Nvidia driver now?\n(requires a reboot afterwards)"; then
      install_confirmed=1
    fi
  else
    local reply
    # `|| reply=""`: on closed/EOF stdin (e.g. invoked from a non-interactive
    # context despite --no-tui not being what disables this fallback), `read`
    # itself returns non-zero, which would otherwise abort the whole script
    # under set -euo pipefail instead of just treating it as "no".
    read -r -p "Install the Nvidia driver now? (requires a reboot afterwards) [y/N] " reply || reply=""
    if [[ "$reply" =~ ^[yY]$ ]]; then
      install_confirmed=1
    fi
  fi

  if [ "$install_confirmed" -eq 1 ]; then
    case "$DISTRO_FAMILY" in
      arch)     pkg_install nvidia nvidia-utils ;;
      debian)   pkg_install nvidia-driver ;;
      fedora)   pkg_install akmod-nvidia ;;
      opensuse) log_warn "On openSUSE, add the official NVIDIA repository then install x11-video-nvidiaG06." ;;
    esac
    log_warn "Reboot the machine then re-run this script to finish the configuration."
    exit 0
  fi

  # Explicit success: without it, declining the prompt above would leave the
  # `if [ "$install_confirmed" -eq 1 ]; then ... fi` as this function's last
  # command, and its false test's exit status would become configure_nvidia's
  # own return code, aborting the script under `set -euo pipefail` even
  # though declining isn't an error (caught by tests/configure_gpu_vendor.bats).
  return 0
}

# ---------------------------------------------------------------------------
# Intel: no dedicated backend in Ollama, we go through Vulkan (Mesa ANV
# driver), which covers both iGPUs (Xe, Iris Xe, UHD) and dedicated Arc GPUs.
# Best effort: if Ollama cannot use it, falls back to CPU automatically,
# without a blocking error.
#
# NOT DISTINGUISHED: this applies OLLAMA_VULKAN=1 + OLLAMA_IGPU_ENABLE=1
# uniformly to *any* GPU_VENDOR=intel match, whether it's an integrated GPU
# or a discrete Arc card - there is no iGPU-vs-Arc branch here. This is
# deliberate for now, not an oversight: a distinction was discussed during
# earlier planning (the idea being that Vulkan on a discrete Arc card is far
# more likely to be a net win than on some older/weaker iGPUs, where plain
# CPU inference can sometimes be faster), but it has never been implemented
# or validated against real Arc hardware - no such machine has been
# available in any session that touched this script. Do not add an
# iGPU/Arc branch here without testing on real discrete Arc hardware first;
# see CLAUDE.md's Intel GPU handling section for the same note.
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
  printf '[Service]\nEnvironment="OLLAMA_VULKAN=1"\nEnvironment="OLLAMA_IGPU_ENABLE=1"\n' | sudo tee /etc/systemd/system/ollama.service.d/override.conf >/dev/null
  sudo systemctl daemon-reload

  log_warn "Intel via Vulkan is more recent than CUDA/ROCm: verify real-world performance,"
  log_warn "and disable (rm /etc/systemd/system/ollama.service.d/override.conf) if slower than CPU alone."
}

# ---------------------------------------------------------------------------
# --detect-only: report distro/GPU/CPU as JSON for a caller (e.g. the Tauri
# GUI) that wants real detection data without running any driver
# installation. Every function called here (detect_distro, detect_all_gpus,
# detect_cpu) is read-only — no sudo, no package install, no systemd call —
# so this path never needs pkexec, unlike the actual GPU configuration below.
# ---------------------------------------------------------------------------
print_detect_json() {
  printf '__DETECT__{"distro_pretty":"%s","gpu_vendor":"%s","gpu_name":"%s","cpu_model":"%s","cpu_threads":%s}\n' \
    "$(json_escape "$DISTRO_PRETTY")" \
    "$(json_escape "$GPU_VENDOR")" \
    "$(json_escape "$GPU_NAME")" \
    "$(json_escape "$CPU_MODEL")" \
    "$CPU_THREADS"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
detect_all_gpus

if [ "$DETECT_ONLY" -eq 1 ]; then
  detect_cpu
  print_detect_json
  exit 0
fi

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
