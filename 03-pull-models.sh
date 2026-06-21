#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# 03-pull-models.sh
# Selects a model tier (XS/S/M/L) based on available RAM and GPU type,
# then downloads 4 models, one per use case: text, code, reasoning, embeddings.
#
# Usage: ./03-pull-models.sh [--tier=XS|S|M|L]
# Edit the MODEL_XS / MODEL_S / MODEL_M / MODEL_L arrays below to change
# which models are selected at each tier.
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
    *) log_err "Unknown option: $arg"; exit 1 ;;
  esac
done

load_state
detect_ram

compute_tier() {
  if [ -n "$FORCE_TIER" ]; then
    TIER="$FORCE_TIER"
    log_info "Tier manually forced: $TIER"
  else
    if   [ "$RAM_GB" -le 8 ];  then TIER="XS"
    elif [ "$RAM_GB" -le 16 ]; then TIER="S"
    elif [ "$RAM_GB" -le 32 ]; then TIER="M"
    else TIER="L"
    fi

    # CPU only (no AMD, Nvidia, or active Intel Vulkan): a 12b+ model becomes
    # too slow in practice, so we drop down to S regardless of raw RAM tier.
    if [ "${GPU_VENDOR:-none}" = "none" ] && { [ "$TIER" = "M" ] || [ "$TIER" = "L" ]; }; then
      log_warn "No dedicated GPU: tier $TIER reduced to S to remain usable in practice."
      TIER="S"
    fi
  fi

  log_info "Selected model tier: $TIER"
  save_state TIER
}

compute_tier

declare -n tier_models="MODEL_${TIER}"

for usage in texte code reflexion embeddings; do
  model="${tier_models[$usage]}"
  log_info "Downloading $usage model: $model"
  ollama pull "$model"
done

log_ok "All models for tier $TIER are ready (run 'ollama list' to verify)."
