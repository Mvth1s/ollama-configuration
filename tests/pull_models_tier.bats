#!/usr/bin/env bats
# Tests for 03-pull-models.sh's tier selection: RAM-based auto-selection,
# --tier= override, and the CPU-only downgrade-to-S rule. Runs the real
# script with `free` and `ollama` stubbed out, so no model is ever actually
# downloaded.

load 'test_helper'

setup() {
  setup_sandbox
  stub_cmd ollama 'exit 0'
}

teardown() {
  teardown_sandbox
}

stub_free_ram() {
  local gb="$1"
  stub_cmd free "
    if [ \"\$1\" = \"-g\" ]; then printf '              total\nMem:      ${gb}\n'; fi
  "
}

seed_gpu_vendor() {
  mkdir -p "$TEST_HOME/.config/ollama-stack"
  printf 'GPU_VENDOR=%s\n' "\"$1\"" > "$TEST_HOME/.config/ollama-stack/state.env"
}

@test "--tier= forces the tier regardless of RAM" {
  stub_free_ram 64
  run "$REPO_ROOT/03-pull-models.sh" --tier=M --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"Tier manually forced: M"* ]]
  grep -q '^TIER=M$' "$TEST_HOME/.config/ollama-stack/state.env"
}

@test "auto-selects XS for <= 8GB RAM" {
  stub_free_ram 8
  run "$REPO_ROOT/03-pull-models.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"Selected model tier: XS"* ]]
}

@test "auto-selects S for <= 16GB RAM" {
  stub_free_ram 16
  run "$REPO_ROOT/03-pull-models.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"Selected model tier: S"* ]]
}

@test "no dedicated GPU downgrades a would-be L tier to S" {
  stub_free_ram 64
  seed_gpu_vendor none
  run "$REPO_ROOT/03-pull-models.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"reduced to S"* ]]
  [[ "$output" == *"Selected model tier: S"* ]]
}

@test "no dedicated GPU downgrades a would-be M tier to S" {
  stub_free_ram 32
  seed_gpu_vendor none
  run "$REPO_ROOT/03-pull-models.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"reduced to S"* ]]
}

@test "a dedicated GPU keeps the RAM-based L tier" {
  stub_free_ram 64
  seed_gpu_vendor nvidia
  run "$REPO_ROOT/03-pull-models.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" != *"reduced to S"* ]]
  [[ "$output" == *"Selected model tier: L"* ]]
}

@test "a forced tier is not affected by the CPU-only downgrade rule" {
  stub_free_ram 64
  seed_gpu_vendor none
  run "$REPO_ROOT/03-pull-models.sh" --tier=L --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" != *"reduced to S"* ]]
  [[ "$output" == *"Tier manually forced: L"* ]]
}

@test "--detect-only prints tier/models/candidates as JSON and never pulls a model" {
  stub_free_ram 8
  stub_cmd ollama 'echo "OLLAMA $*" >> "$STUB_LOG"; exit 0'
  run "$REPO_ROOT/03-pull-models.sh" --detect-only --tier=XS --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *'__DETECT__{"ram_gb":8,"tier":"XS"'* ]]
  [[ "$output" == *'"tier_models":{"texte":"llama3.2:3b"'* ]]
  [[ "$output" == *'"candidates":{"texte":['* ]]
  [ ! -f "$STUB_LOG" ] || ! grep -q '^OLLAMA' "$STUB_LOG"
}

@test "--model-<usage>= overrides the resolved model for that usage only" {
  stub_free_ram 8
  run "$REPO_ROOT/03-pull-models.sh" --detect-only --tier=XS --no-tui --model-code=starcoder2:3b
  [ "$status" -eq 0 ]
  [[ "$output" == *'"code":"starcoder2:3b"'* ]]
  [[ "$output" == *'"texte":"llama3.2:3b"'* ]]
}
