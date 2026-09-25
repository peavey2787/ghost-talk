@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\.."
set "EXIT_CODE=0"
for %%I in ("%CD%\..\..") do set "REPO_ROOT=%%~fI"
set "CARGO_TARGET_DIR=%REPO_ROOT%\target"
if exist "%CD%\target" rmdir /s /q "%CD%\target"
if exist "%CD%\crates\ghost-wasm\dist" rmdir /s /q "%CD%\crates\ghost-wasm\dist"
if exist "%CD%\crates\ghost-talk-native\frontend" rmdir /s /q "%CD%\crates\ghost-talk-native\frontend"
call "%~dp0_msvc-env.cmd"
if errorlevel 1 goto :fail
call "%~dp0_ensure-tools.cmd"
if errorlevel 1 goto :fail

echo Ghost Talk - Windows Rust/WASM development run
pushd crates\ghost-wasm || goto :fail
trunk build
set "EXIT_CODE=%ERRORLEVEL%"
popd
if not "%EXIT_CODE%"=="0" goto :fail_code
call "%~dp0_stage-frontend.cmd"
if errorlevel 1 goto :fail
pushd crates\ghost-talk-native
cargo tauri dev
set "EXIT_CODE=%ERRORLEVEL%"
popd
if not "%EXIT_CODE%"=="0" goto :fail_code
goto :done

:fail
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%"=="0" set "EXIT_CODE=1"
:fail_code
echo.
echo Ghost Talk exited with error %EXIT_CODE%.
:done
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %EXIT_CODE%
