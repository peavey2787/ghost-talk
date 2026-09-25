@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "TARGET=wasm32-unknown-unknown"
set "TOOLCHAIN=%~1"
set "RUSTUP_EXE=%GHOST_TALK_RUSTUP_EXE%"
set "RUSTC_EXE=%GHOST_TALK_RUSTC_EXE%"

rem A preinstalled target must work even when rustup is not available.
call :target_usable
if not errorlevel 1 goto :success

rem rustup is only required to install a target that is actually missing.
if not defined RUSTUP_EXE for %%I in (rustup.exe) do set "RUSTUP_EXE=%%~$PATH:I"
if not defined RUSTUP_EXE (
  echo ERROR: %TARGET% is not usable and rustup was not found.
  echo ERROR: Install the target with your Rust toolchain manager, then rerun QA.
  endlocal & exit /b 127
)
if not defined TOOLCHAIN (
  for /f "tokens=1" %%T in ('"%RUSTUP_EXE%" show active-toolchain 2^>nul') do if not defined TOOLCHAIN set "TOOLCHAIN=%%T"
)
if not defined TOOLCHAIN (
  echo ERROR: rustup could not resolve the active Rust toolchain for this checkout.
  endlocal & exit /b 1
)

echo Installing %TARGET% for Rust %TOOLCHAIN%...
"%RUSTUP_EXE%" target add --toolchain "%TOOLCHAIN%" %TARGET%
if errorlevel 1 goto :install_failed
call :target_usable
if errorlevel 1 goto :verify_failed
goto :success

:target_usable
set "PROBE_BASE=%TEMP%\ghost-talk-wasm-target-%RANDOM%-%RANDOM%"
set "PROBE_SOURCE=%PROBE_BASE%.rs"
set "PROBE_OUTPUT=%PROBE_BASE%.rmeta"
>"%PROBE_SOURCE%" echo #![no_std]
>>"%PROBE_SOURCE%" echo pub fn ghost_talk_wasm_target_probe() {}
set "PROBE_EXIT=1"
if defined RUSTUP_EXE if defined TOOLCHAIN goto :probe_rustup
if not defined RUSTC_EXE for %%I in (rustc.exe) do set "RUSTC_EXE=%%~$PATH:I"
if defined RUSTC_EXE goto :probe_rustc
goto :probe_done

:probe_rustup
"%RUSTUP_EXE%" run "%TOOLCHAIN%" rustc --target %TARGET% --crate-name ghost_talk_wasm_target_probe --crate-type lib --emit metadata "%PROBE_SOURCE%" -o "%PROBE_OUTPUT%" >nul 2>&1
set "PROBE_EXIT=%ERRORLEVEL%"
goto :probe_done

:probe_rustc
"%RUSTC_EXE%" --target %TARGET% --crate-name ghost_talk_wasm_target_probe --crate-type lib --emit metadata "%PROBE_SOURCE%" -o "%PROBE_OUTPUT%" >nul 2>&1
set "PROBE_EXIT=%ERRORLEVEL%"

:probe_done
del /q "%PROBE_SOURCE%" "%PROBE_OUTPUT%" >nul 2>&1
exit /b %PROBE_EXIT%

:install_failed
echo ERROR: failed to install %TARGET% for Rust %TOOLCHAIN%.
endlocal & exit /b 1

:verify_failed
echo ERROR: Rust %TOOLCHAIN% still cannot compile for %TARGET% after target installation.
endlocal & exit /b 1

:success
endlocal & exit /b 0
