#!/usr/bin/env bats
# Tests for toggle-webui-lan.sh: status reporting, the on/off write path, and
# only restarting the service when a unit is actually installed.

load 'test_helper'

setup() {
  setup_sandbox
}

teardown() {
  teardown_sandbox
}

@test "usage: no args prints usage and exits non-zero" {
  run "$REPO_ROOT/toggle-webui-lan.sh"
  [ "$status" -ne 0 ]
  [[ "$output" == *"Usage: ./toggle-webui-lan.sh on|off|status"* ]]
}

@test "usage: unknown arg prints usage and exits non-zero" {
  run "$REPO_ROOT/toggle-webui-lan.sh" frobnicate
  [ "$status" -ne 0 ]
  [[ "$output" == *"Usage:"* ]]
}

@test "status: defaults to OFF/127.0.0.1 when webui.env does not exist yet" {
  run "$REPO_ROOT/toggle-webui-lan.sh" status
  [ "$status" -eq 0 ]
  [[ "$output" == *"LAN access: OFF (this machine only, 127.0.0.1)"* ]]
}

@test "on: writes WEBUI_HOST=0.0.0.0 and warns, without a unit installed" {
  run "$REPO_ROOT/toggle-webui-lan.sh" on
  [ "$status" -eq 0 ]
  grep -q '^WEBUI_HOST=0.0.0.0$' "$TEST_HOME/.config/ollama-stack/webui.env"
  [[ "$output" == *"LAN access enabled"* ]]
  [[ "$output" == *"No login is required by default"* ]]
  [[ "$output" == *"not installed yet"* ]]
}

@test "off after on: restores WEBUI_HOST=127.0.0.1" {
  "$REPO_ROOT/toggle-webui-lan.sh" on
  run "$REPO_ROOT/toggle-webui-lan.sh" off
  [ "$status" -eq 0 ]
  grep -q '^WEBUI_HOST=127.0.0.1$' "$TEST_HOME/.config/ollama-stack/webui.env"
  [[ "$output" == *"LAN access disabled"* ]]
}

@test "on with an installed unit: restarts the service instead of just saving" {
  mkdir -p "$TEST_HOME/.config/systemd/user"
  printf '[Service]\nEnvironment="WEBUI_AUTH=False"\n' > "$TEST_HOME/.config/systemd/user/open-webui.service"
  export STUB_SYSTEMCTL_UNIT_EXISTS=1

  run "$REPO_ROOT/toggle-webui-lan.sh" on
  [ "$status" -eq 0 ]
  [[ "$output" == *"Open WebUI restarted with the new setting."* ]]
  grep -q 'SYSTEMCTL --user restart open-webui' "$STUB_LOG"
}

@test "status: reports LAN ON and reads WEBUI_AUTH from the installed unit" {
  mkdir -p "$TEST_HOME/.config/systemd/user"
  printf '[Service]\nEnvironment="WEBUI_AUTH=False"\n' > "$TEST_HOME/.config/systemd/user/open-webui.service"
  "$REPO_ROOT/toggle-webui-lan.sh" on >/dev/null

  run "$REPO_ROOT/toggle-webui-lan.sh" status
  [ "$status" -eq 0 ]
  [[ "$output" == *"LAN access: ON (reachable from your network)"* ]]
  [[ "$output" == *"Login required (WEBUI_AUTH): False"* ]]
}
