@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\.."
set "EXIT_CODE=0"
call scripts\windows\_msvc-env.cmd
if errorlevel 1 goto :fail
echo Ghost Talk - Windows bootstrap
where cargo >nul 2>&1 || (echo ERROR: Rust/rustup is required. Install from https://rustup.rs/ & goto :fail)
where node >nul 2>&1 || (echo ERROR: Node.js is required. & goto :fail)
where npm >nul 2>&1 || (echo ERROR: npm is required. & goto :fail)
rustup toolchain install 1.95.0 --component clippy,rustfmt || goto :fail
rustup target add wasm32-unknown-unknown || goto :fail
cargo install cargo-fuzz --locked || goto :fail
cd crates\ghost-app
if exist package-lock.json (call npm ci --include=dev) else (call npm install --include=dev)
if errorlevel 1 goto :fail
if not exist node_modules\.bin\tauri.cmd (
  echo ERROR: local Tauri CLI was not installed.
  echo Expected: crates\ghost-app\node_modules\.bin\tauri.cmd
  echo Bootstrap requires devDependencies, including @tauri-apps/cli.
  goto :fail
)
echo.
echo Windows bootstrap complete.
goto :done
:fail
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%"=="0" set "EXIT_CODE=1"
echo.
echo Bootstrap FAILED with exit code %EXIT_CODE%.
:done
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %EXIT_CODE%
