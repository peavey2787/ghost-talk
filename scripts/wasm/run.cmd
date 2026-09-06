@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\.."
rustup target add wasm32-unknown-unknown || goto :fail
cargo build -p ghost-wasm --target wasm32-unknown-unknown || goto :fail
cd crates\ghost-app
if exist package-lock.json (call npm ci) else (call npm install)
if errorlevel 1 goto :fail
call npm run dev -- --host
if errorlevel 1 goto :fail
set "RC=0" & goto :done
:fail
set "RC=%ERRORLEVEL%"
if "%RC%"=="0" set "RC=1"
:done
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %RC%
