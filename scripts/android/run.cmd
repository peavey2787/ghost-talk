@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\..\crates\ghost-app"
if not exist node_modules (
  if exist package-lock.json (call npm ci) else (call npm install)
  if errorlevel 1 goto :fail
)
if not exist src-tauri\gen\android call npm run tauri -- android init
if errorlevel 1 goto :fail
call npm run tauri -- android dev
if errorlevel 1 goto :fail
set "RC=0" & goto :done
:fail
set "RC=%ERRORLEVEL%"
if "%RC%"=="0" set "RC=1"
:done
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %RC%
