param(
    [switch]$ResolveOnly
)

$ErrorActionPreference = 'Stop'
$LlvmPackage = 'LLVM.LLVM'

function Write-GhostToolingStatus([string]$Message) {
    [Console]::Error.WriteLine($Message)
}

function Test-GhostWasmClang([string]$Clang) {
    if (-not $Clang -or -not (Test-Path $Clang)) { return $false }
    $llvmAr = Join-Path (Split-Path -Parent $Clang) 'llvm-ar.exe'
    if (-not (Test-Path $llvmAr)) { return $false }

    $probeRoot = Join-Path $env:TEMP "ghost-talk-wasm-clang-$PID"
    $source = Join-Path $probeRoot 'probe.c'
    $object = Join-Path $probeRoot 'probe.o'
    try {
        New-Item -ItemType Directory -Force -Path $probeRoot | Out-Null
        Set-Content -Path $source -Value 'int ghost_talk_wasm_probe(void) { return 0; }' -Encoding ascii
        & $Clang '--target=wasm32-unknown-unknown' '-c' $source '-o' $object 2>$null | Out-Null
        return ($LASTEXITCODE -eq 0 -and (Test-Path $object))
    } catch {
        return $false
    } finally {
        Remove-Item -Recurse -Force $probeRoot -ErrorAction SilentlyContinue
    }
}

function Find-GhostWasmClang {
    $candidates = @()
    if ($env:GHOST_TALK_WASM_CLANG) { $candidates += $env:GHOST_TALK_WASM_CLANG }
    if ($env:CC_wasm32_unknown_unknown) { $candidates += $env:CC_wasm32_unknown_unknown }
    $command = Get-Command clang.exe -ErrorAction SilentlyContinue
    if ($command) { $candidates += $command.Source }
    if ($env:ProgramFiles) { $candidates += (Join-Path $env:ProgramFiles 'LLVM\bin\clang.exe') }
    if (${env:ProgramFiles(x86)}) { $candidates += (Join-Path ${env:ProgramFiles(x86)} 'LLVM\bin\clang.exe') }

    foreach ($candidate in $candidates | Select-Object -Unique) {
        if (Test-GhostWasmClang $candidate) { return (Resolve-Path $candidate).Path }
    }
    return $null
}

$clang = Find-GhostWasmClang
if (-not $clang -and -not $ResolveOnly) {
    $winget = Get-Command winget.exe -ErrorAction SilentlyContinue
    if (-not $winget) {
        throw 'LLVM/Clang with WebAssembly support is required by secp256k1-sys, and winget is unavailable to install it automatically.'
    }
    Write-GhostToolingStatus 'Installing LLVM/Clang for the wasm32-unknown-unknown C dependency build...'
    $wingetExe = $winget.Source
    & $wingetExe install --exact --id $LlvmPackage --accept-source-agreements --accept-package-agreements --disable-interactivity 2>&1 | ForEach-Object { Write-GhostToolingStatus ($_.ToString()) }
    $wingetExit = $LASTEXITCODE
    $clang = Find-GhostWasmClang
    if ($wingetExit -ne 0 -and -not $clang) { throw "winget failed to install $LlvmPackage (exit $wingetExit)." }
}

if (-not $clang) {
    throw 'A WebAssembly-capable clang.exe plus llvm-ar.exe could not be resolved. Install LLVM and rerun the build.'
}

$llvmAr = Join-Path (Split-Path -Parent $clang) 'llvm-ar.exe'
Write-Output "$clang|$llvmAr"
