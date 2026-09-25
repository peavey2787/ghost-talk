param(
    [Parameter(Mandatory = $true)][string]$EntryScript,
    [Parameter(Mandatory = $true)][string]$LogPath,
    [string]$BuildProfile = ''
)

$ErrorActionPreference = 'Stop'
$exitCode = 1
$failed = $false
$transcriptStarted = $false

if ($BuildProfile -and $BuildProfile -notin @('release', 'debug')) {
    throw "Unsupported Android build profile: $BuildProfile"
}

try {
    $logDirectory = Split-Path -Parent $LogPath
    New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
    Start-Transcript -Path $LogPath -Force | Out-Null
    $transcriptStarted = $true
    Write-Host "Ghost Talk elevated Android log: $LogPath"
    if ($BuildProfile -eq 'debug') {
        & $EntryScript -Debug
    } else {
        & $EntryScript
    }
    $exitCode = if ($LASTEXITCODE -is [int]) { $LASTEXITCODE } else { 0 }
} catch {
    $failed = $true
    $exitCode = 1
    Write-Host ''
    Write-Host 'Ghost Talk Android build failed:' -ForegroundColor Red
    $_ | Format-List * -Force | Out-Host
} finally {
    if ($transcriptStarted) {
        try { Stop-Transcript | Out-Null } catch { }
    }
}

if ($failed -or $exitCode -ne 0) {
    Write-Host ''
    Write-Host "Android build log saved to: $LogPath" -ForegroundColor Yellow
    [void](Read-Host 'Build failed. Press Enter to close this Administrator window')
}
exit $exitCode
