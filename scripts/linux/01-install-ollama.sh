#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# 01-install-ollama.sh
# Installs Ollama (official script, identical on all distros) and ensures
# the service is running. Can be re-run at any time.
# =============================================================================

load_state
detect_distro

if command -v ollama >/dev/null 2>&1; then
  log_ok "Ollama already installed ($(ollama --version 2>/dev/null | head -n1))."
else
  log_info "Installing Ollama..."
  curl -fsSL https://ollama.com/install.sh | sh
fi

if command -v systemctl >/dev/null 2>&1; then
  sudo systemctl daemon-reload
  sudo systemctl enable --now ollama
else
  log_warn "systemd not detected, start Ollama manually with: ollama serve"
fi

log_info "Waiting for Ollama service to start..."
for _ in $(seq 1 30); do
  if curl -fs http://127.0.0.1:11434 >/dev/null 2>&1; then
    log_ok "Ollama is ready at http://127.0.0.1:11434"
    exit 0
  fi
  sleep 1
done

log_err "Ollama did not start in time. Check: systemctl status ollama"
exit 1
