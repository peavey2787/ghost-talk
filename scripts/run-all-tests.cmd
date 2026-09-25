@echo off
setlocal EnableExtensions
cd /d "%~dp0\.."
set "EXIT_CODE=1"
set "ROOT_NO_PAUSE=%GHOST_TALK_NO_PAUSE%"
if not "%~1"=="" (echo ERROR: run-all-tests does not accept filters or extra arguments; the complete test matrix is mandatory. & set "EXIT_CODE=2" & goto :report)

call :select_python
if errorlevel 1 (echo ERROR: A runnable Python 3 interpreter is required for architecture/test-matrix/CRAP checks. & goto :done)
call examples\ghost-talk-app\scripts\windows\_rust-env.cmd
if errorlevel 1 (echo ERROR: A runnable Cargo executable is required. & goto :done)
echo Cargo executable: %GHOST_TALK_CARGO_EXE%
if defined GHOST_TALK_RUSTUP_EXE (echo rustup executable: %GHOST_TALK_RUSTUP_EXE%) else (echo rustup executable: not found ^(only required if the WASM target must be installed^))

if "%GHOST_TALK_WASM_BROWSER%"=="" set "GHOST_TALK_WASM_BROWSER=firefox"
if /I not "%GHOST_TALK_WASM_BROWSER%"=="firefox" if /I not "%GHOST_TALK_WASM_BROWSER%"=="chrome" (echo ERROR: GHOST_TALK_WASM_BROWSER must be firefox or chrome. & goto :done)

echo Ghost Talk - COMPLETE repository quality gates
"%PYTHON_EXE%" %PYTHON_ARGS% examples\ghost-talk-app\scripts\check-test-matrix.py
if errorlevel 1 goto :done
"%PYTHON_EXE%" %PYTHON_ARGS% examples\ghost-talk-app\scripts\check-architecture.py
if errorlevel 1 goto :done

cargo fmt --all -- --check
if errorlevel 1 goto :done
rem Native compilation must succeed before optional WASM target provisioning.
cargo check --workspace --all-targets
if errorlevel 1 goto :done
cargo check --workspace --all-targets --all-features
if errorlevel 1 goto :done
call scripts\tooling\ensure-wasm-target.cmd
if errorlevel 1 goto :done
cargo llvm-cov --version >nul 2>&1
if errorlevel 1 (echo ERROR: cargo-llvm-cov is required for repository-wide LCOV/CRAP coverage. Install it with: cargo install cargo-llvm-cov & goto :done)
where wasm-pack >nul 2>&1
if errorlevel 1 (echo ERROR: wasm-pack is required for real-browser SDK/application WASM tests. & goto :done)
cargo clippy --workspace --all-targets -- -D warnings
if errorlevel 1 goto :done
cargo clippy --workspace --all-targets --all-features -- -D warnings
if errorlevel 1 goto :done
cargo clippy --workspace --target wasm32-unknown-unknown --all-targets -- -D warnings
if errorlevel 1 goto :done
cargo clippy --workspace --target wasm32-unknown-unknown --all-targets --all-features -- -D warnings
if errorlevel 1 goto :done

cargo test --workspace --all-targets --no-fail-fast
if errorlevel 1 goto :done
cargo test --workspace --all-targets --all-features --no-fail-fast
if errorlevel 1 goto :done
cargo test --workspace --doc --no-fail-fast
if errorlevel 1 goto :done
cargo test --workspace --doc --all-features --no-fail-fast
if errorlevel 1 goto :done

if /I "%GHOST_TALK_WASM_BROWSER%"=="firefox" (
  wasm-pack test --headless --firefox crates\ghost-talk-wasm
) else (
  wasm-pack test --headless --chrome crates\ghost-talk-wasm
)
if errorlevel 1 goto :done

if not exist target\coverage mkdir target\coverage
set "GHOST_TALK_ROOT_LCOV_DEFAULT=%CD%\target\coverage\root-default.lcov"
set "GHOST_TALK_ROOT_LCOV_ALL_FEATURES=%CD%\target\coverage\root-all-features.lcov"
cargo llvm-cov --workspace --all-targets --no-fail-fast --lcov --output-path "%GHOST_TALK_ROOT_LCOV_DEFAULT%"
if errorlevel 1 goto :done
cargo llvm-cov --workspace --all-targets --all-features --no-fail-fast --lcov --output-path "%GHOST_TALK_ROOT_LCOV_ALL_FEATURES%"
if errorlevel 1 goto :done
if not exist "%GHOST_TALK_ROOT_LCOV_DEFAULT%" (echo ERROR: required LCOV report is missing: %GHOST_TALK_ROOT_LCOV_DEFAULT% & goto :done)
if not exist "%GHOST_TALK_ROOT_LCOV_ALL_FEATURES%" (echo ERROR: required LCOV report is missing: %GHOST_TALK_ROOT_LCOV_ALL_FEATURES% & goto :done)
for %%I in ("%GHOST_TALK_ROOT_LCOV_DEFAULT%" "%GHOST_TALK_ROOT_LCOV_ALL_FEATURES%") do if %%~zI EQU 0 (echo ERROR: required LCOV report is empty: %%~fI & goto :done)

echo.
echo Ghost Talk - reference application COMPLETE quality gates
set "GHOST_TALK_NO_PAUSE=1"
call examples\ghost-talk-app\scripts\run-all-tests.cmd
set "APP_EXIT=%ERRORLEVEL%"
set "GHOST_TALK_NO_PAUSE=%ROOT_NO_PAUSE%"
if not "%APP_EXIT%"=="0" (set "EXIT_CODE=%APP_EXIT%" & goto :report)

set "EXIT_CODE=0"
goto :report

:select_python
call examples\ghost-talk-app\scripts\windows\_python-env.cmd
if errorlevel 1 exit /b 1
set "PYTHON_EXE=%GHOST_TALK_PYTHON_EXE%"
set "PYTHON_ARGS=%GHOST_TALK_PYTHON_ARGS%"
exit /b 0

:done
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%"=="0" set "EXIT_CODE=1"

:report
echo.
if "%EXIT_CODE%"=="0" (echo ALL QUALITY GATES PASSED. Every configured test surface, every LCOV feature surface, and the combined CRAP gate completed.) else (echo QUALITY GATES FAILED with exit code %EXIT_CODE%.)
if /I not "%GHOST_TALK_NO_PAUSE%"=="1" pause
endlocal & exit /b %EXIT_CODE%
