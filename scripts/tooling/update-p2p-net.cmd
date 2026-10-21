@echo off
rem Re-resolve p2p-net to the newest commit of its default branch in every
rem workspace lockfile, so the suites always build against the current main.
setlocal EnableExtensions
set "ROOT=%~dp0..\.."
set "APP=%ROOT%\examples\ghost-talk-app"
for %%M in ("%ROOT%\Cargo.toml" "%APP%\Cargo.toml" "%APP%\crates\ghost-wasm\Cargo.toml" "%APP%\e2e\harness\Cargo.toml") do (
  echo ==^> cargo update -p p2p-net --manifest-path %%~M
  cargo update -p p2p-net --manifest-path "%%~M" || exit /b 1
)
exit /b 0
