@echo off
setlocal EnableExtensions
cd /d "%~dp0\.."
set "EXIT_CODE=1"
if not "%~1"=="" (echo ERROR: run-all-tests does not accept filters or extra arguments; the complete test matrix is mandatory. & set "EXIT_CODE=2" & goto :report)

rem Resolve Python before VsDevCmd/MSVC initialization. Visual Studio environment setup can
rem rewrite PATH; once resolved, keep the exact interpreter path for the whole QA run.
call scripts\windows\_python-env.cmd
if errorlevel 1 (
  echo ERROR: A runnable Python 3 interpreter is required for the Windows quality-gate orchestrator.
  echo ERROR: Checked inherited override, python/py/python3, registered CPython, WindowsApps, standard CPython, MSYS2, and Scoop locations.
  set "EXIT_CODE=127"
  goto :report
)

echo Python 3 interpreter: %GHOST_TALK_PYTHON_EXE% %GHOST_TALK_PYTHON_ARGS%
call scripts\windows\_rust-env.cmd
if errorlevel 1 (
  echo ERROR: A runnable Cargo executable is required for the Windows quality gates.
  set "EXIT_CODE=127"
  goto :report
)
echo Cargo executable: %GHOST_TALK_CARGO_EXE%
if defined GHOST_TALK_RUSTUP_EXE (echo rustup executable: %GHOST_TALK_RUSTUP_EXE%) else (echo rustup executable: not found ^(only required if the WASM target must be installed^))
call scripts\windows\_msvc-env.cmd
if errorlevel 1 goto :done
set "PATH=%GHOST_TALK_CARGO_BIN%;%PATH%"

"%GHOST_TALK_PYTHON_EXE%" %GHOST_TALK_PYTHON_ARGS% scripts\run-all-tests-windows.py
set "EXIT_CODE=%ERRORLEVEL%"
goto :report

:done
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%"=="0" set "EXIT_CODE=1"

:report
echo.
if "%EXIT_CODE%"=="0" (echo ALL QUALITY GATES PASSED. Default/all-feature unit/integration/all-target/doctest/browser tests, every LCOV feature surface, and CRAP completed.) else (echo QUALITY GATES FAILED with exit code %EXIT_CODE%.)
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %EXIT_CODE%
