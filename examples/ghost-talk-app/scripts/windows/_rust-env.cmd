@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "CARGO_EXE="
set "CARGO_BIN="
set "RUSTUP_EXE="
set "RUSTC_EXE="

rem Cargo is required to start QA. rustup is optional until a missing target must be installed.
if defined GHOST_TALK_CARGO_EXE call :probe_cargo "%GHOST_TALK_CARGO_EXE%"
if defined CARGO_EXE goto :resolve_optional

rem Try the inherited command directly before making any installation-layout assumptions.
cargo.exe --version >nul 2>&1
if not errorlevel 1 (
  for %%I in (cargo.exe) do if not defined CARGO_EXE call :probe_cargo "%%~$PATH:I"
)
if defined CARGO_EXE goto :resolve_optional

rem Standard Cargo homes and common Windows developer installations.
if defined CARGO_HOME call :probe_cargo "%CARGO_HOME%\bin\cargo.exe"
if defined CARGO_EXE goto :resolve_optional
call :probe_cargo "%USERPROFILE%\.cargo\bin\cargo.exe"
if defined CARGO_EXE goto :resolve_optional
if defined HOME call :probe_cargo "%HOME%\.cargo\bin\cargo.exe"
if defined CARGO_EXE goto :resolve_optional
call :probe_cargo "%USERPROFILE%\scoop\shims\cargo.exe"
if defined CARGO_EXE goto :resolve_optional
call :probe_cargo "C:\msys64\mingw64\bin\cargo.exe"
if defined CARGO_EXE goto :resolve_optional
call :probe_cargo "C:\msys64\ucrt64\bin\cargo.exe"
if defined CARGO_EXE goto :resolve_optional
call :probe_cargo "C:\msys64\clang64\bin\cargo.exe"
if defined CARGO_EXE goto :resolve_optional

endlocal & exit /b 1

:probe_cargo
if "%~1"=="" exit /b 0
if not exist "%~1" exit /b 0
"%~1" --version >nul 2>&1
if errorlevel 1 exit /b 0
for %%I in ("%~1") do (
  set "CARGO_EXE=%%~fI"
  set "CARGO_BIN=%%~dpI"
)
exit /b 0

:probe_optional
if "%~1"=="" exit /b 0
if not exist "%~1" exit /b 0
"%~1" --version >nul 2>&1
if errorlevel 1 exit /b 0
if /I "%~2"=="rustup" for %%I in ("%~1") do set "RUSTUP_EXE=%%~fI"
if /I "%~2"=="rustc" for %%I in ("%~1") do set "RUSTC_EXE=%%~fI"
exit /b 0

:resolve_optional
rem Prefer explicit parent overrides, then the Cargo bin and normal rustup locations.
if defined GHOST_TALK_RUSTUP_EXE call :probe_optional "%GHOST_TALK_RUSTUP_EXE%" rustup
if not defined RUSTUP_EXE call :probe_optional "%CARGO_BIN%rustup.exe" rustup
if not defined RUSTUP_EXE if defined CARGO_HOME call :probe_optional "%CARGO_HOME%\bin\rustup.exe" rustup
if not defined RUSTUP_EXE call :probe_optional "%USERPROFILE%\.cargo\bin\rustup.exe" rustup
if not defined RUSTUP_EXE if defined HOME call :probe_optional "%HOME%\.cargo\bin\rustup.exe" rustup
if not defined RUSTUP_EXE call :probe_optional "%USERPROFILE%\scoop\shims\rustup.exe" rustup

if defined GHOST_TALK_RUSTC_EXE call :probe_optional "%GHOST_TALK_RUSTC_EXE%" rustc
if not defined RUSTC_EXE call :probe_optional "%CARGO_BIN%rustc.exe" rustc
if not defined RUSTC_EXE if defined CARGO_HOME call :probe_optional "%CARGO_HOME%\bin\rustc.exe" rustc
if not defined RUSTC_EXE call :probe_optional "%USERPROFILE%\.cargo\bin\rustc.exe" rustc
if not defined RUSTC_EXE if defined HOME call :probe_optional "%HOME%\.cargo\bin\rustc.exe" rustc

rem Strip the trailing slash from the exported Cargo bin for stable diagnostics/PATH use.
for %%I in ("%CARGO_BIN%.") do set "CARGO_BIN=%%~fI"
endlocal & set "GHOST_TALK_CARGO_BIN=%CARGO_BIN%" & set "GHOST_TALK_CARGO_EXE=%CARGO_EXE%" & set "GHOST_TALK_RUSTUP_EXE=%RUSTUP_EXE%" & set "GHOST_TALK_RUSTC_EXE=%RUSTC_EXE%" & set "PATH=%CARGO_BIN%;%PATH%" & exit /b 0
