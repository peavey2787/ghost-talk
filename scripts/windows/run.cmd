@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\..\crates\ghost-app"
set "EXIT_CODE=0"
call "%~dp0_msvc-env.cmd"
if errorlevel 1 goto :fail
echo Ghost Talk - Windows development run
if not exist node_modules\.bin\tauri.cmd (
  if exist package-lock.json (call npm ci --include=dev) else (call npm install --include=dev)
  if errorlevel 1 goto :fail
)
if not exist node_modules\.bin\tauri.cmd (
  echo ERROR: local Tauri CLI was not installed.
  echo Expected: crates\ghost-app\node_modules\.bin\tauri.cmd
  echo Development run requires devDependencies, including @tauri-apps/cli.
  goto :fail
)
call node_modules\.bin\tauri.cmd dev
if errorlevel 1 goto :fail
goto :done
:fail
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%"=="0" set "EXIT_CODE=1"
echo.
echo Ghost Talk exited with error %EXIT_CODE%.
:done
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %EXIT_CODE%
