<#
.SYNOPSIS
    Installs and configures a local Ollama stack on Windows (Ollama + Open WebUI).

.DESCRIPTION
    Windows counterpart to setup.sh. Not a literal port of the Bash scripts:
    adapted to Windows primitives instead. GPU configuration is intentionally
    minimal, unlike 02-configure-gpu.sh: the official Ollama Windows installer
    already detects CUDA and ROCm natively and needs no per-vendor drop-in, so
    this script only detects the GPU vendor to log it and warn about AMD chips
    that may fall outside ROCm's officially supported list on Windows. Model
    tier selection, the model tables, and the interactive-picker candidate
    lists are identical to 03-pull-models.sh, by hand-kept parity, so Linux
    and Windows never drift on model choice or on which alternatives a GUI
    can offer. There is still no TUI here (Windows has none, by design) -
    the candidate lists exist solely for -DetectOnly to report, so a caller
    like the Tauri GUI's wizard can build its own picker. Open
    WebUI runs via a per-user scheduled task (AtLogOn) instead of a systemd
    user service, since systemd does not exist on Windows. It listens on
    127.0.0.1 only by default; run .\toggle-webui-lan.ps1 on|off|status at
    any time afterwards to allow/restrict access from other devices on the
    network.

    This script does not reuse or wrap the Bash scripts, including under WSL:
    it is a separate, native Windows implementation.

.PARAMETER Tier
    Force a specific model tier (XS, S, M, L) instead of auto-detecting from RAM.

.PARAMETER SkipModels
    Install Ollama and Open WebUI without downloading models.

.PARAMETER SkipWebui
    Skip Open WebUI installation.

.PARAMETER DetectOnly
    Print detected GPU/CPU/RAM/tier information as JSON and exit, without
    installing anything. Used by the Tauri GUI's detection screen; the
    underlying Get-RamGb/Get-GpuVendor/Get-CpuInfo/Get-ModelTier calls are
    all read-only, same as the -DetectOnly path in 02-configure-gpu.sh and
    03-pull-models.sh on the Linux side.

.PARAMETER ModelTexte
.PARAMETER ModelCode
.PARAMETER ModelReflexion
.PARAMETER ModelEmbeddings
    Override the resolved tier's default model for that usage, same
    non-interactive mechanism as --model-<usage>= in 03-pull-models.sh. Kept
    for a caller (script or human) that already knows which model tag it
    wants without going through -DetectOnly's reported candidates first.

.EXAMPLE
    .\setup.ps1

.EXAMPLE
    .\setup.ps1 -Tier M -SkipWebui

.EXAMPLE
    .\setup.ps1 -ModelCode 'qwen2.5-coder:14b'
#>
[CmdletBinding()]
param(
    [ValidateSet('XS', 'S', 'M', 'L')]
    [string]$Tier,

    [switch]$SkipModels,

    [switch]$SkipWebui,

    [switch]$DetectOnly,

    [string]$ModelTexte,

    [string]$ModelCode,

    [string]$ModelReflexion,

    [string]$ModelEmbeddings
)

$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'lib\common.ps1')

# ---------------------------------------------------------------------------
# Model tiers: same tags as MODEL_XS/S/M/L in 03-pull-models.sh. Keep both
# tables in sync by hand when changing a model.
# ---------------------------------------------------------------------------
$ModelTiers = @{
    XS = @{ texte = 'llama3.2:3b';  code = 'qwen2.5-coder:3b';  reflexion = 'deepseek-r1:1.5b'; embeddings = 'nomic-embed-text' }
    S  = @{ texte = 'llama3.1:8b';  code = 'qwen2.5-coder:7b';  reflexion = 'deepseek-r1:7b';   embeddings = 'nomic-embed-text' }
    M  = @{ texte = 'gemma3:12b';   code = 'devstral:24b';      reflexion = 'deepseek-r1:14b';  embeddings = 'nomic-embed-text' }
    L  = @{ texte = 'gemma3:27b';   code = 'qwen2.5-coder:32b'; reflexion = 'deepseek-r1:32b';  embeddings = 'nomic-embed-text' }
}

# ---------------------------------------------------------------------------
# Interactive-picker candidates, one array per tier/usage, format
# @{model=...; desc=...}. Windows counterpart to the CAND_<TIER>_<usage>
# arrays in 03-pull-models.sh - kept in sync by hand with that file, same as
# $ModelTiers above. The first entry for each usage always matches that
# usage's $ModelTiers default, same invariant the Bash arrays keep. There is
# still no TUI on Windows to render these directly (see Get-ModelTier) - they
# exist purely for -DetectOnly to report as the "candidates" field, so the
# Tauri GUI wizard's existing "Changer" dropdown (already generic over any
# platform's reported candidates) can offer them the same way it already
# does on Linux.
# ---------------------------------------------------------------------------
$ModelCandidates = @{
    XS = @{
        texte      = @(
            @{ model = 'llama3.2:3b'; desc = 'Fast, solid generalist for low-end hardware' }
            @{ model = 'qwen2.5:3b'; desc = 'Multilingual alternative' }
            @{ model = 'phi3.5:3.8b'; desc = 'Compact, decent basic reasoning' }
        )
        code       = @(
            @{ model = 'qwen2.5-coder:3b'; desc = 'Lightweight general-purpose coding model' }
            @{ model = 'starcoder2:3b'; desc = 'Alternative geared toward completion' }
        )
        reflexion  = @(
            @{ model = 'deepseek-r1:1.5b'; desc = 'Step-by-step reasoning, very lightweight' }
            @{ model = 'qwen2.5:1.5b'; desc = 'Lightweight generalist alternative' }
        )
        embeddings = @(
            @{ model = 'nomic-embed-text'; desc = 'Standard general-purpose embeddings' }
            @{ model = 'all-minilm'; desc = 'Lighter, faster' }
        )
    }
    S = @{
        texte      = @(
            @{ model = 'llama3.1:8b'; desc = 'Well-balanced generalist' }
            @{ model = 'gemma2:9b'; desc = 'Google alternative, good instruction following' }
            @{ model = 'mistral:7b'; desc = 'Fast, good tradeoff' }
        )
        code       = @(
            @{ model = 'qwen2.5-coder:7b'; desc = 'General-purpose coding model' }
            @{ model = 'codellama:7b'; desc = 'Meta alternative, geared toward completion' }
        )
        reflexion  = @(
            @{ model = 'deepseek-r1:7b'; desc = 'Step-by-step reasoning' }
            @{ model = 'qwen2.5:7b'; desc = 'Generalist alternative' }
        )
        embeddings = @(
            @{ model = 'nomic-embed-text'; desc = 'Standard general-purpose embeddings' }
            @{ model = 'all-minilm'; desc = 'Lighter, faster' }
        )
    }
    M = @{
        texte      = @(
            @{ model = 'gemma3:12b'; desc = 'Recent Google generalist' }
            @{ model = 'mistral-nemo:12b'; desc = 'Mistral/Nvidia alternative' }
            @{ model = 'qwen2.5:14b'; desc = 'Bigger, better general reasoning' }
        )
        code       = @(
            @{ model = 'devstral:24b'; desc = 'Geared toward coding agents' }
            @{ model = 'qwen2.5-coder:14b'; desc = 'Lighter alternative' }
        )
        reflexion  = @(
            @{ model = 'deepseek-r1:14b'; desc = 'Step-by-step reasoning' }
            @{ model = 'qwen2.5:14b'; desc = 'Generalist alternative' }
        )
        embeddings = @(
            @{ model = 'nomic-embed-text'; desc = 'Standard general-purpose embeddings' }
            @{ model = 'mxbai-embed-large'; desc = 'More accurate, heavier' }
        )
    }
    L = @{
        texte      = @(
            @{ model = 'gemma3:27b'; desc = 'Large Google generalist' }
            @{ model = 'qwen2.5:32b'; desc = 'Alibaba alternative' }
            @{ model = 'mixtral:8x7b'; desc = 'Mixture-of-experts, good speed/quality tradeoff' }
        )
        code       = @(
            @{ model = 'qwen2.5-coder:32b'; desc = 'Large general-purpose coding model' }
            @{ model = 'devstral:24b'; desc = 'Alternative geared toward coding agents' }
        )
        reflexion  = @(
            @{ model = 'deepseek-r1:32b'; desc = 'Large step-by-step reasoning model' }
            @{ model = 'qwq:32b'; desc = 'Alibaba alternative geared toward reasoning' }
        )
        embeddings = @(
            @{ model = 'nomic-embed-text'; desc = 'Standard general-purpose embeddings' }
            @{ model = 'mxbai-embed-large'; desc = 'More accurate, heavier' }
        )
    }
}

$AmdRocmHint = "Verify your AMD GPU is on ROCm's officially supported list for Windows (https://rocm.docs.amd.com/); the Ollama installer falls back on its own if it isn't."

function Install-OllamaWindows {
    if (Get-Command ollama -ErrorAction SilentlyContinue) {
        $version = (ollama --version 2>$null | Select-Object -First 1)
        Log-Ok "Ollama already installed ($version)."
        return
    }

    Log-Info 'Installing Ollama...'
    if (Get-Command winget -ErrorAction SilentlyContinue) {
        winget install --id Ollama.Ollama -e --silent --accept-package-agreements --accept-source-agreements
    } else {
        $installer = Join-Path $env:TEMP 'OllamaSetup.exe'
        Log-Info 'winget not found, downloading the official installer...'
        Invoke-WebRequest -Uri 'https://ollama.com/download/OllamaSetup.exe' -OutFile $installer
        Log-Warn 'Launching the Ollama installer: follow the wizard, then re-run this script.'
        Start-Process -FilePath $installer -Wait
    }

    if (-not (Get-Command ollama -ErrorAction SilentlyContinue)) {
        Log-Err 'Ollama was not found on PATH after installation. Open a new terminal and re-run this script.'
        exit 1
    }
}

function Set-GpuConfig {
    $vendor = Get-GpuVendor
    switch ($vendor) {
        'nvidia' { Log-Ok "Nvidia GPU detected ($Global:GpuName): CUDA is handled natively by the Ollama installer." }
        'amd'    { Log-Warn "AMD GPU detected ($Global:GpuName). $AmdRocmHint" }
        'intel'  { Log-Warn "Intel GPU detected ($Global:GpuName): no dedicated Windows backend yet, Ollama will use the CPU." }
        default  { Log-Info 'No dedicated GPU detected, Ollama will run on CPU.' }
    }
}

function Get-ModelTier {
    if ($Tier) {
        Log-Info "Tier manually forced: $Tier"
        $Global:Tier = $Tier
    } else {
        $ram = Get-RamGb
        if ($ram -le 8) {
            $Global:Tier = 'XS'
        } elseif ($ram -le 16) {
            $Global:Tier = 'S'
        } elseif ($ram -le 32) {
            $Global:Tier = 'M'
        } else {
            $Global:Tier = 'L'
        }

        # CPU only: a 12b+ model becomes too slow in practice, so we drop
        # down to S regardless of raw RAM tier. Same rule as 03-pull-models.sh.
        if ($Global:GpuVendor -eq 'none' -and ($Global:Tier -eq 'M' -or $Global:Tier -eq 'L')) {
            Log-Warn "No dedicated GPU: tier $($Global:Tier) reduced to S to remain usable in practice."
            $Global:Tier = 'S'
        }
    }

    Log-Info "Selected model tier: $($Global:Tier)"
    Save-State -VarNames @('Tier')
}

# ---------------------------------------------------------------------------
# Non-interactive per-usage overrides: same mutation 03-pull-models.sh's
# --model-<usage>= flags perform on $tier_models, just sourced from
# PowerShell parameters instead of a dialog/whiptail menu (there is no
# TUI/candidate picker on Windows - see Get-ModelTier above and the
# .PARAMETER doc). Called after Get-ModelTier has resolved $Global:Tier, both
# from the -DetectOnly branch and the real run, so an override is reflected
# in detection output too, exactly like the Linux script.
# ---------------------------------------------------------------------------
function Set-ModelOverride {
    if ($ModelTexte) { $ModelTiers[$Global:Tier]['texte'] = $ModelTexte }
    if ($ModelCode) { $ModelTiers[$Global:Tier]['code'] = $ModelCode }
    if ($ModelReflexion) { $ModelTiers[$Global:Tier]['reflexion'] = $ModelReflexion }
    if ($ModelEmbeddings) { $ModelTiers[$Global:Tier]['embeddings'] = $ModelEmbeddings }
}

function Install-Model {
    $models = $ModelTiers[$Global:Tier]
    foreach ($usage in @('texte', 'code', 'reflexion', 'embeddings')) {
        $model = $models[$usage]
        Log-Info "Downloading $usage model: $model"
        ollama pull $model
        if ($LASTEXITCODE -ne 0) {
            Log-Err "Failed to download $usage model: $model (exit code $LASTEXITCODE)"
            exit 1
        }
    }
    Log-Ok "All models for tier $($Global:Tier) are ready (run 'ollama list' to verify)."
}

function Install-OpenWebUI {
    Log-Info 'Installing Open WebUI...'

    if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
        if (Get-Command winget -ErrorAction SilentlyContinue) {
            Log-Info 'Python not found, installing via winget...'
            winget install --id Python.Python.3.12 -e --silent --accept-package-agreements --accept-source-agreements
        } else {
            Log-Err 'Python not found and winget is unavailable. Install Python manually, then re-run this script.'
            exit 1
        }
    }

    if (-not (Get-Command pipx -ErrorAction SilentlyContinue)) {
        python -m pip install --user pipx
        python -m pipx ensurepath
    }

    $webuiBin = $null
    if (Get-Command pipx -ErrorAction SilentlyContinue) {
        pipx install open-webui 2>$null
        if ($LASTEXITCODE -ne 0) { pipx upgrade open-webui 2>$null }
        $pipxBinDir = (pipx environment --value PIPX_BIN_DIR).Trim()
        $candidate = Join-Path $pipxBinDir 'open-webui.exe'
        if (Test-Path $candidate) { $webuiBin = $candidate }
    }

    if (-not $webuiBin) {
        python -m pip install --upgrade open-webui
        $cmd = Get-Command open-webui -ErrorAction SilentlyContinue
        if ($cmd) { $webuiBin = $cmd.Source }
    }

    if (-not $webuiBin) {
        Log-Err 'Could not locate the open-webui executable after installation. Open a new terminal (PATH refresh) and re-run this script.'
        exit 1
    }

    # Scheduled tasks inherit the persisted user environment at logon, so we
    # set these the same way the systemd drop-in sets Environment= on Linux.
    [Environment]::SetEnvironmentVariable('OLLAMA_BASE_URL', 'http://127.0.0.1:11434', 'User')
    [Environment]::SetEnvironmentVariable('WEBUI_AUTH', 'False', 'User')

    # WebuiHost is persisted state (like Tier/GpuVendor), not just an install
    # default: re-running this script must not silently undo a LAN-access
    # choice made afterwards with toggle-webui-lan.ps1.
    if (-not $Global:WebuiHost) {
        $Global:WebuiHost = '127.0.0.1'
    }
    Save-State -VarNames @('WebuiHost')

    $action = New-ScheduledTaskAction -Execute $webuiBin -Argument "serve --port 8080 --host $($Global:WebuiHost)"
    $trigger = New-ScheduledTaskTrigger -AtLogOn
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable

    Register-ScheduledTask -TaskName 'OpenWebUI' -Action $action -Trigger $trigger -Settings $settings `
        -Description 'Open WebUI (local web interface for Ollama)' -Force | Out-Null
    Stop-ScheduledTask -TaskName 'OpenWebUI' -ErrorAction SilentlyContinue
    Start-ScheduledTask -TaskName 'OpenWebUI'

    Log-Ok 'Open WebUI started. Available at http://localhost:8080'
    Log-Info 'It will restart automatically at each logon (scheduled task "OpenWebUI").'

    if ($Global:WebuiHost -eq '0.0.0.0') {
        Log-Warn 'LAN access is enabled: Open WebUI is reachable from other devices on your network.'
        Log-Warn 'No login is required by default (WEBUI_AUTH=False): anyone on your network can use it.'
        Log-Warn 'Restrict to this machine only: .\toggle-webui-lan.ps1 off'
        Log-Warn "Require a login instead: [Environment]::SetEnvironmentVariable('WEBUI_AUTH','True','User'), then restart the 'OpenWebUI' task."
    } else {
        Log-Info 'Open WebUI is restricted to this machine only. Run .\toggle-webui-lan.ps1 on to allow access from other devices (e.g. a phone).'
    }
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
Load-State

if ($DetectOnly) {
    Get-RamGb | Out-Null
    Get-GpuVendor | Out-Null
    Get-CpuInfo | Out-Null
    Get-ModelTier
    Set-ModelOverride

    # snake_case keys, matching the JSON emitted by 02-configure-gpu.sh /
    # 03-pull-models.sh --detect-only, so the Rust side parses both
    # platforms' output with the same field names. "candidates" now mirrors
    # 03-pull-models.sh's own --detect-only output (see $ModelCandidates
    # above) rather than being omitted - the Rust DetectResultRaw type
    # already defaults it to an empty map via #[serde(default)] when a
    # platform's JSON has no such field, so this was always optional from
    # the Rust side; it just had nothing to send before.
    $result = [ordered]@{
        distro_pretty = 'Windows'
        gpu_vendor    = $Global:GpuVendor
        gpu_name      = $Global:GpuName
        cpu_model     = $Global:CpuModel
        cpu_threads   = $Global:CpuThreads
        ram_gb        = $Global:RamGb
        tier          = $Global:Tier
        tier_models   = $ModelTiers[$Global:Tier]
        candidates    = $ModelCandidates[$Global:Tier]
    }
    # -Depth 8: generous margin over the actual nesting
    # (result -> candidates -> usage -> array -> {model,desc}, 4 real
    # levels) since ConvertTo-Json silently truncates anything past -Depth
    # rather than erroring, and there's no real cost to headroom here - this
    # can't be tested locally in this session (no pwsh available), so
    # erring toward "too deep" over "silently truncated JSON" is deliberate.
    Write-Output ('__DETECT__' + ($result | ConvertTo-Json -Compress -Depth 8))
    exit 0
}

Get-RamGb | Out-Null
Install-OllamaWindows
Set-GpuConfig

if (-not $SkipModels) {
    Get-ModelTier
    Set-ModelOverride
    Install-Model
} else {
    Log-Info 'Model download skipped (-SkipModels).'
}

if (-not $SkipWebui) {
    Install-OpenWebUI
} else {
    Log-Info 'Open WebUI installation skipped (-SkipWebui).'
}

Load-State
Write-Host ''
Write-Host '============================================================'
Write-Host ' Installation summary'
Write-Host '============================================================'
$gpuDisplay = if ($Global:GpuName) { $Global:GpuName } else { 'none' }
Write-Host " GPU          : $gpuDisplay ($Global:GpuVendor)"
Write-Host " RAM          : $($Global:RamGb) GB"
if (-not $SkipModels) { Write-Host " Tier         : $($Global:Tier)" }
if (-not $SkipWebui)  {
    $lanState = if ($Global:WebuiHost -eq '0.0.0.0') { 'LAN access: ON' } else { 'LAN access: OFF' }
    Write-Host " Web UI       : http://localhost:8080 ($lanState)"
}
Write-Host '============================================================'
Write-Host ' Useful commands:'
Write-Host '   ollama list                          list installed models'
Write-Host '   Get-ScheduledTask OpenWebUI           Open WebUI task status'
Write-Host '   .\toggle-webui-lan.ps1 on|off|status  allow/restrict LAN access to Open WebUI'
Write-Host '============================================================'
