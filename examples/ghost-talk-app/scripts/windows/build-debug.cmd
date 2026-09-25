@echo off
call "%~dp0build.cmd" debug
exit /b %ERRORLEVEL%
