# Contributing to Ghost Talk

Ghost Talk keeps subsystem ownership explicit and treats the repository quality gates as part of the architecture contract.

## Development requirements

Use the Rust toolchain pinned by `rust-toolchain.toml`. Keep first-party package versions at `0.1.0` during this development line and increment `REVISION` for repository changes. Do not commit credentials, signing keys, local profile data, generated targets, or IDE state.

## Architecture rules

Changes must preserve organized module/folder structure, clear ownership and separation of concerns, consistent naming, SRP, DRY code, and the repository's complexity limits. Prefer a facade only when it encapsulates a genuinely complex subsystem; expose small pure helpers or direct data types without unnecessary indirection. Production `#[path]` aliases, dead first-party crates, unused first-party path dependencies, duplicate outbound mailbox setup, ignored tests, and undocumented complexity exceptions are rejected by automated checks.

## Required validation

Run `scripts/run-all-tests.sh` on Linux or `scripts\run-all-tests.cmd` on Windows before submitting a change. The runner accepts no filters and covers native/default/all-feature compilation, Clippy, tests, doctests, browser/WASM tests, LCOV, and CRAP. Release candidates have additional requirements documented in `docs/release/RELEASE.md`.

Add or update tests for behavior changes, keep public documentation synchronized with the implementation, and explain security/compatibility implications in the change description.
