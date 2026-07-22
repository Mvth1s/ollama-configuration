# Shared setup for the bats suite.
#
# Every test runs against a throwaway $HOME (so state.env / webui.env /
# systemd unit files never touch the real machine) and a stub bin/ directory
# prepended to PATH (so lspci, rocminfo, nvidia-smi, sudo, systemctl, curl and
# ollama are all fakes controlled by the test, never the real system tools).
# This is what makes it safe to run this suite on a developer's own machine,
# not just in CI.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

setup_sandbox() {
  TEST_HOME="$(mktemp -d)"
  STUB_BIN="$(mktemp -d)"
  STUB_LOG="$TEST_HOME/stub.log"
  export HOME="$TEST_HOME"
  export PATH="$STUB_BIN:$PATH"
  export STUB_LOG
  export NO_TUI=1

  # sudo/systemctl are stubbed by default in every test: real ones must
  # never run as a side effect of the suite, regardless of what the
  # invoking script tries to do or what is actually installed on the host.
  stub_cmd sudo '
    # Only drain stdin for `sudo tee`: the real scripts pipe into it, and a
    # stub that exits without reading would SIGPIPE the writer side under
    # set -o pipefail. Draining unconditionally for every sudo call would
    # also eat stdin meant for a later `read -r -p` (e.g. the Nvidia driver
    # install prompt) further down the same script.
    if [ "$1" = "tee" ]; then
      cat >/dev/null 2>&1 || true
    fi
    echo "SUDO $*" >> "$STUB_LOG"
    exit 0
  '
  stub_cmd systemctl '
    echo "SYSTEMCTL $*" >> "$STUB_LOG"
    case "$1" in
      is-active)       [ "${STUB_SYSTEMCTL_ACTIVE:-0}" = "1" ] && exit 0 || exit 3 ;;
      --user)
        case "$2" in
          is-active)        [ "${STUB_SYSTEMCTL_ACTIVE:-0}" = "1" ] && exit 0 || exit 3 ;;
          list-unit-files)  [ "${STUB_SYSTEMCTL_UNIT_EXISTS:-0}" = "1" ] && exit 0 || exit 1 ;;
          *)                exit 0 ;;
        esac
        ;;
      *) exit 0 ;;
    esac
  '
}

teardown_sandbox() {
  rm -rf "$TEST_HOME" "$STUB_BIN"
}

# stub_cmd NAME 'script body' - writes an executable fake NAME into the
# sandbox's PATH, shadowing the real command for the rest of the test.
stub_cmd() {
  local name="$1" body="$2"
  {
    printf '#!/usr/bin/env bash\n'
    printf '%s\n' "$body"
  } > "$STUB_BIN/$name"
  chmod +x "$STUB_BIN/$name"
}
