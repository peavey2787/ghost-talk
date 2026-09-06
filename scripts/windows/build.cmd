@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\.."
set "EXIT_CODE=0"
call scripts\windows\_msvc-env.cmd
if errorlevel 1 goto :fail
echo Ghost Talk - Windows build
cd crates\ghost-app
if exist package-lock.json (call npm ci --include=dev) else (call npm install --include=dev)
if errorlevel 1 goto :fail
if not exist node_modules\.bin\tauri.cmd (
  echo ERROR: local Tauri CLI was not installed.
  echo Expected: crates\ghost-app\node_modules\.bin\tauri.cmd
  echo The build requires devDependencies, including @tauri-apps/cli.
  goto :fail
)
call node_modules\.bin\tauri.cmd build
if errorlevel 1 goto :fail
echo.
echo Ghost Talk Windows build complete.
goto :done
:fail
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%"=="0" set "EXIT_CODE=1"
echo.
echo Windows build FAILED with exit code %EXIT_CODE%.
:done
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %EXIT_CODE%
