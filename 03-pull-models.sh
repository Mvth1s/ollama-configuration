#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# 03-pull-models.sh
# Choisit un palier de modeles (XS/S/M/L) selon la RAM disponible et le
# type de GPU, puis telecharge 4 modeles, un par usage : texte, code,
# reflexion (raisonnement), embeddings.
#
# Usage : ./03-pull-models.sh [--tier=XS|S|M|L]
# Modifie les tableaux MODEL_XS / MODEL_S / MODEL_M / MODEL_L ci dessous pour
# changer les modeles choisis a chaque palier.
# =============================================================================

declare -A MODEL_XS=(
  [texte]="llama3.2:3b"
  [code]="qwen2.5-coder:3b"
  [reflexion]="deepseek-r1:1.5b"
  [embeddings]="nomic-embed-text"
)
declare -A MODEL_S=(
  [texte]="llama3.1:8b"
  [code]="qwen2.5-coder:7b"
  [reflexion]="deepseek-r1:7b"
  [embeddings]="nomic-embed-text"
)
declare -A MODEL_M=(
  [texte]="gemma3:12b"
  [code]="devstral:24b"
  [reflexion]="deepseek-r1:14b"
  [embeddings]="nomic-embed-text"
)
declare -A MODEL_L=(
  [texte]="gemma3:27b"
  [code]="qwen2.5-coder:32b"
  [reflexion]="deepseek-r1:32b"
  [embeddings]="nomic-embed-text"
)

FORCE_TIER=""
for arg in "$@"; do
  case "$arg" in
    --tier=*) FORCE_TIER="${arg#*=}" ;;
    *) log_err "Option inconnue : $arg"; exit 1 ;;
  esac
done

load_state
detect_ram

compute_tier() {
  if [ -n "$FORCE_TIER" ]; then
    TIER="$FORCE_TIER"
    log_info "Palier force manuellement : $TIER"
  else
    if   [ "$RAM_GB" -le 8 ];  then TIER="XS"
    elif [ "$RAM_GB" -le 16 ]; then TIER="S"
    elif [ "$RAM_GB" -le 32 ]; then TIER="M"
    else TIER="L"
    fi

    # CPU pur (ni AMD ni Nvidia ni Intel Vulkan actif) : un 12b+ devient trop
    # lent en pratique, on redescend a S quel que soit le palier RAM brut.
    if [ "${GPU_VENDOR:-none}" = "none" ] && { [ "$TIER" = "M" ] || [ "$TIER" = "L" ]; }; then
      log_warn "Pas de GPU dedie : palier $TIER ramene a S pour rester utilisable en pratique."
      TIER="S"
    fi
  fi

  log_info "Palier de modeles retenu : $TIER"
  save_state TIER
}

compute_tier

declare -n tier_models="MODEL_${TIER}"

for usage in texte code reflexion embeddings; do
  model="${tier_models[$usage]}"
  log_info "Telechargement du modele $usage : $model"
  ollama pull "$model"
done

log_ok "Tous les modeles du palier $TIER sont prets (ollama list pour verifier)."
