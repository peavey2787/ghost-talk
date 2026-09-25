@echo off
setlocal EnableExtensions
cd /d "%~dp0\..\.."
set "ROOT=%CD%"
if not "%~1"=="" (echo ERROR: run-release-gates accepts no filters or arguments. & exit /b 2)
where py >nul 2>&1
if not errorlevel 1 (set "PYTHON=py -3") else (
  where python >nul 2>&1 || (echo ERROR: Python 3 is required. & exit /b 1)
  set "PYTHON=python"
)
where cargo >nul 2>&1 || (echo ERROR: Cargo is required. & exit /b 1)
cargo deny --version >nul 2>&1 || (echo ERROR: cargo-deny is required: cargo install cargo-deny --locked & exit /b 1)
cargo audit --version >nul 2>&1 || (echo ERROR: cargo-audit is required: cargo install cargo-audit --locked & exit /b 1)

%PYTHON% scripts\release\check-release-inputs.py --locks-only || exit /b 1
set "GHOST_TALK_NO_PAUSE=1"
call scripts\run-all-tests.cmd || exit /b 1

set "APP_MANIFEST=%ROOT%\examples\ghost-talk-app\Cargo.toml"
set "WASM_MANIFEST=%ROOT%\examples\ghost-talk-app\crates\ghost-wasm\Cargo.toml"
set "APP_LOCK=%ROOT%\examples\ghost-talk-app\Cargo.lock"
set "WASM_LOCK=%ROOT%\examples\ghost-talk-app\crates\ghost-wasm\Cargo.lock"
set "EVIDENCE=%ROOT%\target\release-evidence"
if exist "%EVIDENCE%" rmdir /s /q "%EVIDENCE%"
mkdir "%EVIDENCE%" || exit /b 1

cargo deny check --manifest-path "%APP_MANIFEST%" || exit /b 1
cargo deny check --manifest-path "%WASM_MANIFEST%" || exit /b 1
cargo audit --file "%APP_LOCK%" || exit /b 1
cargo audit --file "%WASM_LOCK%" || exit /b 1
cargo metadata --manifest-path "%APP_MANIFEST%" --locked --format-version 1 > "%EVIDENCE%\app-metadata.json" || exit /b 1
cargo metadata --manifest-path "%WASM_MANIFEST%" --locked --format-version 1 > "%EVIDENCE%\wasm-metadata.json" || exit /b 1
%PYTHON% scripts\release\generate-sbom.py "%EVIDENCE%\app-metadata.json" "%EVIDENCE%\wasm-metadata.json" --output "%EVIDENCE%\ghost-talk.cdx.json" || exit /b 1

if "%SOURCE_DATE_EPOCH%"=="" set "SOURCE_DATE_EPOCH=1"
set "RUSTFLAGS=%RUSTFLAGS% --remap-path-prefix=%ROOT%=/workspace"
set "A=%EVIDENCE%\build-a"
set "B=%EVIDENCE%\build-b"
for %%T in ("%A%" "%B%") do (
  cargo build --manifest-path "%APP_MANIFEST%" -p ghost-talk-native --release --locked --target-dir "%%~T\app" || exit /b 1
  cargo build --manifest-path "%WASM_MANIFEST%" --target wasm32-unknown-unknown --release --locked --target-dir "%%~T\wasm" || exit /b 1
)
%PYTHON% scripts\release\hash-artifacts.py --pair ghost-talk-native "%A%\app\release\ghost-talk-native.exe" "%B%\app\release\ghost-talk-native.exe" --pair ghost-wasm.wasm "%A%\wasm\wasm32-unknown-unknown\release\ghost_wasm.wasm" "%B%\wasm\wasm32-unknown-unknown\release\ghost_wasm.wasm" --manifest "%EVIDENCE%\SHA256SUMS" || exit /b 1

cargo package -p ghost-talk --allow-dirty || exit /b 1
set /p REVISION=<REVISION
set "SOURCE_ZIP=%EVIDENCE%\ghost-talk-%REVISION%-source.zip"
%PYTHON% scripts\release\package-source.py --output "%SOURCE_ZIP%" || exit /b 1
%PYTHON% scripts\release\check-source-package.py "%SOURCE_ZIP%" || exit /b 1
%PYTHON% scripts\release\check-release-inputs.py || exit /b 1
echo.
echo COMMERCIAL RELEASE GATES PASSED.
exit /b 0
