#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# setup.sh
# Chains the 4 scripts in order. Each one remains usable standalone, which
# allows replaying just one step (e.g. reconfigure the GPU after a card swap,
# or re-download models with a different tier) without redoing everything.
#
# Usage:
#   ./setup.sh                  full install, auto-detection
#   ./setup.sh --tier=M         force the model tier
#   ./setup.sh --skip-models    install Ollama + GPU + WebUI without models
#   ./setup.sh --skip-webui     skip Open WebUI installation
#   ./setup.sh --no-tui         disable the interactive dialog/whiptail menus
#
# Or, step by step:
#   ./01-install-ollama.sh
#   ./02-configure-gpu.sh
#   ./03-pull-models.sh --tier=M
#   ./04-install-webui.sh
# =============================================================================

FORCE_TIER=""
SKIP_MODELS=0
SKIP_WEBUI=0
NO_TUI_FLAG=0

for arg in "$@"; do
  case "$arg" in
    --tier=*) FORCE_TIER="${arg#*=}" ;;
    --skip-models) SKIP_MODELS=1 ;;
    --skip-webui) SKIP_WEBUI=1 ;;
    --no-tui) NO_TUI_FLAG=1 ;;
    -h|--help) sed -n '2,21p' "$0"; exit 0 ;;
    *) log_err "Unknown option: $arg"; exit 1 ;;
  esac
done

TUI_ARG=()
if [ "$NO_TUI_FLAG" -eq 1 ]; then
  TUI_ARG=(--no-tui)
fi

./01-install-ollama.sh
./02-configure-gpu.sh "${TUI_ARG[@]}"

if [ "$SKIP_MODELS" -eq 0 ]; then
  if [ -n "$FORCE_TIER" ]; then
    ./03-pull-models.sh --tier="$FORCE_TIER" "${TUI_ARG[@]}"
  else
    ./03-pull-models.sh "${TUI_ARG[@]}"
  fi
else
  log_info "Model download skipped (--skip-models)."
fi

if [ "$SKIP_WEBUI" -eq 0 ]; then
  ./04-install-webui.sh
else
  log_info "Open WebUI installation skipped (--skip-webui)."
fi

load_state
echo
echo "============================================================"
echo " Installation summary"
echo "============================================================"
echo " Distro       : ${DISTRO_PRETTY:-unknown} ($DISTRO_FAMILY)"
echo " GPU          : ${GPU_NAME:-none} (${GPU_VENDOR:-none})"
echo " RAM          : ${RAM_GB:-?} GB"
[ "$SKIP_MODELS" -eq 0 ] && echo " Tier         : ${TIER:-?}"
[ "$SKIP_WEBUI" -eq 0 ] && echo " Web UI       : http://localhost:8080"
echo "============================================================"
echo " Useful commands:"
echo "   ollama list                            list installed models"
echo "   systemctl status ollama                Ollama service status"
echo "   systemctl --user status open-webui     Open WebUI service status"
echo "============================================================"
