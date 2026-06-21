#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# 04-install-webui.sh
# Installs Open WebUI (without Docker, via pip/pipx) and runs it as a
# user-level systemd service on http://localhost:8080, as a GUI alternative
# to the terminal for chatting with Ollama models.
# =============================================================================

load_state
detect_distro

log_info "Installing Open WebUI..."

case "$DISTRO_FAMILY" in
  arch)     pkg_install python python-pip pipx ;;
  debian)   pkg_install python3 python3-pip pipx ;;
  fedora)   pkg_install python3 python3-pip pipx ;;
  opensuse) pkg_install python3 python3-pip python3-pipx ;;
esac

if command -v pipx >/dev/null 2>&1; then
  pipx ensurepath >/dev/null 2>&1 || true
  pipx install open-webui || pipx upgrade open-webui
  WEBUI_BIN="$(pipx environment --value PIPX_BIN_DIR 2>/dev/null)/open-webui"
else
  pip install --break-system-packages --upgrade open-webui
  WEBUI_BIN="$(command -v open-webui)"
fi

mkdir -p ~/.config/systemd/user

cat > ~/.config/systemd/user/open-webui.service <<EOF
[Unit]
Description=Open WebUI (local web interface for Ollama)
After=network.target

[Service]
Environment="OLLAMA_BASE_URL=http://127.0.0.1:11434"
Environment="WEBUI_AUTH=False"
ExecStart=$WEBUI_BIN serve --port 8080
Restart=on-failure

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now open-webui

log_ok "Open WebUI started. Available at http://localhost:8080"
log_info "To start it without an active login session: sudo loginctl enable-linger \$USER"
