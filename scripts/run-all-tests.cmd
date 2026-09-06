@echo off
setlocal EnableExtensions
cd /d "%~dp0\.."
set "EXIT_CODE=1"
call scripts\windows\_msvc-env.cmd
if errorlevel 1 (
  set "EXIT_CODE=%ERRORLEVEL%"
  goto :done
)
set "PYTHON="
python -c "import sys; raise SystemExit(0 if sys.version_info >= (3,8) else 1)" >nul 2>&1 && set "PYTHON=python"
if not defined PYTHON py -3 -c "import sys; raise SystemExit(0 if sys.version_info >= (3,8) else 1)" >nul 2>&1 && set "PYTHON=py -3"
if not defined PYTHON python3 -c "import sys; raise SystemExit(0 if sys.version_info >= (3,8) else 1)" >nul 2>&1 && set "PYTHON=python3"
if not defined PYTHON (
  echo ERROR: Python 3.8 or newer is required.
  goto :done
)
echo Ghost Talk - COMPLETE test suite
%PYTHON% qa\run-all-tests.py %*
set "EXIT_CODE=%ERRORLEVEL%"
:done
echo.
if "%EXIT_CODE%"=="0" (echo ALL TESTS PASSED.) else (echo TEST SUITE FAILED with exit code %EXIT_CODE%.)
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %EXIT_CODE%
