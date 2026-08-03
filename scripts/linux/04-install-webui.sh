#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# 04-install-webui.sh
# Installs Open WebUI (without Docker, via pip/pipx) and runs it as a
# user-level systemd service on http://localhost:8080, as a GUI alternative
# to the terminal for chatting with Ollama models.
#
# By default the service is reachable from this machine only (127.0.0.1).
# Use ./toggle-webui-lan.sh on|off|status to allow/restrict access from other
# devices on the network at any time, without re-running this script.
# =============================================================================

load_state
detect_distro

# python3/pip/pipx via pkg_install always shells out to `sudo` (see
# lib/common.sh), which is fine for a human running this script directly in a
# terminal but breaks when the GUI runs it as its deliberately-unprivileged
# webui step (Stdio::null() stdin, and often no controlling terminal at all):
# sudo can't prompt and the whole process hangs waiting for a password that
# can never arrive. install_webui_deps is only ever run through pkexec (see
# gui/src-tauri/src/main.rs's "Installing Open WebUI prerequisites" step,
# grouped with the other privileged steps) before this script's normal,
# unprivileged run - and skips the pkg_install call entirely if pipx is
# already present, so that normal run never has a reason to invoke sudo.
install_webui_deps() {
  command -v pipx >/dev/null 2>&1 && return 0
  case "$DISTRO_FAMILY" in
    arch)     pkg_install python python-pip pipx ;;
    debian)   pkg_install python3 python3-pip pipx ;;
    fedora)   pkg_install python3 python3-pip pipx ;;
    opensuse) pkg_install python3 python3-pip python3-pipx ;;
  esac
}

if [ "${1:-}" = "--install-deps" ]; then
  install_webui_deps
  exit 0
fi

log_info "Installing Open WebUI..."

install_webui_deps

if command -v pipx >/dev/null 2>&1; then
  pipx ensurepath >/dev/null 2>&1 || true
  pipx install open-webui || pipx upgrade open-webui
  WEBUI_BIN="$(pipx environment --value PIPX_BIN_DIR 2>/dev/null)/open-webui"
else
  case "$DISTRO_FAMILY" in
    debian) pip install --break-system-packages --upgrade open-webui ;;
    *)      pip install --user --upgrade open-webui ;;
  esac
  WEBUI_BIN="$(command -v open-webui)"
fi

mkdir -p ~/.config/systemd/user

# WEBUI_HOST lives in its own small env file rather than directly in the unit,
# so toggle-webui-lan.sh can flip it later without regenerating the whole
# service file. Only created if missing, so re-running this script (e.g. to
# upgrade Open WebUI) never resets a LAN-access choice made after install.
WEBUI_ENV_FILE="$STATE_DIR/webui.env"
if [ ! -f "$WEBUI_ENV_FILE" ]; then
  mkdir -p "$STATE_DIR"
  echo "WEBUI_HOST=127.0.0.1" > "$WEBUI_ENV_FILE"
fi

# systemd substitutes $WEBUI_HOST into ExecStart from EnvironmentFile at
# start time (not this script), so it is intentionally left unexpanded here.
cat > ~/.config/systemd/user/open-webui.service <<EOF
[Unit]
Description=Open WebUI (local web interface for Ollama)
After=network.target

[Service]
EnvironmentFile=$WEBUI_ENV_FILE
Environment="OLLAMA_BASE_URL=http://127.0.0.1:11434"
Environment="WEBUI_AUTH=False"
ExecStart=$WEBUI_BIN serve --port 8080 --host \${WEBUI_HOST}
Restart=on-failure

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now open-webui

log_ok "Open WebUI started. Available at http://localhost:8080"
log_info "To start it without an active login session: sudo loginctl enable-linger \$USER"

if [ "$(grep -m1 '^WEBUI_HOST=' "$WEBUI_ENV_FILE" | cut -d= -f2-)" = "0.0.0.0" ]; then
  log_warn "LAN access is enabled: Open WebUI is reachable from other devices on your network."
  log_warn "No login is required by default (WEBUI_AUTH=False): anyone on your network can use it."
  log_warn "Restrict to this machine only: ./toggle-webui-lan.sh off"
  log_warn "Require a login instead: edit WEBUI_AUTH in ~/.config/systemd/user/open-webui.service to \"True\", then:"
  log_warn "  systemctl --user daemon-reload && systemctl --user restart open-webui"
else
  log_info "Open WebUI is restricted to this machine only. Run ./toggle-webui-lan.sh on to allow access from other devices (e.g. a phone)."
fi
