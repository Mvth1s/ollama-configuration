#!/usr/bin/env bash
# =============================================================================
# lib/common.sh
# Source from other scripts with: source "$(dirname "$0")/lib/common.sh"
#
# Provides:
#   - logging helpers (log_info, log_ok, log_warn, log_err)
#   - distro detection + package manager wrapper
#   - a small shared state system between scripts (~/.config/ollama-stack)
#     so each script can run standalone or chained after the others
#     without re-detecting everything each time.
# =============================================================================

STATE_DIR="$HOME/.config/ollama-stack"
STATE_FILE="$STATE_DIR/state.env"

log_info() { printf '\033[1;34m[INFO]\033[0m %s\n' "$1"; }
log_ok()   { printf '\033[1;32m[OK]\033[0m %s\n' "$1"; }
log_warn() { printf '\033[1;33m[WARNING]\033[0m %s\n' "$1"; }
log_err()  { printf '\033[1;31m[ERROR]\033[0m %s\n' "$1" >&2; }

# ---------------------------------------------------------------------------
# TUI helpers (dialog, fallback whiptail). Every caller must go through
# tui_available first: it stays false (backend "none") when --no-tui was
# passed, when stdin/stdout is not an interactive terminal (scripted or
# chained via setup.sh), or when neither dialog nor whiptail is installed.
# Callers then fall back to their existing plain read/echo prompts, so no
# new package is required to keep the scripts working everywhere.
# ---------------------------------------------------------------------------
NO_TUI="${NO_TUI:-0}"
TUI_BACKEND=""

detect_tui() {
  [ -n "$TUI_BACKEND" ] && return 0
  if [ "$NO_TUI" -eq 1 ] || [ ! -t 0 ] || [ ! -t 1 ]; then
    TUI_BACKEND="none"
  elif command -v dialog >/dev/null 2>&1; then
    TUI_BACKEND="dialog"
  elif command -v whiptail >/dev/null 2>&1; then
    TUI_BACKEND="whiptail"
  else
    TUI_BACKEND="none"
  fi
}

tui_available() {
  detect_tui
  [ "$TUI_BACKEND" != "none" ]
}

# tui_yesno "Title" "Message" -> exit status 0 = yes, 1 = no/cancel
tui_yesno() {
  "$TUI_BACKEND" --title "$1" --yesno "$2" 10 70
}

# tui_menu "Title" "Message" tag1 item1 tag2 item2 ... -> echoes the chosen tag
tui_menu() {
  local title="$1" msg="$2"
  shift 2
  "$TUI_BACKEND" --title "$title" --menu "$msg" 20 76 10 "$@" 3>&1 1>&2 2>&3
}

# ---------------------------------------------------------------------------
# Shared state between scripts (DISTRO_FAMILY, GPU_VENDOR, GPU_NAME, GPU_GFX,
# RAM_GB, TIER, ...). Each script can load what was already detected by a
# previous script, or re-detect on its own if run standalone.
# ---------------------------------------------------------------------------
load_state() {
  mkdir -p "$STATE_DIR"
  # shellcheck source=/dev/null
  [ -f "$STATE_FILE" ] && . "$STATE_FILE"
  return 0
}

save_state() {
  # usage: save_state VAR1 VAR2 VAR3
  mkdir -p "$STATE_DIR"
  touch "$STATE_FILE"
  for var in "$@"; do
    # remove old value if present, then append the new one
    sed -i "/^${var}=/d" "$STATE_FILE"
    printf '%s=%q\n' "$var" "${!var}" >> "$STATE_FILE"
  done
}

# ---------------------------------------------------------------------------
# Distro: covers the 4 most common package manager families on desktop Linux.
# For everything else (NixOS, Gentoo, Alpine, immutable distros like
# Silverblue, ...) driver installation is skipped with a clear message:
# the user installs manually, and the Ollama binary itself installs the same
# way everywhere (official script, independent of the package manager).
# ---------------------------------------------------------------------------
OS_RELEASE_FILE="${OS_RELEASE_FILE:-/etc/os-release}"

detect_distro() {
  [ -n "${DISTRO_FAMILY:-}" ] && return 0
  DISTRO_FAMILY="unknown"
  DISTRO_PRETTY="unknown"

  if [ -f "$OS_RELEASE_FILE" ]; then
    # shellcheck source=/dev/null
    . "$OS_RELEASE_FILE"
    DISTRO_PRETTY="${PRETTY_NAME:-unknown}"
    case "${ID:-}${ID_LIKE:-}" in
      *arch*)            DISTRO_FAMILY="arch" ;;
      *debian*|*ubuntu*) DISTRO_FAMILY="debian" ;;
      *fedora*|*rhel*)   DISTRO_FAMILY="fedora" ;;
      *suse*)            DISTRO_FAMILY="opensuse" ;;
      *)                 DISTRO_FAMILY="unknown" ;;
    esac
  fi

  log_info "Detected distro: $DISTRO_PRETTY (family: $DISTRO_FAMILY)"
  save_state DISTRO_FAMILY DISTRO_PRETTY
}

pkg_install() {
  case "$DISTRO_FAMILY" in
    arch)
      sudo pacman -Sy --noconfirm --needed "$@"
      ;;
    debian)
      sudo apt-get update -qq
      sudo apt-get install -y "$@"
      ;;
    fedora)
      sudo dnf install -y "$@"
      ;;
    opensuse)
      sudo zypper --non-interactive install "$@"
      ;;
    *)
      log_warn "Distro not recognized by this script (unknown package manager)."
      log_warn "Install these packages manually with your usual tool: $*"
      ;;
  esac
}

detect_ram() {
  [ -n "${RAM_GB:-}" ] && return 0
  RAM_GB=$(free -g | awk '/^Mem:/{print $2}')
  if [ "$RAM_GB" -eq 0 ]; then
    RAM_GB=$(( $(free -m | awk '/^Mem:/{print $2}') / 1024 + 1 ))
  fi
  log_info "Detected total RAM: ${RAM_GB} GB"
  save_state RAM_GB
}
