# Release qualification

A green development test run is necessary but not sufficient for a commercial release. Release qualification is fail-closed and separates reproducible software evidence from platform/device evidence that cannot be inferred from unit tests.

## 1. Dependency locks

The reference application and its standalone `ghost-wasm` workspace must commit their generated lockfiles:

- `examples/ghost-talk-app/Cargo.lock`
- `examples/ghost-talk-app/crates/ghost-wasm/Cargo.lock`

Generate them with the pinned toolchain (`cargo generate-lockfile` in each workspace) and review all dependency changes. Never hand-author a Cargo lockfile.

## 2. Automated software gates

Run the complete repository quality suite first. A release candidate then runs `scripts/release/run-release-gates.sh` (Linux) or `scripts\release\run-release-gates.cmd` (Windows). The release gate additionally performs locked dependency resolution, `cargo deny`, `cargo audit`, locked release builds, CycloneDX SBOM generation, reproducibility comparison, artifact hashing, and external qualification-evidence validation.

## 3. External qualification evidence

Copy `docs/release/release-evidence.example.json` to a release-controlled location, replace every `pending` status with verified `pass` evidence, and point `GHOST_TALK_RELEASE_EVIDENCE` at that file. Each record must identify the tested revision, platform/environment, evidence location, and date. The gate rejects missing, pending, skipped, or failed requirements.

The required evidence covers funded multi-instance/restart/reorg/failure E2E, production keystore/biometric behavior, group cryptographic lifecycle, device/audio HIL, supported-platform hardening, interoperability, production background/notification behavior, and UI/platform parity.

## 4. Signing and publication

The release gate creates SHA-256 manifests under `target/release-evidence`. Signing keys must never live in the repository. Sign the final release manifest with the organization's protected signing system and retain its detached signature with the release evidence. A release is not qualified until the signing evidence entry is `pass`.

Do not describe a revision as commercially qualified until all automated gates and all required external evidence pass against that exact revision.
