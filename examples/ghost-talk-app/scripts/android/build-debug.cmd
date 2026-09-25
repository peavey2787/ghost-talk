@echo off
call "%~dp0build.cmd" -Debug
exit /b %ERRORLEVEL%
