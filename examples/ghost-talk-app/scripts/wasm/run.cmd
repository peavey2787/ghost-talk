@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\.."
set "RC=0"
for %%I in ("%CD%\..\..") do set "REPO_ROOT=%%~fI"
set "CARGO_TARGET_DIR=%REPO_ROOT%\target"
call "%~dp0..\windows\_ensure-tools.cmd"
if errorlevel 1 goto :fail
pushd crates\ghost-wasm || goto :fail
trunk serve --address 0.0.0.0
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" goto :done
goto :done
:fail
set "RC=%ERRORLEVEL%"
if "%RC%"=="0" set "RC=1"
:done
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %RC%
