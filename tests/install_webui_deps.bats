#!/usr/bin/env bats
# Tests 04-install-webui.sh's --install-deps flag: the privileged pre-step
# the GUI now runs via pkexec (see CLAUDE.md's Desktop GUI section) so its
# own, deliberately-unprivileged run of this script never has to shell out to
# `sudo` itself (which used to hang forever under the GUI's non-interactive
# invocation - a real bug found and reported by a user against v1.1.3).
#
# The "pipx not yet installed" branch (which actually calls pkg_install) is
# not separately exercised here: it's the same pkg_install already used
# throughout the other scripts, and reliably faking a real command's
# *absence* when the test-running machine may already have pipx installed
# (true on both this repo's dev machine and GitHub-hosted ubuntu-latest
# runners) isn't something `command -v` can be made to lie about from within
# a bats test without much more invasive PATH surgery than it's worth here.

load 'test_helper'

setup() {
  setup_sandbox
  write_os_release 'ID=ubuntu' 'ID_LIKE=debian'
  # Present in the stub PATH => install_webui_deps's `command -v pipx` check
  # succeeds, so it must skip pkg_install (and therefore never call sudo).
  stub_cmd pipx 'exit 0'
}

teardown() {
  teardown_sandbox
}

write_os_release() {
  TEST_OS_RELEASE="$TEST_HOME/os-release"
  printf '%s\n' "$@" > "$TEST_OS_RELEASE"
  export OS_RELEASE_FILE="$TEST_OS_RELEASE"
}

@test "--install-deps: skips pkg_install (no sudo call) when pipx is already present" {
  run "$REPO_ROOT/04-install-webui.sh" --install-deps
  [ "$status" -eq 0 ]
  [ ! -s "$STUB_LOG" ]
}

@test "--install-deps: exits before the rest of the script runs" {
  run "$REPO_ROOT/04-install-webui.sh" --install-deps
  [ "$status" -eq 0 ]
  [[ "$output" != *"Installing Open WebUI..."* ]]
}
