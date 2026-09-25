@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1" %*
set "RC=%ERRORLEVEL%"
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %RC%
