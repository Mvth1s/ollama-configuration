#!/usr/bin/env bats
# Tests 01-install-ollama.sh's idempotent "already installed" path only: the
# curl-pipe-to-sh install branch is deliberately not exercised here (it would
# mean actually running a downloaded installer, which this suite never does).

load 'test_helper'

setup() {
  setup_sandbox
  # `ollama` present in PATH => the script must skip straight to service
  # management instead of re-installing.
  stub_cmd ollama 'if [ "$1" = "--version" ]; then echo "ollama version 0.0.0-test"; fi; exit 0'
  # The readiness probe polls this URL; answering immediately keeps the test
  # from waiting through the script's real 30 x 1s retry loop.
  stub_cmd curl 'exit 0'
}

teardown() {
  teardown_sandbox
}

@test "already installed: skips the install script and reports ready" {
  run "$REPO_ROOT/01-install-ollama.sh"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Ollama already installed"* ]]
  [[ "$output" == *"Ollama is ready at http://127.0.0.1:11434"* ]]
  grep -q 'SUDO systemctl enable --now ollama' "$STUB_LOG"
}
