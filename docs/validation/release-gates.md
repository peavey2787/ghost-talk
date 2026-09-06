# Release gates

Mandatory before production-tagged release: rustfmt, clippy -D warnings, workspace tests, full upstream feature checks, TypeScript build, architecture QA, fuzzing of protocol/carrier/GTCD/voice parsers, mutation thresholds, supply-chain/license audit, protocol vectors, multi-peer offline/reorg E2E, Devuan/Windows/Android/iOS/WASM build matrix, signed artifacts, SBOM and independent security review.

This source package is a development repository, not a production-readiness certification.
