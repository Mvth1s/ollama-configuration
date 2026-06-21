#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# setup.sh
# Enchaine les 4 scripts dans l'ordre. Chacun reste utilisable seul, ce qui
# permet de rejouer juste une etape (ex: reconfigurer le GPU apres un
# changement de carte, ou retelecharger les modeles avec un autre palier)
# sans tout refaire.
#
# Usage :
#   ./setup.sh                  installation complete, detection auto
#   ./setup.sh --tier=M          force le palier de modeles
#   ./setup.sh --skip-models     installe Ollama + GPU + WebUI sans modeles
#   ./setup.sh --skip-webui      n'installe pas Open WebUI
#
# Ou, etape par etape :
#   ./01-install-ollama.sh
#   ./02-configure-gpu.sh
#   ./03-pull-models.sh --tier=M
#   ./04-install-webui.sh
# =============================================================================

FORCE_TIER=""
SKIP_MODELS=0
SKIP_WEBUI=0

for arg in "$@"; do
  case "$arg" in
    --tier=*) FORCE_TIER="${arg#*=}" ;;
    --skip-models) SKIP_MODELS=1 ;;
    --skip-webui) SKIP_WEBUI=1 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) log_err "Option inconnue : $arg"; exit 1 ;;
  esac
done

./01-install-ollama.sh
./02-configure-gpu.sh

if [ "$SKIP_MODELS" -eq 0 ]; then
  if [ -n "$FORCE_TIER" ]; then
    ./03-pull-models.sh --tier="$FORCE_TIER"
  else
    ./03-pull-models.sh
  fi
else
  log_info "Telechargement des modeles ignore (--skip-models)."
fi

if [ "$SKIP_WEBUI" -eq 0 ]; then
  ./04-install-webui.sh
else
  log_info "Installation d'Open WebUI ignoree (--skip-webui)."
fi

load_state
echo
echo "============================================================"
echo " Resume de l'installation"
echo "============================================================"
echo " Distro       : ${DISTRO_PRETTY:-inconnue} ($DISTRO_FAMILY)"
echo " GPU          : ${GPU_NAME:-aucun} (${GPU_VENDOR:-none})"
echo " RAM          : ${RAM_GB:-?} Go"
[ "$SKIP_MODELS" -eq 0 ] && echo " Palier       : ${TIER:-?}"
[ "$SKIP_WEBUI" -eq 0 ] && echo " Interface web: http://localhost:8080"
echo "============================================================"
echo " Commandes utiles :"
echo "   ollama list                            liste les modeles installes"
echo "   systemctl status ollama                etat du service Ollama"
echo "   systemctl --user status open-webui     etat de l'interface web"
echo "============================================================"
