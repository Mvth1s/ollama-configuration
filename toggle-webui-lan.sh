#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source lib/common.sh

# =============================================================================
# toggle-webui-lan.sh
# Switches Open WebUI between "this machine only" (127.0.0.1, the default set
# by 04-install-webui.sh) and "reachable from the local network" (0.0.0.0),
# at any time after install, without re-running the installer. Only touches
# the small $STATE_DIR/webui.env file the systemd unit reads WEBUI_HOST from,
# then reloads/restarts the service.
#
# Usage: ./toggle-webui-lan.sh on|off|status
# =============================================================================

usage() {
  cat <<'EOF'
Usage: ./toggle-webui-lan.sh on|off|status

  on      Allow Open WebUI to be reached from other devices on your network
          (e.g. a phone). No login is required by default (WEBUI_AUTH=False).
  off     Restrict Open WebUI to this machine only (127.0.0.1). Default.
  status  Show the current setting.
EOF
  exit 1
}

[ $# -eq 1 ] || usage

WEBUI_ENV_FILE="$STATE_DIR/webui.env"
UNIT_FILE="$HOME/.config/systemd/user/open-webui.service"

current_host() {
  if [ -f "$WEBUI_ENV_FILE" ]; then
    grep -m1 '^WEBUI_HOST=' "$WEBUI_ENV_FILE" | cut -d= -f2-
  else
    echo "127.0.0.1"
  fi
}

case "$1" in
  status)
    host="$(current_host)"
    if [ "$host" = "0.0.0.0" ]; then
      log_info "LAN access: ON (reachable from your network)"
    else
      log_info "LAN access: OFF (this machine only, $host)"
    fi
    if [ -f "$UNIT_FILE" ]; then
      auth="$(grep -oE 'WEBUI_AUTH=[^"]*' "$UNIT_FILE" 2>/dev/null | head -n1 | cut -d= -f2)"
      log_info "Login required (WEBUI_AUTH): ${auth:-unknown}"
    fi
    exit 0
    ;;
  on)  new_host="0.0.0.0" ;;
  off) new_host="127.0.0.1" ;;
  *)   usage ;;
esac

mkdir -p "$STATE_DIR"
touch "$WEBUI_ENV_FILE"
sed -i '/^WEBUI_HOST=/d' "$WEBUI_ENV_FILE"
echo "WEBUI_HOST=$new_host" >> "$WEBUI_ENV_FILE"

if [ "$new_host" = "0.0.0.0" ]; then
  log_warn "LAN access enabled: Open WebUI will be reachable from other devices on your network."
  log_warn "No login is required by default (WEBUI_AUTH=False): anyone on your network can use it."
  log_warn "Require a login instead: edit WEBUI_AUTH in $UNIT_FILE to \"True\", then:"
  log_warn "  systemctl --user daemon-reload && systemctl --user restart open-webui"
else
  log_info "LAN access disabled: Open WebUI is now restricted to this machine only."
fi

if [ -f "$UNIT_FILE" ] && systemctl --user list-unit-files open-webui.service >/dev/null 2>&1; then
  systemctl --user daemon-reload
  systemctl --user restart open-webui
  log_ok "Open WebUI restarted with the new setting."
else
  log_info "Open WebUI is not installed yet; this setting will be applied automatically when you run ./04-install-webui.sh or ./setup.sh."
fi
