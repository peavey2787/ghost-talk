@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\.."
set "EXIT_CODE=0"
set "BUILD_PROFILE=release"
if /I "%~1"=="debug" set "BUILD_PROFILE=debug"
for %%I in ("%CD%\..\..") do set "REPO_ROOT=%%~fI"
set "CARGO_TARGET_DIR=%REPO_ROOT%\target"

if exist "%CD%\target" rmdir /s /q "%CD%\target"
if exist "%CD%\crates\ghost-wasm\dist" rmdir /s /q "%CD%\crates\ghost-wasm\dist"
if exist "%CD%\crates\ghost-talk-native\frontend" rmdir /s /q "%CD%\crates\ghost-talk-native\frontend"

call "%~dp0_msvc-env.cmd"
if errorlevel 1 goto :fail
call "%~dp0_ensure-tools.cmd"
if errorlevel 1 goto :fail
call "%~dp0_python-env.cmd"
if errorlevel 1 goto :missing_python

echo Ghost Talk - Windows %BUILD_PROFILE% Rust/WASM build
pushd crates\ghost-wasm || goto :fail
if /I "%BUILD_PROFILE%"=="debug" (
  trunk build
) else (
  trunk build --release
)
set "EXIT_CODE=%ERRORLEVEL%"
popd
if not "%EXIT_CODE%"=="0" goto :fail_code
call "%~dp0_stage-frontend.cmd"
if errorlevel 1 goto :fail
pushd crates\ghost-talk-native
if /I "%BUILD_PROFILE%"=="debug" (
  cargo tauri build --debug
) else (
  cargo tauri build
)
set "EXIT_CODE=%ERRORLEVEL%"
popd
if not "%EXIT_CODE%"=="0" goto :fail_code

"%GHOST_TALK_PYTHON_EXE%" %GHOST_TALK_PYTHON_ARGS% "%CD%\scripts\artifacts\stage.py" --platform windows --profile %BUILD_PROFILE%
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" goto :fail_code

echo.
echo Ghost Talk Windows %BUILD_PROFILE% build complete.
echo Final files: %REPO_ROOT%\target\dist\windows\%BUILD_PROFILE%
goto :done

:missing_python
echo ERROR: Python 3 is required to stage distributable artifacts.
set "EXIT_CODE=1"
goto :fail_code

:fail
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%"=="0" set "EXIT_CODE=1"
:fail_code
echo.
echo Windows build FAILED with exit code %EXIT_CODE%.
:done
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %EXIT_CODE%
