@echo off
setlocal EnableExtensions
for %%I in ("%~dp0..\..\..\..") do set "REPO_ROOT=%%~fI"
set "FRONTEND=%REPO_ROOT%\target\build\frontend"
if not exist "%FRONTEND%\index.html" (
  echo ERROR: Ghost WASM frontend output is missing: %FRONTEND%
  exit /b 1
)
echo Frontend staged in target: %FRONTEND%
exit /b 0
