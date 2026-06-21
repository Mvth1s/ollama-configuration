#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# 02-configure-gpu.sh
# Detecte TOUS les controleurs graphiques presents (un PC portable a souvent
# un GPU Intel integre + un GPU Nvidia ou AMD dedie en meme temps), choisit
# le plus puissant, et configure le backend d'acceleration adapte :
#   - Nvidia  : CUDA, marche de la meme facon sur toutes les cartes Nvidia
#               supportees par le driver installe, pas de cas particulier
#               carte par carte necessaire.
#   - AMD     : ROCm/HIP par defaut. Pour les puces trop recentes pour etre
#               deja dans la liste officiellement supportee par ROCm, bascule
#               automatiquement sur le backend Vulkan avec le contournement
#               HSA_OVERRIDE_GFX_VERSION connu pour cette generation (detecte
#               via le code gfx reel de la puce, pas via le nom commercial,
#               donc ca couvre toute la gamme RDNA4 d'un coup, pas juste 3-4
#               references).
#   - Intel   : pas de backend dedie dans Ollama, mais Mesa fournit un driver
#               Vulkan (ANV) pour tous les iGPU Xe/Iris et les GPU Arc. On
#               active le backend Vulkan d'Ollama pour en profiter. C'est en
#               best effort, le support Intel via Vulkan est plus recent et
#               moins eprouve que CUDA/ROCm : si ca ne s'active pas, Ollama
#               retombe simplement sur le CPU sans planter.
#   - Aucun GPU dedie : rien a faire, CPU.
# =============================================================================

load_state
detect_distro

# ---------------------------------------------------------------------------
# Detection de tous les controleurs graphiques, choix du "meilleur"
# (priorite : Nvidia dedie > AMD dedie > Intel) en cas de config hybride
# (typique sur portable : iGPU Intel + dGPU Nvidia/AMD).
# ---------------------------------------------------------------------------
GPU_VENDOR="none"
GPU_NAME=""

detect_all_gpus() {
  local lines
  lines=$(lspci -nnk 2>/dev/null | grep -Ei 'vga|3d|display' || true)

  if [ -z "$lines" ]; then
    log_warn "Aucun controleur graphique detecte via lspci."
    return
  fi

  log_info "Controleurs graphiques detectes :"
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

  log_info "GPU retenu pour l'acceleration : ${GPU_NAME:-aucun} ($GPU_VENDOR)"
  save_state GPU_VENDOR GPU_NAME
}

# ---------------------------------------------------------------------------
# AMD : detection du code gfx reel via rocminfo plutot que via le nom
# commercial de la carte. Ca couvre n'importe quelle puce AMD, presente ou
# future, sans avoir a maintenir une liste de noms de cartes a la main.
# ---------------------------------------------------------------------------

# Generations connues pour ne PAS encore etre dans la liste officiellement
# supportee par ROCm au moment de l'ecriture de ce script (donc necessitant
# le contournement Vulkan + HSA_OVERRIDE_GFX_VERSION). A completer au fil du
# temps si ROCm ajoute le support officiel ou si une nouvelle generation sort
# avant d'etre supportee.
declare -A AMD_GFX_OVERRIDE=(
  ["gfx1200"]="11.5.0"   # RDNA4 (RX 9060 / 9060 XT)
  ["gfx1201"]="11.5.0"   # RDNA4 (RX 9070 / 9070 XT)
)

configure_amd() {
  log_info "Configuration GPU AMD..."
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
    log_warn "Code gfx non detecte (rocminfo absent ou puce non reconnue)."
    log_warn "Activation du backend Vulkan par defaut, sans contournement specifique."
    write_amd_override "" 
    return
  fi

  log_info "Puce AMD identifiee : $gfx"

  if [ -n "${AMD_GFX_OVERRIDE[$gfx]:-}" ]; then
    local override="${AMD_GFX_OVERRIDE[$gfx]}"
    log_warn "$gfx pas encore officiellement supporte par ROCm a ce jour."
    log_warn "Contournement applique : OLLAMA_VULKAN=1 + HSA_OVERRIDE_GFX_VERSION=$override"
    write_amd_override "$override"
  else
    log_ok "$gfx est deja supporte officiellement par ROCm, config par defaut (HIP)."
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
# Nvidia : CUDA fonctionne de la meme maniere sur toute la gamme supportee
# par le driver, aucune configuration specifique par modele de carte n'est
# necessaire ici, contrairement a AMD.
# ---------------------------------------------------------------------------
configure_nvidia() {
  log_info "Configuration GPU Nvidia (CUDA)..."
  clear_ollama_override

  if command -v nvidia-smi >/dev/null 2>&1; then
    log_ok "Driver Nvidia deja present : $(nvidia-smi --query-gpu=name --format=csv,noheader | head -n1)"
    return
  fi

  log_warn "Aucun driver Nvidia detecte, Ollama utilisera le CPU tant qu'il n'est pas installe."
  read -r -p "Installer le driver Nvidia maintenant ? (necessite un redemarrage apres) [o/N] " reply
  if [[ "$reply" =~ ^[oOyY]$ ]]; then
    case "$DISTRO_FAMILY" in
      arch)     pkg_install nvidia nvidia-utils ;;
      debian)   pkg_install nvidia-driver ;;
      fedora)   pkg_install akmod-nvidia ;;
      opensuse) log_warn "Sur openSUSE, ajoute le depot NVIDIA officiel puis installe x11-video-nvidiaG06." ;;
    esac
    log_warn "Redemarre la machine puis relance ce script pour finaliser la config."
    exit 0
  fi
}

# ---------------------------------------------------------------------------
# Intel : pas de backend dedie dans Ollama, on passe par Vulkan (driver Mesa
# ANV), qui couvre aussi bien les iGPU (Xe, Iris Xe, UHD) que les GPU Arc
# dedies. Best effort : si Ollama ne sait pas l'utiliser, retombe sur le CPU
# automatiquement, sans erreur bloquante.
# ---------------------------------------------------------------------------
configure_intel() {
  log_info "Configuration GPU Intel (Vulkan, best effort)..."
  case "$DISTRO_FAMILY" in
    arch)     pkg_install vulkan-intel vulkan-icd-loader vulkan-tools ;;
    debian)   pkg_install mesa-vulkan-drivers vulkan-tools ;;
    fedora)   pkg_install mesa-vulkan-drivers vulkan-tools ;;
    opensuse) pkg_install vulkan-tools ;;
  esac

  sudo mkdir -p /etc/systemd/system/ollama.service.d
  printf '[Service]\nEnvironment="OLLAMA_VULKAN=1"\n' | sudo tee /etc/systemd/system/ollama.service.d/override.conf >/dev/null
  sudo systemctl daemon-reload

  log_warn "Support Intel via Vulkan plus recent que CUDA/ROCm : verifie les perfs reelles,"
  log_warn "et desactive (rm /etc/systemd/system/ollama.service.d/override.conf) si pire que le CPU seul."
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
detect_all_gpus

case "$GPU_VENDOR" in
  amd)    configure_amd ;;
  nvidia) configure_nvidia ;;
  intel)  configure_intel ;;
  none)   log_info "Pas de GPU dedie, Ollama tournera sur CPU." ; clear_ollama_override ;;
esac

if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet ollama 2>/dev/null; then
  sudo systemctl restart ollama
fi

log_ok "Configuration GPU terminee."
