$ErrorActionPreference = 'Stop'
$GhostRustVersion = '1.98.0'
$GhostTrunkVersion = '0.21.14'
$GhostTauriVersion = '2.11.4'
$GhostAndroidApi = '36'
$GhostAndroidBuildTools = '36.0.0'
$GhostAndroidNdk = '30.0.16248370'
$GhostCmdlineTools = '15859902'
$GhostAndroidBuildTargets = @('aarch64', 'armv7')
$GhostCmdlineSha256 = '90ae805d20434428bffcb699c290860f19bb5f66a67e6b330067e3de801fb04a'

. (Join-Path $PSScriptRoot 'windows_host.ps1')

function Get-GhostAppRoot { return (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path }
function Get-GhostRepoRoot { return (Resolve-Path (Join-Path (Get-GhostAppRoot) '..\..')).Path }
function Get-GhostNativeRoot { return (Join-Path (Get-GhostAppRoot) 'crates\ghost-talk-native') }

function Invoke-GhostChecked([string]$File, [string[]]$Arguments) {
    & $File @Arguments | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "$File failed with exit code $LASTEXITCODE" }
}

function Invoke-GhostNativeCapture([string]$File, [string[]]$Arguments) {
    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = @(& $File @Arguments 2>&1 | ForEach-Object { $_.ToString() })
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($exitCode -ne 0) { return @() }
    return $output
}

function Add-GhostCargoPath {
    $cargoBin = Join-Path $HOME '.cargo\bin'
    if (($env:PATH -split ';') -notcontains $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }
}

function Ensure-GhostRustTools {
    Add-GhostCargoPath
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
        $installer = Join-Path $env:TEMP 'ghost-talk-rustup-init.exe'
        Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile $installer
        Invoke-GhostChecked $installer @('-y', '--profile', 'minimal')
        Add-GhostCargoPath
    }
    Invoke-GhostChecked 'rustup' @('toolchain', 'install', $GhostRustVersion, '--profile', 'minimal', '--component', 'rustfmt', '--component', 'clippy')
    Invoke-GhostChecked 'rustup' @('target', 'add', '--toolchain', $GhostRustVersion, 'wasm32-unknown-unknown')
    Invoke-GhostChecked 'rustup' @('target', 'add', '--toolchain', $GhostRustVersion, 'aarch64-linux-android', 'armv7-linux-androideabi')
    $env:RUSTUP_TOOLCHAIN = $GhostRustVersion

    $trunkVersion = if (Get-Command trunk -ErrorAction SilentlyContinue) { (((Invoke-GhostNativeCapture 'trunk' @('--version')) -join ' ') -split '\s+')[1] } else { '' }
    if ($trunkVersion -ne $GhostTrunkVersion) {
        Invoke-GhostChecked 'cargo' @('install', 'trunk', '--version', $GhostTrunkVersion, '--locked')
    }
    $tauriVersion = ''
    if (Get-Command cargo-tauri -ErrorAction SilentlyContinue) {
        $tauriVersion = (((Invoke-GhostNativeCapture 'cargo' @('tauri', '--version')) -join ' ') -split '\s+')[1]
    }
    if ($tauriVersion -ne $GhostTauriVersion) {
        Invoke-GhostChecked 'cargo' @('install', 'tauri-cli', '--version', $GhostTauriVersion, '--locked', '--force')
    }
}

function Get-JavaMajor([string]$Java) {
    $line = (Invoke-GhostNativeCapture $Java @('-version') | Select-Object -First 1) -join ''
    if ($line -match 'version "(\d+)') { return [int]$Matches[1] }
    return 0
}

function Find-GhostJdk {
    $candidates = @()
    if ($env:JAVA_HOME) { $candidates += $env:JAVA_HOME }
    $candidates += 'C:\Program Files\Android\Android Studio\jbr'
    $candidates += Get-ChildItem 'C:\Program Files\Microsoft' -Directory -Filter 'jdk-17*' -ErrorAction SilentlyContinue | ForEach-Object FullName
    foreach ($candidate in $candidates) {
        $java = Join-Path $candidate 'bin\java.exe'
        if ((Test-Path $java) -and (Get-JavaMajor $java) -ge 17) { return $candidate }
    }
    return $null
}

function Ensure-GhostJdk {
    $jdk = Find-GhostJdk
    if (-not $jdk) {
        if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
            throw 'Java 17+ is missing and winget is unavailable. Install a JDK 17+ and rerun.'
        }
        Invoke-GhostChecked 'winget' @('install', '-e', '--id', 'Microsoft.OpenJDK.17', '--accept-source-agreements', '--accept-package-agreements')
        $jdk = Find-GhostJdk
    }
    if (-not $jdk) { throw 'Java 17+ installation completed but JAVA_HOME could not be resolved.' }
    $env:JAVA_HOME = $jdk
    $env:PATH = "$(Join-Path $jdk 'bin');$env:PATH"
}

function Ensure-GhostAndroidSdk {
    $sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
    $env:ANDROID_HOME = $sdk
    $env:ANDROID_SDK_ROOT = $sdk
    $manager = Join-Path $sdk 'cmdline-tools\latest\bin\sdkmanager.bat'
    if (-not (Test-Path $manager)) {
        New-Item -ItemType Directory -Force -Path $sdk | Out-Null
        $zip = Join-Path $env:TEMP "commandlinetools-win-$GhostCmdlineTools.zip"
        $url = "https://dl.google.com/android/repository/commandlinetools-win-$($GhostCmdlineTools)_latest.zip"
        Invoke-WebRequest $url -OutFile $zip
        $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
        if ($actual -ne $GhostCmdlineSha256) { throw "Android command-line tools checksum mismatch: $actual" }
        $extract = Join-Path $env:TEMP 'ghost-talk-android-cli'
        Remove-Item -Recurse -Force $extract -ErrorAction SilentlyContinue
        Expand-Archive -Path $zip -DestinationPath $extract -Force
        $latest = Join-Path $sdk 'cmdline-tools\latest'
        Remove-Item -Recurse -Force $latest -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $latest | Out-Null
        Copy-Item -Recurse -Force (Join-Path $extract 'cmdline-tools\*') $latest
    }
    $answers = Join-Path $env:TEMP 'ghost-talk-android-license-answers.txt'
    (1..100 | ForEach-Object { 'y' }) | Set-Content -Encoding ascii $answers
    cmd.exe /d /s /c "`"$manager`" --licenses < `"$answers`"" | Out-Host
    Invoke-GhostChecked $manager @('--install', 'platform-tools', "platforms;android-$GhostAndroidApi", "build-tools;$GhostAndroidBuildTools", "ndk;$GhostAndroidNdk")
    $env:NDK_HOME = Join-Path $sdk "ndk\$GhostAndroidNdk"
    $env:PATH = "$(Join-Path $sdk 'platform-tools');$env:PATH"
}

function Find-GhostPython {
    foreach ($name in @('python', 'python3', 'py')) {
        $command = Get-Command $name -ErrorAction SilentlyContinue
        if ($command) { return $command.Source }
    }
    $local = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Python') -Recurse -Filter python.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($local) { return $local.FullName }
    return $null
}

function Ensure-GhostPython {
    $python = Find-GhostPython
    if (-not $python) {
        if (-not (Get-Command winget -ErrorAction SilentlyContinue)) { throw 'Python 3 is required and winget is unavailable.' }
        Invoke-GhostChecked 'winget' @('install', '-e', '--id', 'Python.Python.3.14', '--accept-source-agreements', '--accept-package-agreements')
        $python = Find-GhostPython
    }
    if (-not $python) { throw 'Python installation completed but python.exe could not be resolved.' }
    return $python
}

function Set-GhostWasmCToolchain {
    $helper = Join-Path (Get-GhostRepoRoot) 'scripts\tooling\ensure-wasm-clang.ps1'
    $resolved = (& $helper | Select-Object -Last 1).Trim()
    $parts = $resolved -split '\|', 2
    if ($parts.Count -ne 2 -or -not (Test-Path $parts[0]) -or -not (Test-Path $parts[1])) {
        throw "Invalid WASM C toolchain response: $resolved"
    }
    $env:GHOST_TALK_WASM_CLANG = $parts[0]
    $env:CC_wasm32_unknown_unknown = $parts[0]
    $env:AR_wasm32_unknown_unknown = $parts[1]
    Write-Host "WASM C compiler: $($parts[0])"
}

function Build-GhostFrontend {
    $app = Get-GhostAppRoot
    Set-GhostWasmCToolchain
    Push-Location (Join-Path $app 'crates\ghost-wasm')
    try { Invoke-GhostChecked 'trunk' @('build', '--release') } finally { Pop-Location }
    $frontend = Join-Path (Get-GhostRepoRoot) 'target\build\frontend\index.html'
    if (-not (Test-Path $frontend)) { throw "Ghost WASM frontend output is missing: $frontend" }
}

function Stage-GhostBuildArtifacts([string]$Platform, [string]$Profile) {
    $python = Ensure-GhostPython
    $script = Join-Path (Get-GhostAppRoot) 'scripts\artifacts\stage.py'
    Invoke-GhostChecked $python @($script, '--platform', $Platform, '--profile', $Profile)
}

function Remove-GhostAndroidPackageOutputs {
    $native = Get-GhostNativeRoot
    $repo = Get-GhostRepoRoot
    $roots = @(
        (Join-Path $native 'gen\android'),
        (Join-Path $repo 'target\aarch64-linux-android'),
        (Join-Path $repo 'target\armv7-linux-androideabi'),
        (Join-Path $repo 'target\android')
    )
    foreach ($root in $roots) {
        if (-not (Test-Path $root)) { continue }
        Get-ChildItem -Path $root -Recurse -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Extension -in @('.apk', '.aab') } |
            Remove-Item -Force -ErrorAction SilentlyContinue
    }
}

function Assert-GhostAndroidArtifacts([string]$Profile) {
    $repo = Get-GhostRepoRoot
    $destinations = @(
        (Join-Path $repo "target\android\$Profile"),
        (Join-Path $repo "target\dist\android\$Profile")
    )
    foreach ($destination in $destinations) {
        $apk = @(Get-ChildItem -Path $destination -File -Filter '*.apk' -ErrorAction SilentlyContinue)
        $aab = @(Get-ChildItem -Path $destination -File -Filter '*.aab' -ErrorAction SilentlyContinue)
        if ($apk.Count -eq 0 -or $aab.Count -eq 0) {
            throw "Android $Profile build did not stage both an APK and AAB: $destination"
        }
        Write-Host "Verified Android $Profile APK/AAB: $destination" -ForegroundColor Green
    }
}

function Remove-GhostLegacyBuildOutputs {
    $app = Get-GhostAppRoot
    foreach ($path in @(
        (Join-Path $app 'target'),
        (Join-Path $app 'crates\ghost-wasm\dist'),
        (Join-Path $app 'crates\ghost-talk-native\frontend')
    )) {
        Remove-Item -Recurse -Force $path -ErrorAction SilentlyContinue
    }
}

function Remove-GhostAndroidTransientBuildState {
    $android = Join-Path (Get-GhostNativeRoot) 'gen\android'
    if (-not (Test-Path $android)) { return }
    Get-ChildItem $android -Directory -Recurse -Force -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -in @('build', '.gradle', '.cxx', '.externalNativeBuild') } |
        Sort-Object { $_.FullName.Length } -Descending |
        ForEach-Object { Remove-Item -Recurse -Force $_.FullName -ErrorAction SilentlyContinue }
    Remove-Item -Recurse -Force (Join-Path $android 'app\src\main\jniLibs') -ErrorAction SilentlyContinue
}

function Initialize-GhostAndroidProject([string]$Python) {
    $native = Get-GhostNativeRoot
    Push-Location $native
    try {
        if (-not (Test-Path (Join-Path $native 'gen\android'))) { Invoke-GhostChecked 'cargo' @('tauri', 'android', 'init') }
    } finally { Pop-Location }
    Invoke-GhostChecked $Python @((Join-Path (Get-GhostAppRoot) 'scripts\mobile\configure.py'), 'android', $native)
}

function Initialize-GhostAndroidEnvironment {
    $env:CARGO_TARGET_DIR = Join-Path (Get-GhostRepoRoot) 'target'
    Remove-GhostLegacyBuildOutputs
    Ensure-GhostRustTools
    Ensure-GhostJdk
    Ensure-GhostAndroidSdk
    $python = Ensure-GhostPython
    Build-GhostFrontend
    Initialize-GhostAndroidProject $python
}


function Get-GhostArmAndroidDevice {
    $adb = Get-Command adb -ErrorAction SilentlyContinue
    if (-not $adb) { throw 'adb is unavailable after Android SDK bootstrap.' }
    $serials = @(& $adb.Source devices | Select-Object -Skip 1 | ForEach-Object {
        if ($_ -match '^([^\s]+)\s+device$') { $Matches[1] }
    })
    foreach ($serial in $serials) {
        $abi = ((& $adb.Source -s $serial shell getprop ro.product.cpu.abi 2>$null) -join '').Trim()
        if ($abi -in @('arm64-v8a', 'armeabi-v7a')) { return $serial }
    }
    if ($serials.Count -gt 0) {
        throw 'Connected Android targets are x86/x86_64 only. Rusty-Kaspa v2.0.1 has an upstream x86_64-Android build-script failure; connect an ARM64/ARMv7 Android device.'
    }
    throw 'No Android device is connected. Connect an ARM64/ARMv7 Android device before running Ghost Talk.'
}
