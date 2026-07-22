#!/usr/bin/env bats
# Unit tests for lib/common.sh: distro detection, RAM detection, the
# state.env read/write round trip, and the --no-tui backend selection.

load 'test_helper'

setup() {
  setup_sandbox
}

teardown() {
  teardown_sandbox
}

write_os_release() {
  TEST_OS_RELEASE="$TEST_HOME/os-release"
  printf '%s\n' "$@" > "$TEST_OS_RELEASE"
  export OS_RELEASE_FILE="$TEST_OS_RELEASE"
}

@test "detect_distro: arch via ID" {
  write_os_release 'ID=arch'
  run bash -c "source '$REPO_ROOT/lib/common.sh'; detect_distro >/dev/null; echo \"\$DISTRO_FAMILY\""
  [ "$status" -eq 0 ]
  [ "$output" = "arch" ]
}

@test "detect_distro: debian family via ID=ubuntu" {
  write_os_release 'ID=ubuntu' 'ID_LIKE=debian'
  run bash -c "source '$REPO_ROOT/lib/common.sh'; detect_distro >/dev/null; echo \"\$DISTRO_FAMILY\""
  [ "$status" -eq 0 ]
  [ "$output" = "debian" ]
}

@test "detect_distro: fedora family via ID_LIKE=rhel fedora" {
  write_os_release 'ID=nobara' 'ID_LIKE="fedora"'
  run bash -c "source '$REPO_ROOT/lib/common.sh'; detect_distro >/dev/null; echo \"\$DISTRO_FAMILY\""
  [ "$status" -eq 0 ]
  [ "$output" = "fedora" ]
}

@test "detect_distro: opensuse family via ID_LIKE=suse" {
  write_os_release 'ID=opensuse-tumbleweed' 'ID_LIKE="suse opensuse"'
  run bash -c "source '$REPO_ROOT/lib/common.sh'; detect_distro >/dev/null; echo \"\$DISTRO_FAMILY\""
  [ "$status" -eq 0 ]
  [ "$output" = "opensuse" ]
}

@test "detect_distro: unknown when os-release is missing" {
  export OS_RELEASE_FILE="$TEST_HOME/does-not-exist"
  run bash -c "source '$REPO_ROOT/lib/common.sh'; detect_distro >/dev/null; echo \"\$DISTRO_FAMILY\""
  [ "$status" -eq 0 ]
  [ "$output" = "unknown" ]
}

@test "detect_distro: does not re-detect once DISTRO_FAMILY is already set" {
  write_os_release 'ID=arch'
  run bash -c "source '$REPO_ROOT/lib/common.sh'; DISTRO_FAMILY=debian; detect_distro >/dev/null; echo \"\$DISTRO_FAMILY\""
  [ "$status" -eq 0 ]
  [ "$output" = "debian" ]
}

@test "detect_ram: reads GB straight from 'free -g'" {
  stub_cmd free '
    if [ "$1" = "-g" ]; then printf "              total\nMem:      16\n"; fi
  '
  run bash -c "source '$REPO_ROOT/lib/common.sh'; detect_ram >/dev/null; echo \"\$RAM_GB\""
  [ "$status" -eq 0 ]
  [ "$output" = "16" ]
}

@test "detect_ram: falls back to MB rounding when 'free -g' reports 0" {
  stub_cmd free '
    if [ "$1" = "-g" ]; then printf "              total\nMem:      0\n"
    elif [ "$1" = "-m" ]; then printf "              total\nMem:      512\n"; fi
  '
  run bash -c "source '$REPO_ROOT/lib/common.sh'; detect_ram >/dev/null; echo \"\$RAM_GB\""
  [ "$status" -eq 0 ]
  [ "$output" = "1" ]
}

@test "save_state / load_state: round-trips values, including ones with spaces" {
  run bash -c "
    source '$REPO_ROOT/lib/common.sh'
    FOO='bar baz'
    NUM=42
    save_state FOO NUM
  "
  [ "$status" -eq 0 ]

  run bash -c "source '$REPO_ROOT/lib/common.sh'; load_state; echo \"\$FOO|\$NUM\""
  [ "$status" -eq 0 ]
  [ "$output" = "bar baz|42" ]
}

@test "save_state: re-saving a variable overwrites the old value, not appends" {
  run bash -c "
    source '$REPO_ROOT/lib/common.sh'
    TIER=S; save_state TIER
    TIER=L; save_state TIER
    load_state
    echo \"\$TIER\"
    [ \"\$(grep -c '^TIER=' '$TEST_HOME/.config/ollama-stack/state.env')\" -eq 1 ]
  "
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "L" ]
}

@test "detect_tui: NO_TUI=1 forces backend 'none' even if a TUI tool exists" {
  stub_cmd dialog 'exit 0'
  run bash -c "source '$REPO_ROOT/lib/common.sh'; NO_TUI=1; detect_tui; echo \"\$TUI_BACKEND\"; tui_available && echo available || echo unavailable"
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "none" ]
  [ "${lines[1]}" = "unavailable" ]
}
