#!/usr/bin/env bash
# All MODEL_<TIER> / CAND_<TIER>_<usage> arrays below are only ever read
# through dynamic `declare -n` namerefs built from tier/usage strings at
# runtime (compute_tier, select_models_tui), which ShellCheck's static
# analysis cannot follow — hence this file-wide disable rather than ~20
# repeats of the same false positive.
# shellcheck disable=SC2034
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
CAND_XS_texte=("llama3.2:3b|Fast, solid generalist for low-end hardware" "qwen2.5:3b|Multilingual alternative" "phi3.5:3.8b|Compact, decent basic reasoning")
CAND_XS_code=("qwen2.5-coder:3b|Lightweight general-purpose coding model" "starcoder2:3b|Alternative geared toward completion")
CAND_XS_reflexion=("deepseek-r1:1.5b|Step-by-step reasoning, very lightweight" "qwen2.5:1.5b|Lightweight generalist alternative")
CAND_XS_embeddings=("nomic-embed-text|Standard general-purpose embeddings" "all-minilm|Lighter, faster")

CAND_S_texte=("llama3.1:8b|Well-balanced generalist" "gemma2:9b|Google alternative, good instruction following" "mistral:7b|Fast, good tradeoff")
CAND_S_code=("qwen2.5-coder:7b|General-purpose coding model" "codellama:7b|Meta alternative, geared toward completion")
CAND_S_reflexion=("deepseek-r1:7b|Step-by-step reasoning" "qwen2.5:7b|Generalist alternative")
CAND_S_embeddings=("nomic-embed-text|Standard general-purpose embeddings" "all-minilm|Lighter, faster")

CAND_M_texte=("gemma3:12b|Recent Google generalist" "mistral-nemo:12b|Mistral/Nvidia alternative" "qwen2.5:14b|Bigger, better general reasoning")
CAND_M_code=("devstral:24b|Geared toward coding agents" "qwen2.5-coder:14b|Lighter alternative")
CAND_M_reflexion=("deepseek-r1:14b|Step-by-step reasoning" "qwen2.5:14b|Generalist alternative")
CAND_M_embeddings=("nomic-embed-text|Standard general-purpose embeddings" "mxbai-embed-large|More accurate, heavier")

CAND_L_texte=("gemma3:27b|Large Google generalist" "qwen2.5:32b|Alibaba alternative" "mixtral:8x7b|Mixture-of-experts, good speed/quality tradeoff")
CAND_L_code=("qwen2.5-coder:32b|Large general-purpose coding model" "devstral:24b|Alternative geared toward coding agents")
CAND_L_reflexion=("deepseek-r1:32b|Large step-by-step reasoning model" "qwq:32b|Alibaba alternative geared toward reasoning")
CAND_L_embeddings=("nomic-embed-text|Standard general-purpose embeddings" "mxbai-embed-large|More accurate, heavier")

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

    chosen=$(tui_menu "$usage model (tier $TIER)" "Choose the $usage model:" "${menu_args[@]}") || chosen=""
    if [ -n "$chosen" ]; then
      # tier_models is a nameref to an associative array (string keys), not
      # an indexed one: dropping $ here would silently write to a literal
      # "usage" key instead of the intended tier/usage, so keep it despite
      # ShellCheck's arithmetic-subscript suggestion (verified in bash).
      # shellcheck disable=SC2004
      tier_models[$usage]="$chosen"
    else
      log_warn "Selection cancelled for $usage, default kept (${tier_models[$usage]})."
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
