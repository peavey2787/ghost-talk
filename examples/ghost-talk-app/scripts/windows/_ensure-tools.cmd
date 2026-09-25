@echo off
rem Internal helper used by the Windows run/build entrypoints.
rem Installs the pinned Rust/WASM/Tauri tools when they are missing or the wrong version.

set "GHOST_TALK_RUST_VERSION=1.98.0"
set "GHOST_TALK_TRUNK_VERSION=0.21.14"
set "GHOST_TALK_TAURI_VERSION=2.11.4"
for %%I in ("%~dp0..\..\..\..") do set "REPO_ROOT=%%~fI"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where rustup >nul 2>&1 || goto :missing_rust
where cargo >nul 2>&1 || goto :missing_rust

rustup toolchain list | findstr /b /c:"%GHOST_TALK_RUST_VERSION%-" >nul 2>&1
if errorlevel 1 (
  echo Installing Rust %GHOST_TALK_RUST_VERSION%...
  rustup toolchain install %GHOST_TALK_RUST_VERSION% --profile minimal --component rustfmt --component clippy || exit /b 1
)

call "%REPO_ROOT%\scripts\tooling\ensure-wasm-target.cmd" %GHOST_TALK_RUST_VERSION%
if errorlevel 1 exit /b 1

set "GHOST_TALK_WASM_TOOLCHAIN="
for /f "usebackq delims=" %%W in (`powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%REPO_ROOT%\scripts\tooling\ensure-wasm-clang.ps1"`) do set "GHOST_TALK_WASM_TOOLCHAIN=%%W"
if not defined GHOST_TALK_WASM_TOOLCHAIN goto :tool_failed
for /f "tokens=1,2 delims=|" %%A in ("%GHOST_TALK_WASM_TOOLCHAIN%") do (
  set "GHOST_TALK_WASM_CLANG=%%~A"
  set "CC_wasm32_unknown_unknown=%%~A"
  set "AR_wasm32_unknown_unknown=%%~B"
)
if not exist "%CC_wasm32_unknown_unknown%" goto :tool_failed
if not exist "%AR_wasm32_unknown_unknown%" goto :tool_failed

set "GHOST_TALK_TRUNK_OK="
for /f "tokens=2" %%V in ('trunk --version 2^>nul') do if "%%V"=="%GHOST_TALK_TRUNK_VERSION%" set "GHOST_TALK_TRUNK_OK=1"
if not defined GHOST_TALK_TRUNK_OK (
  echo Installing trunk %GHOST_TALK_TRUNK_VERSION%...
  rustup run %GHOST_TALK_RUST_VERSION% cargo install trunk --version %GHOST_TALK_TRUNK_VERSION% --locked || exit /b 1
)

set "GHOST_TALK_TAURI_OK="
for /f "tokens=2" %%V in ('cargo tauri --version 2^>nul') do if "%%V"=="%GHOST_TALK_TAURI_VERSION%" set "GHOST_TALK_TAURI_OK=1"
if not defined GHOST_TALK_TAURI_OK (
  echo Installing tauri-cli %GHOST_TALK_TAURI_VERSION%...
  rustup run %GHOST_TALK_RUST_VERSION% cargo install tauri-cli --version %GHOST_TALK_TAURI_VERSION% --locked --force || exit /b 1
)

trunk --version | findstr /c:"%GHOST_TALK_TRUNK_VERSION%" >nul 2>&1 || goto :tool_failed
cargo tauri --version | findstr /c:"%GHOST_TALK_TAURI_VERSION%" >nul 2>&1 || goto :tool_failed
exit /b 0

:missing_rust
echo ERROR: Rust and rustup are required before Ghost Talk can build.
echo Install Rust once from https://rustup.rs/ and then double-click run.cmd or build.cmd again.
exit /b 1

:tool_failed
echo ERROR: Ghost Talk could not prepare its pinned Rust/WASM build tools.
exit /b 1
