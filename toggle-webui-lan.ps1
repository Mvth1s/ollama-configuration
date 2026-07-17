<#
.SYNOPSIS
    Switches Open WebUI between "this machine only" and "reachable from the
    local network", at any time after install.

.DESCRIPTION
    Windows counterpart to toggle-webui-lan.sh. Rewrites the 'OpenWebUI'
    scheduled task's action with the new --host value and restarts it. Does
    not reinstall or touch Ollama/model state.

.PARAMETER Mode
    on      Allow Open WebUI to be reached from other devices on your network
            (e.g. a phone). No login is required by default (WEBUI_AUTH=False).
    off     Restrict Open WebUI to this machine only (127.0.0.1). Default.
    status  Show the current setting.

.EXAMPLE
    .\toggle-webui-lan.ps1 on

.EXAMPLE
    .\toggle-webui-lan.ps1 status
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)]
    [ValidateSet('on', 'off', 'status')]
    [string]$Mode
)

$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'lib\common.ps1')

Load-State

$TaskName = 'OpenWebUI'

if ($Mode -eq 'status') {
    $currentHost = if ($Global:WebuiHost) { $Global:WebuiHost } else { '127.0.0.1' }
    if ($currentHost -eq '0.0.0.0') {
        Log-Info 'LAN access: ON (reachable from your network)'
    } else {
        Log-Info "LAN access: OFF (this machine only, $currentHost)"
    }
    $authState = [Environment]::GetEnvironmentVariable('WEBUI_AUTH', 'User')
    Log-Info "Login required (WEBUI_AUTH): $(if ($authState) { $authState } else { 'unknown' })"
    exit 0
}

$newHost = if ($Mode -eq 'on') { '0.0.0.0' } else { '127.0.0.1' }
$Global:WebuiHost = $newHost
Save-State -VarNames @('WebuiHost')

if ($newHost -eq '0.0.0.0') {
    Log-Warn 'LAN access enabled: Open WebUI will be reachable from other devices on your network.'
    Log-Warn 'No login is required by default (WEBUI_AUTH=False): anyone on your network can use it.'
    Log-Warn "Require a login instead: [Environment]::SetEnvironmentVariable('WEBUI_AUTH','True','User'), then re-run this command or restart the '$TaskName' task."
} else {
    Log-Info 'LAN access disabled: Open WebUI is now restricted to this machine only.'
}

$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if (-not $task) {
    Log-Info "Open WebUI is not installed yet; this setting will be applied automatically the next time you run .\setup.ps1."
    exit 0
}

$webuiBin = ($task.Actions | Select-Object -First 1).Execute
$action = New-ScheduledTaskAction -Execute $webuiBin -Argument "serve --port 8080 --host $newHost"
Set-ScheduledTask -TaskName $TaskName -Action $action | Out-Null

Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
Start-ScheduledTask -TaskName $TaskName

Log-Ok 'Open WebUI restarted with the new setting.'
