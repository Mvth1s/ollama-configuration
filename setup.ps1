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
    tier selection and the model tables are identical to 03-pull-models.sh, by
    hand-kept parity, so Linux and Windows never drift on model choice. Open
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

.EXAMPLE
    .\setup.ps1

.EXAMPLE
    .\setup.ps1 -Tier M -SkipWebui
#>
[CmdletBinding()]
param(
    [ValidateSet('XS', 'S', 'M', 'L')]
    [string]$Tier,

    [switch]$SkipModels,

    [switch]$SkipWebui,

    [switch]$DetectOnly
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
        pipx upgrade open-webui 2>$null
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

    # No CAND_<TIER>_<usage> equivalent exists on Windows (no interactive
    # model picker here, by design - see Get-ModelTier/Install-Model above),
    # so only the resolved per-usage defaults are reported, unlike the
    # Linux --detect-only path which also reports alternative candidates.
    # snake_case keys, matching the JSON emitted by 02-configure-gpu.sh /
    # 03-pull-models.sh --detect-only, so the Rust side parses both
    # platforms' output with the same field names.
    $result = [ordered]@{
        distro_pretty = 'Windows'
        gpu_vendor    = $Global:GpuVendor
        gpu_name      = $Global:GpuName
        cpu_model     = $Global:CpuModel
        cpu_threads   = $Global:CpuThreads
        ram_gb        = $Global:RamGb
        tier          = $Global:Tier
        tier_models   = $ModelTiers[$Global:Tier]
    }
    Write-Output ('__DETECT__' + ($result | ConvertTo-Json -Compress -Depth 4))
    exit 0
}

Get-RamGb | Out-Null
Install-OllamaWindows
Set-GpuConfig

if (-not $SkipModels) {
    Get-ModelTier
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
