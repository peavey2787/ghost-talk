function Test-GhostWindowsSymlinkSupport {
    $probeRoot = Join-Path $env:TEMP "ghost-talk-symlink-probe-$PID"
    $target = Join-Path $probeRoot 'target.txt'
    $link = Join-Path $probeRoot 'link.txt'
    try {
        Remove-Item -Recurse -Force $probeRoot -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $probeRoot | Out-Null
        Set-Content -Path $target -Value 'ghost-talk' -Encoding ascii
        New-Item -ItemType SymbolicLink -Path $link -Target $target -ErrorAction Stop | Out-Null
        return (Test-Path $link)
    } catch {
        return $false
    } finally {
        Remove-Item -Recurse -Force $probeRoot -ErrorAction SilentlyContinue
    }
}

function Test-GhostWindowsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Quote-GhostProcessArgument([string]$Value) {
    return '"' + $Value.Replace('"', '\"') + '"'
}

function Get-GhostAndroidLogPath([string]$ScriptPath) {
    $operation = [System.IO.Path]::GetFileNameWithoutExtension($ScriptPath)
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $root = Join-Path (Get-GhostRepoRoot) 'target\logs\android'
    New-Item -ItemType Directory -Force -Path $root | Out-Null
    return (Join-Path $root "android-$operation-$stamp.log")
}

function Show-GhostAndroidFailureTail([string]$LogPath) {
    if (-not (Test-Path $LogPath)) {
        Write-Host "Android diagnostic log was not created: $LogPath" -ForegroundColor Yellow
        return
    }
    Write-Host ''
    Write-Host "===== Android failure log tail: $LogPath =====" -ForegroundColor Yellow
    Get-Content -Path $LogPath -Tail 160 | Out-Host
    Write-Host '===== End Android failure log tail =====' -ForegroundColor Yellow
}

function Invoke-GhostVisibleElevatedAndroidEntry([string]$ScriptPath, [string]$BuildProfile = '') {
    $workingDirectory = (Get-Location).Path
    $powershell = Join-Path $PSHOME 'powershell.exe'
    $wrapper = Join-Path $PSScriptRoot 'elevated_entry.ps1'
    $logPath = Get-GhostAndroidLogPath $ScriptPath
    Write-Host 'Windows is denying unprivileged symbolic-link creation.'
    Write-Host 'Opening one Administrator PowerShell window for the Android operation...'
    Write-Host 'The Android build output will be shown live there and saved to:'
    Write-Host "  $logPath"
    $wrapperArguments = @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Quote-GhostProcessArgument $wrapper),
        '-EntryScript', (Quote-GhostProcessArgument $ScriptPath),
        '-LogPath', (Quote-GhostProcessArgument $logPath)
    )
    if ($BuildProfile) {
        $wrapperArguments += @('-BuildProfile', (Quote-GhostProcessArgument $BuildProfile))
    }
    try {
        $process = Start-Process -FilePath $powershell -Verb RunAs -PassThru -Wait -WindowStyle Normal `
            -WorkingDirectory $workingDirectory `
            -ArgumentList $wrapperArguments
    } catch [System.ComponentModel.Win32Exception] {
        if ($_.Exception.NativeErrorCode -eq 1223) {
            throw 'Ghost Talk Android could not continue because administrator approval was cancelled.'
        }
        throw
    }
    if ($process.ExitCode -ne 0) {
        Show-GhostAndroidFailureTail $logPath
        throw "Elevated Ghost Talk Android operation failed with exit code $($process.ExitCode). Full log: $logPath"
    }
    if ($BuildProfile) { Assert-GhostAndroidArtifacts $BuildProfile }
    Write-Host "Elevated Ghost Talk Android operation completed successfully. Log: $logPath"
    return $process.ExitCode
}

function Ensure-GhostWindowsSymlinkSupport([string]$EntryScript, [string]$BuildProfile = '') {
    if (Test-GhostWindowsSymlinkSupport) {
        Write-Host 'Windows symbolic-link support: ready'
        return
    }
    if (Test-GhostWindowsAdministrator) {
        throw @'
Windows is refusing symbolic-link creation even from an Administrator process.
Tauri Android requires this capability for jniLibs. Confirm the repository is on an NTFS volume and that Windows security/group policy is not denying symbolic links.
'@
    }
    $exitCode = Invoke-GhostVisibleElevatedAndroidEntry $EntryScript $BuildProfile
    exit $exitCode
}
