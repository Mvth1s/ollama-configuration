# =============================================================================
# lib/common.ps1
# Dot-source from other scripts with: . (Join-Path $PSScriptRoot 'lib\common.ps1')
#
# Windows counterpart to lib/common.sh. Not a literal port: adapted to
# Windows/PowerShell primitives (CIM for hardware info, %APPDATA% instead of
# ~/.config, no distro/package-manager detection since there is only one
# Windows). Provides:
#   - logging helpers (Log-Info, Log-Ok, Log-Warn, Log-Err)
#   - a small shared state system between scripts (%APPDATA%\ollama-stack),
#     using the same "VAR="value"" line format as state.env on Linux, so
#     each script can run standalone or chained after another without
#     re-detecting everything every time
#   - RAM detection
#   - GPU vendor detection, keyed off the same PCI vendor IDs used by
#     02-configure-gpu.sh (10DE=Nvidia, 1002/1022=AMD, 8086=Intel) rather
#     than commercial card names, for consistency with the Linux scripts
# =============================================================================

$StateDir = Join-Path $env:APPDATA 'ollama-stack'
$StateFile = Join-Path $StateDir 'state.env'

function Log-Info([string]$Message) { Write-Host "[INFO] $Message" -ForegroundColor Cyan }
function Log-Ok([string]$Message)   { Write-Host "[OK] $Message" -ForegroundColor Green }
function Log-Warn([string]$Message) { Write-Host "[WARNING] $Message" -ForegroundColor Yellow }
function Log-Err([string]$Message)  { Write-Host "[ERROR] $Message" -ForegroundColor Red }

# ---------------------------------------------------------------------------
# Shared state between scripts (GpuVendor, GpuName, RamGb, Tier, ...).
# Variables are stored/restored as globals so callers can read them straight
# after Load-State without re-detecting, exactly like sourcing state.env in
# the Bash scripts.
# ---------------------------------------------------------------------------
function Load-State {
    if (-not (Test-Path $StateDir)) { New-Item -ItemType Directory -Path $StateDir -Force | Out-Null }
    if (-not (Test-Path $StateFile)) { return }

    Get-Content $StateFile | ForEach-Object {
        if ($_ -match '^(\w+)="(.*)"$') {
            Set-Variable -Scope Global -Name $Matches[1] -Value $Matches[2]
        }
    }
}

function Save-State {
    param([Parameter(Mandatory)][string[]]$VarNames)

    if (-not (Test-Path $StateDir)) { New-Item -ItemType Directory -Path $StateDir -Force | Out-Null }

    $existing = @()
    if (Test-Path $StateFile) {
        $pattern = '^(' + ($VarNames -join '|') + ')='
        $existing = @(Get-Content $StateFile | Where-Object { $_ -notmatch $pattern })
    }

    $updated = @()
    foreach ($name in $VarNames) {
        $value = Get-Variable -Scope Global -Name $name -ValueOnly -ErrorAction SilentlyContinue
        $updated += '{0}="{1}"' -f $name, $value
    }

    Set-Content -Path $StateFile -Value ($existing + $updated)
}

# ---------------------------------------------------------------------------
# RAM, rounded up to the next GB (same tiering input as detect_ram in
# lib/common.sh, computed directly from bytes rather than `free -g`).
# ---------------------------------------------------------------------------
function Get-RamGb {
    if ($Global:RamGb) { return $Global:RamGb }

    $bytes = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
    $Global:RamGb = [Math]::Ceiling($bytes / 1GB)
    Log-Info "Detected total RAM: $($Global:RamGb) GB"
    Save-State -VarNames @('RamGb')
    return $Global:RamGb
}

# ---------------------------------------------------------------------------
# GPU vendor: reads PNPDeviceID (format "PCI\VEN_xxxx&DEV_...") from every
# video controller CIM reports, matches the same PCI vendor IDs as
# 02-configure-gpu.sh, priority Nvidia > AMD > Intel for hybrid laptops
# (typical: Intel iGPU + Nvidia/AMD dGPU).
# ---------------------------------------------------------------------------
function Get-GpuVendor {
    if ($Global:GpuVendor) { return $Global:GpuVendor }

    $controllers = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
        Where-Object { $_.PNPDeviceID -match 'VEN_[0-9A-Fa-f]{4}' }

    $Global:GpuVendor = 'none'
    $Global:GpuName = ''

    $nvidia = $controllers | Where-Object { $_.PNPDeviceID -match 'VEN_10DE' } | Select-Object -First 1
    $amd    = $controllers | Where-Object { $_.PNPDeviceID -match 'VEN_(1002|1022)' } | Select-Object -First 1
    $intel  = $controllers | Where-Object { $_.PNPDeviceID -match 'VEN_8086' } | Select-Object -First 1

    if ($nvidia) {
        $Global:GpuVendor = 'nvidia'
        $Global:GpuName = $nvidia.Name
    } elseif ($amd) {
        $Global:GpuVendor = 'amd'
        $Global:GpuName = $amd.Name
    } elseif ($intel) {
        $Global:GpuVendor = 'intel'
        $Global:GpuName = $intel.Name
    }

    $displayName = if ($Global:GpuName) { $Global:GpuName } else { 'none' }
    Log-Info "GPU selected: $displayName ($($Global:GpuVendor))"
    Save-State -VarNames @('GpuVendor', 'GpuName')
    return $Global:GpuVendor
}

# ---------------------------------------------------------------------------
# CPU: model name + logical thread count, for display purposes only (no
# tier/config decision depends on this, unlike RAM/GPU). Windows counterpart
# to detect_cpu in lib/common.sh.
# ---------------------------------------------------------------------------
function Get-CpuInfo {
    if ($Global:CpuModel) { return $Global:CpuModel }

    $cpu = Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1
    $Global:CpuModel = if ($cpu) { $cpu.Name.Trim() } else { 'unknown' }
    $Global:CpuThreads = if ($cpu) { $cpu.NumberOfLogicalProcessors } else { 1 }

    Log-Info "Detected CPU: $($Global:CpuModel) ($($Global:CpuThreads) threads)"
    Save-State -VarNames @('CpuModel', 'CpuThreads')
    return $Global:CpuModel
}
