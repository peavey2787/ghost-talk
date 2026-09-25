param([switch]$Debug)

if ($env:GHOST_TALK_ANDROID_BUILD_PROFILE -eq 'debug') { $Debug = $true }
$env:GHOST_TALK_ANDROID_BUILD_PROFILE = if ($Debug) { 'debug' } else { 'release' }

. (Join-Path $PSScriptRoot 'common.ps1')
$profile = if ($Debug) { 'debug' } else { 'release' }
Ensure-GhostWindowsSymlinkSupport $PSCommandPath $profile
Initialize-GhostAndroidEnvironment
Remove-GhostAndroidPackageOutputs
$arguments = @('tauri', 'android', 'build', '--ci', '--apk', '--aab', '--target') + $GhostAndroidBuildTargets
if ($Debug) { $arguments += '--debug' }
try {
    Push-Location (Get-GhostNativeRoot)
    try { Invoke-GhostChecked 'cargo' $arguments } finally { Pop-Location }
    Stage-GhostBuildArtifacts 'android' $profile
    Assert-GhostAndroidArtifacts $profile
    Write-Host "Ghost Talk Android $profile build complete."
    Write-Host "Final files: $(Join-Path (Get-GhostRepoRoot) "target\android\$profile")"
    Write-Host "Compatibility mirror: $(Join-Path (Get-GhostRepoRoot) "target\dist\android\$profile")"
} finally {
    Remove-GhostAndroidTransientBuildState
}
