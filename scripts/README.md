# Ghost Talk scripts

Platform automation is intentionally separated so a build host has one obvious folder.

```text
scripts/
├── windows/
│   ├── bootstrap.cmd
│   ├── build.cmd
│   └── run.cmd
├── linux/
│   ├── _common.sh
│   ├── bootstrap.sh
│   ├── build.sh
│   └── run.sh
├── android/
│   ├── build.cmd
│   ├── build.sh
│   ├── run.cmd
│   └── run.sh
├── ios/
│   ├── build.command
│   ├── build.sh
│   ├── run.command
│   └── run.sh
├── wasm/
│   ├── build.cmd
│   ├── build.sh
│   ├── run.cmd
│   └── run.sh
├── release/
│   └── package-flat-zip.py
├── run-all-tests.cmd
└── run-all-tests.sh
```

## Double-click behavior

- Windows uses `.cmd` only for Ghost Talk first-party Windows entrypoints. No first-party `.ps1` launcher is used.
- Devuan/Linux `.sh` launchers detect graphical invocation and reopen in an available terminal emulator.
- iOS provides `.command` wrappers because that is the normal double-clickable Terminal script format on macOS.
- Android and WASM provide both Windows `.cmd` and Unix `.sh` host entrypoints.

## Complete tests

`run-all-tests.cmd` and `run-all-tests.sh` invoke the same `qa/run-all-tests.py`. The order is:

1. Ghost Talk static architecture/security gates.
2. First-party Rust format/check/clippy/unit/integration/doc tests and upstream-feature compile gates.
3. Every standalone example package.
4. Every declared frontend test plus the production frontend build.
5. Kaspa Portal's complete QA/E2E suite.
6. HYDRA release validation through mutation testing.
7. Final pre-fuzz repository hygiene.
8. **Fuzzing last:** every Kaspa Portal cargo-fuzz target, then HYDRA deep fuzz.

The exhaustive runner treats missing required toolchains as failures. It does not silently downgrade the test plan. Portal's own funded-network safety policy remains authoritative for its funded E2E stages.

Resume at any numbered main boundary with `--from`: `static`, `rust`, `examples`, `frontend`, `portal`, `hydra`, `hygiene`, or `fuzz`. Both wrappers forward arguments, for example `scripts\run-all-tests.cmd --from examples` on Windows or `scripts/run-all-tests.sh --from examples` on Unix. `--from` means “start here and continue through all later sections”; it does not re-run earlier sections.
