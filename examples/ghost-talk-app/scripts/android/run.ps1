. (Join-Path $PSScriptRoot 'common.ps1')
Ensure-GhostWindowsSymlinkSupport $PSCommandPath
Initialize-GhostAndroidEnvironment
$device = Get-GhostArmAndroidDevice
Push-Location (Get-GhostNativeRoot)
try { Invoke-GhostChecked 'cargo' @('tauri', 'android', 'dev', $device) } finally { Pop-Location }
