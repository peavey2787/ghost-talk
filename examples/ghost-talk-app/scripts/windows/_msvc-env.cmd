@echo off
rem Ghost Talk Windows native build environment bootstrap.
rem This file is meant to be CALLed so any environment changes remain active.

rem tauri-winres accepts either RC on the environment or rc.exe on PATH.
rem Clear a stale/non-file RC value before discovery so it cannot block SDK probing.
if defined RC (
  if exist "%RC%" goto :ready
  where "%RC%" >nul 2>&1 && goto :ready
  set "RC="
)
where rc.exe >nul 2>&1 && goto :ready

rem Prefer Visual Studio's own developer environment when available.
set "GHOST_TALK_VSDEVCMD="
set "GHOST_TALK_VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist "%GHOST_TALK_VSWHERE%" (
  for /f "usebackq delims=" %%I in (`"%GHOST_TALK_VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find Common7\Tools\VsDevCmd.bat`) do if not defined GHOST_TALK_VSDEVCMD set "GHOST_TALK_VSDEVCMD=%%I"
)

if not defined GHOST_TALK_VSDEVCMD if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat" set "GHOST_TALK_VSDEVCMD=%ProgramFiles%\Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat"
if not defined GHOST_TALK_VSDEVCMD if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Professional\Common7\Tools\VsDevCmd.bat" set "GHOST_TALK_VSDEVCMD=%ProgramFiles%\Microsoft Visual Studio\2022\Professional\Common7\Tools\VsDevCmd.bat"
if not defined GHOST_TALK_VSDEVCMD if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\Common7\Tools\VsDevCmd.bat" set "GHOST_TALK_VSDEVCMD=%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\Common7\Tools\VsDevCmd.bat"
if not defined GHOST_TALK_VSDEVCMD if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" set "GHOST_TALK_VSDEVCMD=%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"

if defined GHOST_TALK_VSDEVCMD (
  call "%GHOST_TALK_VSDEVCMD%" -no_logo -arch=x64 -host_arch=x64 >nul
  if errorlevel 1 goto :vs_failed
  where rc.exe >nul 2>&1 && goto :ready
)

rem Some Windows SDK installs contain RC.EXE but do not put it on PATH.
rem Find the newest x64 SDK and point tauri-winres at it directly.
set "GHOST_TALK_SDK_BIN=%ProgramFiles(x86)%\Windows Kits\10\bin"
if exist "%GHOST_TALK_SDK_BIN%" (
  for /f "delims=" %%V in ('dir /b /ad /o-n "%GHOST_TALK_SDK_BIN%" 2^>nul') do call :try_sdk "%GHOST_TALK_SDK_BIN%\%%V\x64\rc.exe"
)
if defined RC if exist "%RC%" goto :ready

rem Older SDK layouts may place x64 directly under bin.
if exist "%GHOST_TALK_SDK_BIN%\x64\rc.exe" (
  set "RC=%GHOST_TALK_SDK_BIN%\x64\rc.exe"
  set "PATH=%GHOST_TALK_SDK_BIN%\x64;%PATH%"
  goto :ready
)

echo ERROR: Microsoft Windows Resource Compiler ^(RC.EXE^) was not found.
echo.
echo Ghost Talk/Tauri Windows builds require the MSVC C++ build tools and a Windows SDK.
echo Open Visual Studio Installer and add:
echo   - Desktop development with C++
echo   - MSVC v143 x64/x86 build tools
echo   - Windows 10 or Windows 11 SDK
echo.
echo Then double-click this script again. You do NOT need to use a Developer Command Prompt.
exit /b 1

:try_sdk
if defined RC exit /b 0
if exist "%~1" (
  set "RC=%~1"
  for %%D in ("%~1") do set "PATH=%%~dpD;%PATH%"
)
exit /b 0

:vs_failed
echo ERROR: Visual Studio developer environment initialization failed.
exit /b 1

:ready
set "GHOST_TALK_RC_FOUND="
if not defined RC goto :resolve_rc_from_path
if exist "%RC%" goto :announce_rc
for /f "delims=" %%I in ('where "%RC%" 2^>nul') do if not defined GHOST_TALK_RC_FOUND set "GHOST_TALK_RC_FOUND=%%I"
if not defined GHOST_TALK_RC_FOUND goto :missing_rc_after_ready
set "RC=%GHOST_TALK_RC_FOUND%"
goto :announce_rc

:resolve_rc_from_path
for /f "delims=" %%I in ('where rc.exe 2^>nul') do if not defined GHOST_TALK_RC_FOUND set "GHOST_TALK_RC_FOUND=%%I"
if not defined GHOST_TALK_RC_FOUND goto :missing_rc_after_ready
set "RC=%GHOST_TALK_RC_FOUND%"

:announce_rc
echo Windows SDK resource compiler: %RC%
set "GHOST_TALK_VSDEVCMD="
set "GHOST_TALK_VSWHERE="
set "GHOST_TALK_SDK_BIN="
set "GHOST_TALK_RC_FOUND="
exit /b 0

:missing_rc_after_ready
echo ERROR: RC.EXE discovery reached an invalid ready state.
exit /b 1
