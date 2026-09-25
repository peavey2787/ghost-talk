@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\.."
set "RC=0"
for %%I in ("%CD%\..\..") do set "REPO_ROOT=%%~fI"
set "CARGO_TARGET_DIR=%REPO_ROOT%\target"
set "FRONTEND_BUILD=%REPO_ROOT%\target\build\frontend"
set "WEB_RELEASE=%REPO_ROOT%\target\dist\web\release"

call "%~dp0..\windows\_ensure-tools.cmd"
if errorlevel 1 goto :fail
call "%~dp0..\windows\_python-env.cmd"
if errorlevel 1 goto :missing_python

if exist "%FRONTEND_BUILD%" rmdir /s /q "%FRONTEND_BUILD%"
if exist "%WEB_RELEASE%" rmdir /s /q "%WEB_RELEASE%"
if exist "%CD%\target" rmdir /s /q "%CD%\target"
if exist "%CD%\crates\ghost-wasm\dist" rmdir /s /q "%CD%\crates\ghost-wasm\dist"

echo Ghost Talk - Web release build
pushd crates\ghost-wasm || goto :fail
trunk build --release
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" goto :done

"%GHOST_TALK_PYTHON_EXE%" %GHOST_TALK_PYTHON_ARGS% "%CD%\scripts\artifacts\stage.py" --platform web --profile release
set "RC=%ERRORLEVEL%"
if not "%RC%"=="0" goto :done

echo.
echo Ghost Talk Web release build complete.
echo Final files: %WEB_RELEASE%
goto :done

:missing_python
echo ERROR: Python 3 is required to stage the web release.
set "RC=1"
goto :done

:fail
set "RC=%ERRORLEVEL%"
if "%RC%"=="0" set "RC=1"
:done
if not "%RC%"=="0" echo Web release build FAILED with exit code %RC%.
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %RC%
