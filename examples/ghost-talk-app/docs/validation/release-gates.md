# Validation scope

## Authoritative full-suite entry point

Run `scripts/run-all-tests.sh` at the repository root on Linux/macOS-like shells or `scripts\run-all-tests.cmd` at the repository root on Windows. These wrappers accept **no test filter arguments**. A missing prerequisite, omitted test surface, empty coverage report, unmeasured critical function, or CRAP violation is a hard failure.

The protected matrix is:

```text
root SDK workspace
  scripts/check-test-matrix.py
  scripts/check-architecture.py
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo clippy --workspace --target wasm32-unknown-unknown --all-targets -- -D warnings
  cargo clippy --workspace --target wasm32-unknown-unknown --all-targets --all-features -- -D warnings
  cargo test --workspace --all-targets --no-fail-fast
  cargo test --workspace --all-targets --all-features --no-fail-fast
  cargo test --workspace --doc --no-fail-fast
  cargo test --workspace --doc --all-features --no-fail-fast
  wasm-pack test --headless --firefox|--chrome crates/ghost-talk-wasm
  cargo llvm-cov --workspace --all-targets --no-fail-fast --lcov
  cargo llvm-cov --workspace --all-targets --all-features --no-fail-fast --lcov

reference application workspace
  scripts/check-architecture.py
  scripts/check-test-matrix.py
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-targets --no-fail-fast
  cargo test --workspace --all-targets --all-features --no-fail-fast
  cargo test --workspace --doc --no-fail-fast
  cargo test --workspace --doc --all-features --no-fail-fast
  cargo llvm-cov --workspace --all-targets --no-fail-fast --lcov
  cargo llvm-cov --workspace --all-targets --all-features --no-fail-fast --lcov

standalone browser application crate (excluded intentionally from the host workspace)
  cargo fmt --manifest-path crates/ghost-wasm/Cargo.toml -- --check
  cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --all-targets -- -D warnings
  cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features -- -D warnings
  cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings
  cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets --all-features -- -D warnings
  cargo test --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --no-fail-fast
  cargo test --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features --no-fail-fast
  cargo test --manifest-path crates/ghost-wasm/Cargo.toml --doc --no-fail-fast
  cargo test --manifest-path crates/ghost-wasm/Cargo.toml --doc --all-features --no-fail-fast
  wasm-pack test --headless --firefox|--chrome crates/ghost-wasm
  cargo llvm-cov --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --no-fail-fast --lcov
  cargo llvm-cov --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features --no-fail-fast --lcov

coverage/CRAP
  root-default.lcov
  root-all-features.lcov
  workspace-default.lcov
  workspace-all-features.lcov
  ghost-wasm-default.lcov
  ghost-wasm-all-features.lcov
  scripts/check-crap.py <all six reports> --max-crap 25 \
    --report target/coverage/crap-report.tsv \
    --unmeasured-report target/coverage/crap-unmeasured.tsv
```

`--all-targets` is deliberate: unit tests, integration-test targets, binaries/examples with test harnesses, and other Cargo test targets must not be hidden behind a `--lib`-only run. Native workspaces execute both default-feature and all-feature configurations so positive and negative feature gates cannot silently hide tests. Doctests are invoked explicitly in both configurations because they are not included by `--all-targets`. `--no-fail-fast` asks Cargo to continue through the target matrix before returning failure so one failing target does not conceal later failures. The browser harness is separate because browser-only `wasm_bindgen_test` tests must execute in an actual supported browser.

The runners require Python 3 and `wasm-pack`, and Firefox or Chrome must be available for the browser harness. On Windows, the application runner honors `rust-toolchain.toml` even when the host Rust came from a standalone MSI: if global rustup is unavailable it bootstraps a checksum-verified isolated rustup environment under the user profile, then installs the pinned compiler, `rustfmt`, `clippy`, `llvm-tools-preview`, and `wasm32-unknown-unknown` there without replacing the system Rust installation. It also installs pinned `cargo-llvm-cov 0.8.7` under the Ghost Talk QA-tools directory when missing. Other runners require rustup when a declared cross-target is missing and require `cargo-llvm-cov` to be installed. Tool, bootstrap, component, target, or QA-tool installation failures fail closed.

## Architecture/SRP gate

`scripts/check-architecture.py` is a thin facade over focused `scripts/architecture/` checks. It scans the entire first-party production Rust tree and also enforces SRP/naming limits on first-party QA scripts. It rejects:

- production Rust files at or above the 300-line normal review threshold (350 remains the absolute safety ceiling), first-party QA scripts at or above 300 lines, and functions above 75 lines;
- structural complexity above 8, plus unreviewed functions at/above 50 lines or above CC 6;
- textual `include!` composition, numeric-order source fragments, compound `_and_` source modules, and crowded source directories;
- wildcard sibling imports/re-exports and stale module/export surfaces;
- duplicate shared domain/API type declarations across first-party crates;
- component-level durable-state orchestration or persistence-patch construction;
- public mutable Chat/Room/contact identity/lifecycle fields that bypass their authoritative owners;
- direct chat/contact/room collection mutation outside their domain services;
- whole-profile `ProfilePatch` snapshots;
- direct wallet mutation outside `WalletStateService` and the narrow profile reconciler;
- HYDRA chat-session projection changes outside `HydraSessionManager`;
- Kaspa gateway network/endpoint ownership that bypasses `NodeSession`;
- live-call persistence or direct call-phase mutation outside `CallManager`;
- one-sided authenticated-contact matching instead of `PeerBinding` + `ContactService`;
- durable mailbox frame/envelope mutation outside `MailboxService`, or drains that do not isolate envelope failures with `PacketDisposition`;
- obsolete/legacy/deprecated production implementations, unexplained dead APIs, and dead/unused-code suppressions;
- storage paths that bypass the Tauri application-data root;
- HYDRA opens that bypass the lifetime profile lease;
- first-party package semantic versions other than `0.1.0`.

The normal design targets are `<300` lines per production file, `<50` lines per function, and CC<=6. The hard function/complexity ceilings remain 75/8; every normal-target crossing must have a current, self-validating SRP review record.

## CRAP and LCOV gate

CRAP is calculated from measured LCOV line coverage and structural CC using:

```text
CRAP(m) = CC(m)^2 * (1 - coverage(m))^3 + CC(m)
```

The combined reporter consumes **all six mandatory first-party LCOV reports**:

1. repository root SDK default features: `target/coverage/root-default.lcov`;
2. repository root SDK all features: `target/coverage/root-all-features.lcov`;
3. reference-application default features: `examples/ghost-talk-app/target/coverage/workspace-default.lcov`;
4. reference-application all features: `examples/ghost-talk-app/target/coverage/workspace-all-features.lcov`;
5. standalone browser crate default features: `examples/ghost-talk-app/target/coverage/ghost-wasm-default.lcov`;
6. standalone browser crate all features: `examples/ghost-talk-app/target/coverage/ghost-wasm-all-features.lcov`.

The default/all-feature pairs are all required so code compiled only under `cfg(not(feature = ...))` or `cfg(feature = ...)` contributes real LCOV rather than disappearing from the report.

Every first-party production function appears in `crap-report.tsv` as measured, violation, unmeasured, or critical-unmeasured. Every unmeasured production function is also written to `crap-unmeasured.tsv`. The hard measured threshold is CRAP<=25. Ownership/security-critical host-executable functions—including core domain/protocol code, mailbox routing, wallet reconciliation/progress, transaction fee replan, and profile patch reconciliation—must have executable LCOV measurement; an unmeasured critical function fails the suite.

Browser-only code cannot receive meaningful host LLVM line coverage from `cargo llvm-cov`; it remains explicitly unmeasured and is exercised through the mandatory `wasm-pack` browser test surface. No synthetic coverage is assigned. A release must never claim a CRAP/coverage result unless all LCOV commands and `check-crap.py` completed successfully.

## Test-matrix self-check

`scripts/check-test-matrix.py` audits both shell and Windows runners and inventories every first-party `Cargo.toml` and Rust test attribute. It fails if a new manifest is not covered by a mandatory workspace/standalone surface, if a manifest disables tests/doctests, or if default/all-feature all-target tests, both doctest configurations, default/all-feature native/wasm32 Clippy, either real-browser WASM surface, any of the six LCOV reports, CRAP<=25, or the unmeasured-function report disappear. It also rejects ignored Rust tests, `--lib` narrowing, and external test-filter arguments in the full-suite runners.

## Automated interaction regressions

The workspace includes a deterministic two-instance ownership regression covering saved-contact B versus unrelated fresh wallet C, bidirectional messages, two call cycles, mute/unmute, both hang-up directions, leave/recreate, serialization restart, phantom-call absence, preserved contact identity, and post-restart delivery. Browser/WASM tests render the actual call/chat controls and verify exact IDs, incoming notification identity, start-chat targets, message bodies, local-first leave, accept/decline, mute/unmute, and local-first hang-up.

These deterministic tests do not replace the real-network process-level scenarios below.

## Required two-client regressions

Before a production release, exercise the real Kaspa/HYDRA path with two independent client processes. At minimum verify:

1. Client A already has an unrelated saved contact B.
2. Completely different wallet C sends A a new chat request.
3. A displays C as the authenticated unknown peer rather than B.
4. A accepts C and messages are delivered in both directions.
5. Either peer starts a voice call and both reach `Connected`.
6. Mute stops local outbound microphone media; unmute resumes it.
7. A hangs up and both peers leave the call.
8. A second call is established and C hangs up successfully.
9. Both leave/re-enter chat and can start a fresh session without restarting.
10. Both applications restart: durable contacts/chats remain correct and no active call is resurrected.
11. A stale/rejected mailbox envelope cannot prevent a later valid chat/call request from surfacing.
12. A spent-input rejection refreshes/replans safely, while an already-accepted broadcast followed by observer lag is not reported as a failed send.

Also retain call-specific scenarios for decline, caller cancel, unanswered timeout, secure-transport timeout, duplicate/replayed ring, restart with a pending ring, simultaneous/busy calls, and operation at the final prederived change address.

Formatting, Clippy, unit tests, WASM-target checks, architecture checks, coverage/CRAP, and two-client runtime scenarios validate different layers and must not be represented as substitutes for one another.
