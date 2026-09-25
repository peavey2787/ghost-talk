@echo off
call "%~dp0scripts\wasm\build.cmd"
exit /b %ERRORLEVEL%
