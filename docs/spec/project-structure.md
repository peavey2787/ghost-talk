# Project structure

Repository source/work directories stay limited to:

```text
crates/ docs/ qa/ examples/ external/ scripts/
```

`crates/` contains single-responsibility first-party modules and the shared Tauri/React application. `docs/` contains spec/impl/validation/project/future-work. `qa/run-all.py` is the fast/static validation gate and `qa/run-all-tests.py` is the exhaustive test orchestrator used by `scripts/run-all-tests.cmd` and `.sh`. `examples/` contains focused runnable examples. `external/` contains pinned upstream source/reference material. `scripts/` contains platform-separated environment/build/run/package automation.

Architecture invariants: SRP; CC target <=6/function; DRY; no legacy/dead copies; UI cannot import HYDRA/Portal; Kinesis reference cannot be a runtime dependency; security permissions are enforced below UI state.


## Scripts layout

```text
scripts/
├── windows/   # .cmd bootstrap/build/run
├── linux/     # Devuan .sh bootstrap/build/run
├── android/   # .cmd + .sh build/run
├── ios/       # Xcode 16.2 .sh + double-click .command wrappers
├── wasm/      # .cmd + .sh build/run
├── release/   # non-platform release packaging
├── run-all-tests.cmd
└── run-all-tests.sh
```

No first-party Windows launcher under `scripts/` may use `.ps1`. Long-running test phases are ordered by `qa/run-all-tests.py`; coverage-guided fuzzing is always the final phase.
