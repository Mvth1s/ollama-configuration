#!/usr/bin/env bats
# Tests for 02-configure-gpu.sh: GPU vendor detection from lspci output and
# the Nvidia > AMD > Intel priority for hybrid (laptop) configs. sudo and
# systemctl are stubbed by test_helper so nothing ever touches the real
# /etc/systemd or restarts a real ollama service, no matter what is actually
# installed on the machine running the suite.

load 'test_helper'

setup() {
  setup_sandbox
}

teardown() {
  teardown_sandbox
}

# stub_lspci VENDOR - fakes `lspci -nnk` output for one or more controllers.
# Uses the same [XXXX:...] PCI-vendor-ID bracket format 02-configure-gpu.sh
# actually greps for.
stub_lspci() {
  local body=""
  case "$1" in
    nvidia)
      body='echo "01:00.0 VGA compatible controller [0300]: NVIDIA Corporation GA104 [GeForce RTX 3070] [10de:2484] (rev a1)"'
      ;;
    amd)
      body='echo "03:00.0 VGA compatible controller [0300]: Advanced Micro Devices, Inc. [AMD/ATI] Navi 21 [Radeon RX 6800] [1002:73bf]"'
      ;;
    intel)
      body='echo "00:02.0 VGA compatible controller [0300]: Intel Corporation TigerLake-LP GT2 [Iris Xe Graphics] [8086:9a49]"'
      ;;
    hybrid-intel-nvidia)
      body='echo "00:02.0 VGA compatible controller [0300]: Intel Corporation TigerLake-LP GT2 [Iris Xe Graphics] [8086:9a49]"
echo "01:00.0 3D controller [0302]: NVIDIA Corporation GA107M [GeForce RTX 3050 Mobile] [10de:25a2] (rev a1)"'
      ;;
    none)
      body='true'
      ;;
  esac
  stub_cmd lspci "$body"
}

@test "no dedicated GPU: falls back to CPU, no driver install attempted" {
  stub_lspci none
  run "$REPO_ROOT/02-configure-gpu.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"No dedicated GPU, Ollama will run on CPU."* ]]
}

@test "Nvidia GPU with driver already present: no prompt, CUDA configured" {
  stub_lspci nvidia
  stub_cmd nvidia-smi '
    if [ "$1" = "--query-gpu=name" ]; then echo "NVIDIA GeForce RTX 3070"; fi
  '
  run "$REPO_ROOT/02-configure-gpu.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"GPU selected for acceleration:"*"(nvidia)"* ]]
  [[ "$output" == *"Configuring Nvidia GPU (CUDA)"* ]]
  [[ "$output" == *"Nvidia driver already present"* ]]
}

@test "Nvidia GPU without a driver: declining the install prompt does not install anything" {
  stub_lspci nvidia
  run bash -c "printf 'n\n' | '$REPO_ROOT/02-configure-gpu.sh' --no-tui"
  [ "$status" -eq 0 ]
  [[ "$output" == *"No Nvidia driver detected"* ]]
  [[ "$output" == *"GPU configuration complete."* ]]
}

@test "hybrid Intel+Nvidia laptop: Nvidia wins the priority" {
  stub_lspci hybrid-intel-nvidia
  stub_cmd nvidia-smi '
    if [ "$1" = "--query-gpu=name" ]; then echo "NVIDIA GeForce RTX 3050 Mobile"; fi
  '
  run "$REPO_ROOT/02-configure-gpu.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"GPU selected for acceleration:"*"(nvidia)"* ]]
}

@test "AMD GPU with an unsupported gfx code: falls back to Vulkan + HSA override" {
  stub_lspci amd
  stub_cmd rocminfo 'echo "  Name: gfx1201"'
  run "$REPO_ROOT/02-configure-gpu.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"gfx1201 is not yet officially supported by ROCm."* ]]
  [[ "$output" == *"Applying workaround: OLLAMA_VULKAN=1 + HSA_OVERRIDE_GFX_VERSION=11.5.0"* ]]
}

@test "AMD GPU with an officially supported gfx code: uses default HIP config" {
  stub_lspci amd
  stub_cmd rocminfo 'echo "  Name: gfx1030"'
  run "$REPO_ROOT/02-configure-gpu.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"gfx1030 is officially supported by ROCm, using default config (HIP)."* ]]
}

@test "AMD GPU with rocminfo unavailable: enables Vulkan without a specific workaround" {
  stub_lspci amd
  run "$REPO_ROOT/02-configure-gpu.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"GFX code not detected"* ]]
  [[ "$output" == *"Enabling Vulkan backend by default, without a specific workaround."* ]]
}

@test "Intel GPU: enables the best-effort Vulkan backend" {
  stub_lspci intel
  run "$REPO_ROOT/02-configure-gpu.sh" --no-tui
  [ "$status" -eq 0 ]
  [[ "$output" == *"Configuring Intel GPU (Vulkan, best effort)"* ]]
}
