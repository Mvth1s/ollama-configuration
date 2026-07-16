#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# 03-pull-models.sh
# Selects a model tier (XS/S/M/L) based on available RAM and GPU type,
# then downloads 4 models, one per use case: text, code, reasoning, embeddings.
#
# Usage: ./03-pull-models.sh [--tier=XS|S|M|L] [--no-tui]
# Edit the MODEL_XS / MODEL_S / MODEL_M / MODEL_L arrays below to change
# the default model selected at each tier, and the CAND_<TIER>_<usage>
# arrays to change the candidates offered in the interactive TUI menu.
#
# The TUI model-selection menu only appears when: no --tier was forced
# (forcing a tier implies a scripted, non-interactive call), --no-tui was
# not passed, and a dialog/whiptail backend is available in an interactive
# terminal. Otherwise the single default model per tier (tables below) is
# used, so setup.sh and any scripted invocation keep working unattended.
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

# ---------------------------------------------------------------------------
# Interactive candidates per tier/usage, format "model|short description".
# The first entry always matches the corresponding MODEL_<TIER> default above.
# ---------------------------------------------------------------------------
CAND_XS_texte=("llama3.2:3b|Rapide, bon généraliste pour petite config" "qwen2.5:3b|Alternative multilingue" "phi3.5:3.8b|Compact, bon raisonnement de base")
CAND_XS_code=("qwen2.5-coder:3b|Généraliste code léger" "starcoder2:3b|Alternative orientée complétion")
CAND_XS_reflexion=("deepseek-r1:1.5b|Raisonnement pas à pas, très léger" "qwen2.5:1.5b|Alternative généraliste légère")
CAND_XS_embeddings=("nomic-embed-text|Embeddings généralistes, standard" "all-minilm|Plus léger, plus rapide")

CAND_S_texte=("llama3.1:8b|Généraliste équilibré" "gemma2:9b|Alternative Google, bon suivi d'instructions" "mistral:7b|Rapide, bon compromis")
CAND_S_code=("qwen2.5-coder:7b|Généraliste code" "codellama:7b|Alternative Meta, orientée complétion")
CAND_S_reflexion=("deepseek-r1:7b|Raisonnement pas à pas" "qwen2.5:7b|Alternative généraliste")
CAND_S_embeddings=("nomic-embed-text|Embeddings généralistes, standard" "all-minilm|Plus léger, plus rapide")

CAND_M_texte=("gemma3:12b|Généraliste Google récent" "mistral-nemo:12b|Alternative Mistral/Nvidia" "qwen2.5:14b|Plus grand, meilleur raisonnement général")
CAND_M_code=("devstral:24b|Orienté agents de code" "qwen2.5-coder:14b|Alternative plus légère")
CAND_M_reflexion=("deepseek-r1:14b|Raisonnement pas à pas" "qwen2.5:14b|Alternative généraliste")
CAND_M_embeddings=("nomic-embed-text|Embeddings généralistes, standard" "mxbai-embed-large|Plus précis, plus lourd")

CAND_L_texte=("gemma3:27b|Généraliste Google, gros modèle" "qwen2.5:32b|Alternative Alibaba" "mixtral:8x7b|Mixture-of-experts, bon compromis vitesse/qualité")
CAND_L_code=("qwen2.5-coder:32b|Généraliste code, gros modèle" "devstral:24b|Alternative orientée agents de code")
CAND_L_reflexion=("deepseek-r1:32b|Raisonnement pas à pas, gros modèle" "qwq:32b|Alternative Alibaba orientée raisonnement")
CAND_L_embeddings=("nomic-embed-text|Embeddings généralistes, standard" "mxbai-embed-large|Plus précis, plus lourd")

FORCE_TIER=""
for arg in "$@"; do
  case "$arg" in
    --tier=*) FORCE_TIER="${arg#*=}" ;;
    --no-tui) NO_TUI=1 ;;
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

# ---------------------------------------------------------------------------
# Interactive selection: for each usage, let the user pick among the tier's
# candidates instead of the single default. Skipped (default kept) when the
# tier was forced, or when no TUI backend is available/usable.
# ---------------------------------------------------------------------------
select_models_tui() {
  local usage entry model desc chosen
  local -a menu_args
  for usage in texte code reflexion embeddings; do
    local -n candidates="CAND_${TIER}_${usage}"
    menu_args=()
    for entry in "${candidates[@]}"; do
      model="${entry%%|*}"
      desc="${entry#*|}"
      menu_args+=("$model" "$desc")
    done

    chosen=$(tui_menu "Modèle $usage (tier $TIER)" "Choisissez le modèle $usage :" "${menu_args[@]}") || chosen=""
    if [ -n "$chosen" ]; then
      tier_models[$usage]="$chosen"
    else
      log_warn "Sélection annulée pour $usage, valeur par défaut conservée (${tier_models[$usage]})."
    fi
  done
}

if [ -z "$FORCE_TIER" ] && tui_available; then
  select_models_tui
fi

for usage in texte code reflexion embeddings; do
  model="${tier_models[$usage]}"
  log_info "Downloading $usage model: $model"
  ollama pull "$model"
done

log_ok "All models for tier $TIER are ready (run 'ollama list' to verify)."
