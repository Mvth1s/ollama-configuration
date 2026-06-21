#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# 01-install-ollama.sh
# Installe Ollama (script officiel, identique sur toutes les distros) et
# s'assure que le service tourne. Peut etre relance seul a tout moment.
# =============================================================================

load_state
detect_distro

if command -v ollama >/dev/null 2>&1; then
  log_ok "Ollama deja installe ($(ollama --version 2>/dev/null | head -n1))."
else
  log_info "Installation d'Ollama..."
  curl -fsSL https://ollama.com/install.sh | sh
fi

if command -v systemctl >/dev/null 2>&1; then
  sudo systemctl daemon-reload
  sudo systemctl enable --now ollama
else
  log_warn "systemd non detecte, demarre Ollama manuellement avec : ollama serve"
fi

log_info "Attente du demarrage du service Ollama..."
for _ in $(seq 1 30); do
  if curl -fs http://127.0.0.1:11434 >/dev/null 2>&1; then
    log_ok "Ollama est pret sur http://127.0.0.1:11434"
    exit 0
  fi
  sleep 1
done

log_err "Ollama n'a pas demarre a temps. Verifie : systemctl status ollama"
exit 1
