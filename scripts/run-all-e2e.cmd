@echo off
rem Ghost Talk two-instance end-to-end suite on Kaspa testnet-10.
rem Builds the web release (one WASM module with p2p-net), checks the persistent
rem dev wallet (prompting for faucet funding when needed), tops up the two
rem instance wallets, starts a local p2p-net relay, and drives two isolated
rem browser instances through direct chat, p2p-net, Kaspa-only, and Room flows.
setlocal EnableExtensions
cd /d "%~dp0\.."
set "ROOT=%CD%"
set "APP=%ROOT%\examples\ghost-talk-app"
set "E2E=%APP%\e2e"
set "OUT=%ROOT%\target\e2e"
set "EXIT_CODE=1"
if "%GHOST_E2E_NETWORK%"=="" set "GHOST_E2E_NETWORK=testnet-10"
if "%GHOST_E2E_MIN_DEV_KAS%"=="" set "GHOST_E2E_MIN_DEV_KAS=25"
if not "%~1"=="" (echo ERROR: run-all-e2e accepts no filters; every scenario is mandatory. & set "EXIT_CODE=2" & goto :report)
if not exist "%OUT%" mkdir "%OUT%"

for %%T in (cargo trunk node npm python) do (
  where %%T >nul 2>&1 || (echo ERROR: %%T is required for the E2E suite. & goto :report)
)
if exist "%ProgramFiles%\LLVM\bin\clang.exe" (
  set "CC_wasm32_unknown_unknown=%ProgramFiles%\LLVM\bin\clang.exe"
  set "AR_wasm32_unknown_unknown=%ProgramFiles%\LLVM\bin\llvm-ar.exe"
)
call scripts\tooling\ensure-wasm-target.cmd || goto :report

echo ==^> trunk build --release
if exist "%ROOT%	argetuildrontend" rmdir /s /q "%ROOT%	argetuildrontend"
pushd "%APP%\crates\ghost-wasm"
trunk build --release
set "BUILD=%ERRORLEVEL%"
popd
if not "%BUILD%"=="0" goto :report

cargo build --release --manifest-path "%E2E%\harness\Cargo.toml" || goto :report
set "HARNESS=%E2E%\harness\target\release\ghost-e2e.exe"
"%HARNESS%" ensure --network %GHOST_E2E_NETWORK% --min-kas %GHOST_E2E_MIN_DEV_KAS% || goto :report
"%HARNESS%" fund --network %GHOST_E2E_NETWORK% --min-kas 5 --topup-kas 10 || goto :report
"%HARNESS%" status --network %GHOST_E2E_NETWORK% > "%OUT%\wallets.json" || goto :report

if exist "%OUT%\relay.json" del "%OUT%\relay.json"
start "ghost-e2e relay" /b cmd /c ""%HARNESS%" relay --network %GHOST_E2E_NETWORK% > "%OUT%\relay.json" 2> "%OUT%\relay.err""
for /l %%I in (1,1,120) do (
  findstr /c:"peerId" "%OUT%\relay.json" >nul 2>&1 && goto :relay_ready
  ping -n 2 127.0.0.1 >nul
)
echo ERROR: local p2p-net relay did not start.
type "%OUT%\relay.err"
goto :stop_relay

:relay_ready
pushd "%E2E%\playwright"
call npm ci --no-audit --no-fund
if errorlevel 1 (popd & goto :stop_relay)
call npx playwright install chromium
if errorlevel 1 (popd & goto :stop_relay)
set "GHOST_E2E_RELAY_FILE=%OUT%\relay.json"
set "GHOST_E2E_WALLETS_FILE=%OUT%\wallets.json"
call npx playwright test
set "EXIT_CODE=%ERRORLEVEL%"
popd

:stop_relay
taskkill /f /im ghost-e2e.exe >nul 2>&1

:report
echo.
if "%EXIT_CODE%"=="0" (echo ALL E2E SCENARIOS PASSED. Report: %OUT%\report\index.html) else (echo E2E FAILED with exit code %EXIT_CODE%. Report: %OUT%\report\index.html)
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %EXIT_CODE%
