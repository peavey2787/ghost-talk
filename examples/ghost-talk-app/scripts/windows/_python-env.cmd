@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "PYTHON_EXE="
set "PYTHON_ARGS="
set "PYTHON_PROBE_FILE=%TEMP%\ghost-talk-python-%RANDOM%-%RANDOM%.txt"

rem Prefer an exact interpreter already resolved by a parent Ghost Talk launcher.
if defined GHOST_TALK_PYTHON_EXE (
  call :probe_file "%GHOST_TALK_PYTHON_EXE%"
  if not errorlevel 1 goto :found
)

rem Probe commands exactly as CMD resolves them. Do not require WHERE first.
call :probe_command python
if not errorlevel 1 goto :found
call :probe_command py -3
if not errorlevel 1 goto :found
call :probe_command python3
if not errorlevel 1 goto :found

rem Python Install Manager / modern per-user CPython layout.
for /d %%D in ("%LOCALAPPDATA%\Python\pythoncore-*") do (
  call :probe_file "%%~fD\python.exe"
  if not errorlevel 1 goto :found
)

rem Traditional per-user CPython installs.
for /d %%D in ("%LOCALAPPDATA%\Programs\Python\Python*") do (
  call :probe_file "%%~fD\python.exe"
  if not errorlevel 1 goto :found
)

rem App Execution Alias path. A stub is harmless because probe_file must execute it.
call :probe_file "%LOCALAPPDATA%\Microsoft\WindowsApps\python.exe"
if not errorlevel 1 goto :found
call :probe_file "%LOCALAPPDATA%\Microsoft\WindowsApps\python3.exe"
if not errorlevel 1 goto :found

rem PEP 514 registered Python installations. Registry lookup does not depend on PATH.
call :probe_registry "HKCU\Software\Python"
if not errorlevel 1 goto :found
call :probe_registry "HKLM\Software\Python"
if not errorlevel 1 goto :found
call :probe_registry "HKLM\Software\WOW6432Node\Python"
if not errorlevel 1 goto :found

rem Standard machine-wide CPython installs, regardless of minor version.
for /d %%D in ("%ProgramFiles%\Python*") do (
  call :probe_file "%%~fD\python.exe"
  if not errorlevel 1 goto :found
)
if defined ProgramFiles(x86) for /d %%D in ("%ProgramFiles(x86)%\Python*") do (
  call :probe_file "%%~fD\python.exe"
  if not errorlevel 1 goto :found
)

rem Common MSYS2 and Scoop installations used by Ghost Talk developers.
call :probe_file "C:\msys64\mingw64\bin\python3.exe"
if not errorlevel 1 goto :found
call :probe_file "C:\msys64\ucrt64\bin\python3.exe"
if not errorlevel 1 goto :found
call :probe_file "C:\msys64\clang64\bin\python3.exe"
if not errorlevel 1 goto :found
call :probe_file "C:\msys64\usr\bin\python3.exe"
if not errorlevel 1 goto :found
call :probe_file "%USERPROFILE%\scoop\apps\python\current\python.exe"
if not errorlevel 1 goto :found

del /q "%PYTHON_PROBE_FILE%" >nul 2>&1
endlocal & exit /b 1

:probe_command
set "PYTHON_EXE="
del /q "%PYTHON_PROBE_FILE%" >nul 2>&1
%~1 %~2 -c "import sys; print(sys.executable)" >"%PYTHON_PROBE_FILE%" 2>nul
if errorlevel 1 goto :probe_command_failed
set /p "PYTHON_EXE=" <"%PYTHON_PROBE_FILE%"
if not defined PYTHON_EXE goto :probe_command_failed
call :validate_exe "%PYTHON_EXE%"
if errorlevel 1 goto :probe_command_failed
set "PYTHON_ARGS="
del /q "%PYTHON_PROBE_FILE%" >nul 2>&1
exit /b 0

:probe_command_failed
set "PYTHON_EXE="
del /q "%PYTHON_PROBE_FILE%" >nul 2>&1
exit /b 1

:probe_file
if not exist "%~1" exit /b 1
call :validate_exe "%~1"
if errorlevel 1 exit /b 1
set "PYTHON_EXE=%~1"
set "PYTHON_ARGS="
exit /b 0

:probe_registry
for /f "tokens=1,2,*" %%A in ('reg query "%~1" /s /v ExecutablePath 2^>nul') do (
  if /I "%%A"=="ExecutablePath" (
    call :probe_file "%%C"
    if not errorlevel 1 exit /b 0
  )
)
exit /b 1

:validate_exe
"%~1" -c "import sys; raise SystemExit(0 if sys.version_info.major == 3 else 1)" >nul 2>&1
exit /b %ERRORLEVEL%

:found
del /q "%PYTHON_PROBE_FILE%" >nul 2>&1
endlocal & set "GHOST_TALK_PYTHON_EXE=%PYTHON_EXE%" & set "GHOST_TALK_PYTHON_ARGS=%PYTHON_ARGS%" & exit /b 0
