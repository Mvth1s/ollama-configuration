#!/usr/bin/env bash
# =============================================================================
# lib/common.sh
# A sourcer depuis les autres scripts : source "$(dirname "$0")/lib/common.sh"
#
# Fournit :
#   - logging (log_info, log_ok, log_warn, log_err)
#   - detection de la distro + wrapper d'installation de paquets
#   - un petit systeme d'etat partage entre les scripts (~/.config/ollama-stack)
#     pour que chaque script puisse tourner seul ou enchaine apres les autres
#     sans tout redetecter a chaque fois.
# =============================================================================

STATE_DIR="$HOME/.config/ollama-stack"
STATE_FILE="$STATE_DIR/state.env"

log_info() { printf '\033[1;34m[INFO]\033[0m %s\n' "$1"; }
log_ok()   { printf '\033[1;32m[OK]\033[0m %s\n' "$1"; }
log_warn() { printf '\033[1;33m[ATTENTION]\033[0m %s\n' "$1"; }
log_err()  { printf '\033[1;31m[ERREUR]\033[0m %s\n' "$1" >&2; }

# ---------------------------------------------------------------------------
# Etat partage entre scripts (DISTRO_FAMILY, GPU_VENDOR, GPU_NAME, GPU_GFX,
# RAM_GB, TIER, ...). Chaque script peut charger ce qui a deja ete detecte
# par un script precedent, ou redetecter lui-meme si lance seul.
# ---------------------------------------------------------------------------
load_state() {
  mkdir -p "$STATE_DIR"
  [ -f "$STATE_FILE" ] && . "$STATE_FILE"
  return 0
}

save_state() {
  # usage: save_state VAR1 VAR2 VAR3
  mkdir -p "$STATE_DIR"
  touch "$STATE_FILE"
  for var in "$@"; do
    # supprime l'ancienne valeur si presente, puis ajoute la nouvelle
    sed -i "/^${var}=/d" "$STATE_FILE"
    printf '%s=%q\n' "$var" "${!var}" >> "$STATE_FILE"
  done
}

# ---------------------------------------------------------------------------
# Distro : couvre les 4 familles de gestionnaires de paquets les plus
# courantes sur desktop Linux. Pour tout le reste (NixOS, Gentoo, Alpine,
# distros immutables type Silverblue, ...) l'installation des drivers est
# ignoree avec un message clair : l'utilisateur installe lui meme, le binaire
# Ollama en lui meme s'installe partout pareil (script officiel, independant
# du gestionnaire de paquets).
# ---------------------------------------------------------------------------
detect_distro() {
  [ -n "${DISTRO_FAMILY:-}" ] && return 0
  DISTRO_FAMILY="unknown"
  DISTRO_PRETTY="inconnue"

  if [ -f /etc/os-release ]; then
    . /etc/os-release
    DISTRO_PRETTY="${PRETTY_NAME:-inconnue}"
    case "${ID:-}${ID_LIKE:-}" in
      *arch*)            DISTRO_FAMILY="arch" ;;
      *debian*|*ubuntu*) DISTRO_FAMILY="debian" ;;
      *fedora*|*rhel*)   DISTRO_FAMILY="fedora" ;;
      *suse*)            DISTRO_FAMILY="opensuse" ;;
      *)                 DISTRO_FAMILY="unknown" ;;
    esac
  fi

  log_info "Distro detectee : $DISTRO_PRETTY (famille: $DISTRO_FAMILY)"
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
      log_warn "Distro non reconnue par ce script (gestionnaire de paquets inconnu)."
      log_warn "Installe ces paquets manuellement avec ton outil habituel : $*"
      ;;
  esac
}

detect_ram() {
  [ -n "${RAM_GB:-}" ] && return 0
  RAM_GB=$(free -g | awk '/^Mem:/{print $2}')
  if [ "$RAM_GB" -eq 0 ]; then
    RAM_GB=$(( $(free -m | awk '/^Mem:/{print $2}') / 1024 + 1 ))
  fi
  log_info "RAM totale detectee : ${RAM_GB} Go"
  save_state RAM_GB
}
