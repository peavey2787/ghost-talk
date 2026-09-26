# Changelog

## r239

- Completed the p2p-net migration so it actually builds and runs. r238 never resolved: libp2p 0.57's browser transports require the `wasm-bindgen 0.2.108` family while Rusty-Kaspa and the old HYDRA pin required `0.2.100`. Ghost Talk now imports its dependencies cleanly into one module: Kaspa access moved entirely to Kaspa Portal 1.1.0 (live BlockAdded, wallet UtxosChanged/VirtualDaaScoreChanged streams, dot.k UTXO/covenant verification and P2SH derivation), Rusty-Kaspa crates were removed, HYDRA moved to the aligned `d90b387` pin, KasKold 2.0 is imported from upstream (`peavey2787/KasKold` branch `v2`) instead of a vendored copy (its libraries now declare a compatible wasm-bindgen range, and Ghost Talk passes the new `approve(now_unix)` signing-policy clock), and `ghost-p2p` drives p2p-net's `WasmNode` in process. Architecture guards forbid Rusty-Kaspa dependencies, any vendored `external/` tree, a second p2p module, and split p2p-net or KasKold revisions.
- Fixed defects found by the new two-instance testnet-10 E2E suite: the browser host discarded contact requests carried as signed KKTP `discovery` anchors (new chats never reached the recipient) and dropped every Kaspa-carried GTR1 realtime carrier (p2p announcements and Kaspa voice fallback), because the live-block filter and mailbox dispatcher did not recognize them. Sessions established while p2p-net is already running now announce their transport binding once per SID, bound peers are re-dialed after idle relay drops, and the browser live BlockAdded stream reconnects after a node disconnect.
- Kaspa sends now replan instead of failing when they lose a race for an input (the node's UTXO index trails the mempool, so a message racing a realtime announcement could double-spend) and when the node's fee policy asks for more than the planner's first estimate. The E2E harness consolidates fragmented instance wallets before each run.
- Added the shared SDK crates `ghost-realtime` (GTR1 framing, replay identity/window, canonical topics, Automatic/KaspaOnly route policy, SID/HYDRA/PeerId bindings, the `GhostP2pTransport` contract, and — behind the default `p2p-net` feature — `PeerDirectory`, `send_to_game_peer`, and the Ghost node policy) and `ghost-voice` (1:1 call and Room voice packet formats plus the voice route names). They are the single owners used by the application and are a drop-in for Kaspa Kinesis's `ghost-realtime`/`ghost-voice` imports; kaspa-kinesis compiles and passes its tests against them with p2p-net main.
- Added a per-profile **Text messages** carrier: Kaspa (durable, default), p2p-net preferred (Kaspa fallback), or p2p-net only. p2p-net text is sealed by the active HYDRA session, never stored on Kaspa (and excluded from Kaspa backups), costs no Kaspa fees, and is visibly marked "p2p · not stored on Kaspa".
- Added operator p2p-net relay/bootstrap settings, Room voice Join/Leave controls (Room voice was previously unreachable from the UI), carrier-level protocol-debug records, and a connected-peer check so a route dialed by the remote side reaches "P2P connected".
- Added the two-instance end-to-end suite (`scripts/run-all-e2e.cmd` / `.sh`): persistent funded dev wallet with a faucet prompt, instance top-ups, a local p2p-net relay, and Playwright scenarios for direct chat text+voice, Kaspa-signalled p2p-net text+voice, Kaspa-only text+voice, and Room text+voice on testnet-10.
- Removed dead code left by the migration (unused realtime vocabulary, the app-owned GTR1/topic copy, the app `ghost-realtime` crate, unused browser helpers and view models), fixed the r238 compile/clippy/test failures (call modal phases and classes, test imports, lint debt), formatted the whole tree with rustfmt, and split files that exceeded the SRP limits once formatted.
- Advanced Android/iOS build metadata to revision 239 while keeping semantic package versions at 0.1.0.

## r238

- Replaced the temporary application-owned browser WebRTC data-channel implementation with the shared `p2p-net` browser backend. Ghost now owns only the transport-neutral `ghost-p2p` adapter; SDP, ICE, STUN, `RTCPeerConnection`, relay selection, and physical transport policy remain inside p2p-net/libp2p.
- Added authenticated p2p transport announcements over the already-authenticated KKTP/HYDRA/Kaspa session, SID/HYDRA/PeerId binding checks before decryption, profile-scoped persistent p2p identity, p2p lifecycle event handling, and session-topic subscriptions.
- Reworked realtime delivery so each logical packet is sealed exactly once into GTR1. Auto first sends those exact bytes through p2p-net and, on failure or unavailability, publishes the same GTR1 carrier over Kaspa without advancing HYDRA again or producing a second ciphertext. Cross-carrier replay handling processes the logical packet once.
- Removed the old direct-route offer/answer protocol, SDP schema, negotiation IDs, browser peer-connection code, hard-coded STUN servers, and the associated direct-carrier state. Updated architecture guards and documentation to make reintroduction fail closed.
- Advanced Android/iOS build metadata to revision 238 while keeping semantic package versions at 0.1.0.

## r237

- Expanded `ghost-p2p` into the single backend-neutral realtime transport contract used by native and future browser p2p-net adapters, including start/bind/connect/send/subscribe/shutdown operations and transport-neutral route state.
- Added an architecture gate that confines WebRTC/SDP/STUN/ICE implementation details to the existing isolated browser migration backend; application UI no longer names WebRTC.
- Documented the exact browser p2p-net parity/deletion gate. The upstream p2p-net public surface still lacks an importable pure-browser wasm32 NodeHandle backend, so Ghost Talk does not falsely replace it with another custom WebRTC implementation.
- Advanced Android/iOS build metadata to revision 237 while keeping semantic package versions at 0.1.0.

## r236

- Froze the shared Ghost/Kinesis realtime interoperability contract before replacing transports. `ghost-protocol` now owns the exact carrier-neutral GTR1 encoder/decoder, bounded transport announcements, shared `RealtimeBodyV1` control/voice types, raw-Opus batch limits, and domain-separated session-topic derivation. Native Kaspa realtime now uses that shared GTR1 codec instead of maintaining its own header parser/builder.
- Added reusable `ghost-realtime` and `ghost-p2p` public surfaces for carrier-independent Auto/Kaspa routing, bounded cross-carrier replay keys, and fail-closed SID/HYDRA/P2P-PeerId bindings. These crates intentionally contain no libp2p/WebRTC implementation; the future p2p-net `NodeHandle` adapter has one ownership boundary and transport PeerId never becomes a Ghost identity.
- Raised the repository/application Rust MSRV and bootstrap pin from 1.95.0 to 1.98.0 to match the current p2p-net prerequisite, documented the staged migration accurately, and advanced Android/iOS build metadata to revision 236 while keeping semantic package versions at 0.1.0. The legacy application-owned WebRTC call path is still present and is explicitly not considered final P2P parity.

## r235

- Fixed Android release artifact delivery after successful Gradle/Tauri packaging. Android wrappers now run `tauri android build --ci` so packaging is non-interactive, stage the finished APK/AAB into the expected repository-level `target/android/<profile>/` folder, keep `target/dist/android/<profile>/` as a compatibility mirror, and verify that both folders contain both package types before reporting success. Updated Android build documentation/architecture guards and advanced Android/iOS build metadata to revision 235; semantic package versions remain 0.1.0.

## r234

- Fixed the Android native build exposed after the browser/WASM path became buildable: moved stable Ghost Talk receive-address selection into one shared wallet helper used by both public-profile publishing and permanent Kaspa media archives, added regression tests for its first-address/error contract, and removed the stale `Serialize` re-export that produced an unused-import warning. Advanced Android/iOS build metadata to revision 234; semantic package versions remain 0.1.0.

## r233

- Repaired the standalone browser-host source integration exposed after the WASM dependency graph became buildable: widened nested support/runtime/interop helpers only to the `browser_host` boundary, restored the missing shared API re-exports, completed browser Kasia history/handshake decryption adapters, reused the shared profile-backup validator, restored remembered-unlock credential key derivation, fixed typed mailbox contact-request dispatch, corrected the Kasia debug module path and media-reference borrowing, and exposed only the shared JavaScript error adapter needed by the broadcast controller. Added regression coverage for these cross-module contracts and advanced Android/iOS build metadata to revision 233; semantic package versions remain 0.1.0.

## r232

- Replaced the incompatible Yew 0.21 / implicit-clone 0.4 resolver workaround with the supported Yew 0.22 dependency family. The standalone WASM frontend now pins Yew 0.22.0, implicit-clone 0.6.0, and indexmap 2.14.2 so the UI stack and Rusty-Kaspa share one compatible indexmap 2.x type identity. Migrated function-component attributes to Yew 0.22's `#[component]` spelling, refreshed the standalone WASM lock graph, added a regression invariant for the compatible family, and advanced Android/iOS build metadata to revision 232; semantic package versions remain 0.1.0.

## r231

- Corrected the Yew 0.21 / implicit-clone 0.4.9 WASM compatibility pin. `implicit-clone 0.4.9` declares `indexmap = ">= 1, <= 2"`, whose Cargo semver upper bound is exactly 2.0.0; r230 incorrectly pinned 2.14.2. The standalone WASM workspace now pins `indexmap = "=2.0.0"`, and its lock binds both Yew and implicit-clone to indexmap 2.0.0 while retaining newer indexmap 2.x packages for unrelated dependencies that require them. Added a regression invariant for the exact shared bound and advanced Android/iOS build metadata to revision 231; semantic package versions remain 0.1.0.

## r230

- Fixed the Yew 0.21 standalone WASM `IndexMap` type split exposed after the r229 ring fix. `implicit-clone 0.4.9` accepts either indexmap 1.x or 2.x, while Yew 0.21 requires indexmap 2.x; because the wider Ghost Talk graph legitimately also contains indexmap 1.9.x, Cargo could bind `implicit-clone::IMap` to 1.x and Yew's `IndexMap` conversion to 2.x. The WASM workspace now pins explicit compatibility anchors for `implicit-clone 0.4.9` and `indexmap 2.14.2`, and the committed lock binds both Yew and implicit-clone to the same indexmap 2.14.2 package. Added a lock-graph regression guard and advanced Android/iOS build metadata to revision 230; semantic package versions remain 0.1.0.

## r229

- Fixed the `rustls 0.23` / `ring 0.17` standalone Web compile failure exposed after the r228 WASM C-toolchain fix. The pinned Rusty-Kaspa wRPC client selects rustls with the ring provider, while Rusty-Kaspa's WASM wrapper separately enables ring's `wasm32_unknown_unknown_js` feature; Ghost Talk consumes the client directly, so that browser entropy feature was previously absent. The standalone WASM manifest now mirrors the upstream WASM requirement, allowing `ring::SystemRandom` to implement `SecureRandom` through Web Crypto without changing native crypto providers. Added an architecture regression guard and advanced Android/iOS build metadata to revision 229; semantic package versions remain 0.1.0.

## r228

- Fixed the next standalone `wasm32-unknown-unknown` build prerequisite exposed after the r227 entropy fix: Rusty-Kaspa's Web dependency graph includes `secp256k1 0.29.x`, whose `secp256k1-sys` crate compiles bundled C and therefore requires a WebAssembly-capable Clang/LLVM archiver on Windows.
- Added a shared Windows WASM C-toolchain bootstrap that validates `clang --target=wasm32-unknown-unknown`, requires the matching `llvm-ar`, installs the `LLVM.LLVM` WinGet package when necessary, and binds only the target-specific `CC_wasm32_unknown_unknown` / `AR_wasm32_unknown_unknown` variables. Android frontend packaging and normal Windows/Web build entry points now use the same contract without changing the later Android NDK compiler selection. Added architecture regressions and advanced Android/iOS build metadata to revision 228; semantic package versions remain 0.1.0.

## r227

- Fixed the standalone `wasm32-unknown-unknown` frontend build used by Web and Android packaging when transitive dependencies introduced `getrandom` 0.3 alongside the existing 0.2 generation. The WASM manifest now explicitly enables the JavaScript/Web Crypto backend for every active `getrandom` generation (0.2 `js`, 0.3/0.4 `wasm_js`) because Cargo features unify only within the same package version.
- Added an architecture regression guard for all three WASM entropy contracts and refreshed the standalone frontend lockfile without changing the pinned dependency versions. Advanced Android/iOS build metadata to revision 227; semantic package versions remain 0.1.0.

## r226

- Completed the standalone Web command surface against the native Tauri host: all 68 native commands now have concrete browser implementations, and QA forbids the old “not yet available in the standalone Web release” feature-gap fallback. Shared owners now cover KasKold compatibility, wallet-history classification, Kaspa archive planning/progress, HYDRA mailbox/session cryptography, and broadcast relay framing so host adapters stay thin and DRY.
- Added browser-safe live broadcasting parity: Web and mobile use the versioned Ghost Talk WSS relay for RTMP/RTMPS forwarding while desktop may use direct RTMP; local recordings remain available for verified Kaspa archive publication.
- Hardened Android packaging: release/debug builds explicitly request both APK and AAB, purge stale package outputs, discover Tauri/Gradle packages across supported build roots, stage only into `target/dist/android/<profile>`, and fail unless both package types are present there—even after the elevated Windows/UAC child exits successfully.
- Kept Web Kaspa parity live: persistent wRPC health/reconnect, BlockAdded scanning, directory/mailbox fan-out, funded-address projection from actual UTXOs, peer resolution, contact/session handshake/control/recovery/delivery flows, wallet history/send/consolidation, media/archive, Kasia, profile backup, diagnostics, and KasKold helpers. Advanced Android/iOS build metadata to revision 226; semantic package versions remain 0.1.0.

## r225

- Fixed standalone Web wallet accounting so `Funded addresses` contains only derived addresses that actually own live UTXOs. The previous browser snapshot incorrectly exposed all 128 scanned receive/change addresses as funded, even for an empty wallet. Removed the stale native `WalletPublic` import warning.
- Added a real standalone-Web BlockAdded stream using the pinned Rusty Kaspa wRPC client. Native and browser transports now feed the same shared Ghost carrier predicate/event-ID semantics; browser live observations update the shared directory/mailbox event contracts instead of merely maintaining a balance RPC connection. Mailbox-only live events no longer clear a valid wallet snapshot.
- Implemented browser `resolve_ghost_peer` / profile lookup from the live Ghost directory, plus browser contact-request submission and GTCR/KKTP discovery-envelope receive handling. This closes the initial Web start-chat/incoming-request gap while keeping transport-specific block acquisition separate from shared Ghost protocol processing.
- Added architecture regressions requiring shared live-carrier ownership, funded-only Web address projection, Web BlockAdded delivery, live directory resolution, and browser contact-request send/receive. Advanced Android/iOS build metadata to revision 225; semantic package versions remain 0.1.0.

## r224

- Fixed the standalone Web Kaspa bootstrap at its root. The browser wallet monitor is no longer a no-op and Web no longer forces network state to `disconnected` after unlock. The active browser profile now owns a persistent Kaspa Portal wRPC connection, resolves public endpoints when no explicit node is configured, verifies the connection with live DAA state, refreshes the wallet baseline, and reconnects with bounded backoff after failure. Discover publishing and the KasKold external-signer path reuse that profile connection instead of creating unrelated one-shot node sessions.
- Browser wallet monitoring now re-reads the active profile on each reconnect cycle so endpoint or public-wallet updates are not trapped in a stale startup clone. Added explicit browser monitor routing for start/snapshot/stop and fail-closed architecture regressions preventing the old `wallet_monitor_*` no-op from returning.
- Fixed Discover avatar Horizontal/Vertical/Zoom controls by binding both live `input` and committed `change` events to `HtmlInputElement::value_as_number()`. Slider values are now visible beside each control while direct drag-to-reposition remains supported and uses the same crop state.
- Added browser resolver-response regression coverage and Web network/avatar binding architecture guards. Advanced Android/iOS build metadata to revision 224; semantic package versions remain 0.1.0.

## r223

- Fixed standalone Web Discover publishing for new profiles. `Publish / update` no longer depends on a previously registered Ghost address, and the browser host now resolves/connects to a public Kaspa wRPC endpoint on demand, builds/signs the GTCD in-browser, publishes it through Kaspa Portal, and persists the resulting registered address/wallet projection.
- Added a real Discover avatar editor with a live circular preview, drag-to-reposition, horizontal/vertical positioning, zoom, reset, and an explicit `Apply crop` step. Applied avatars are rendered to a canonical 256x256 PNG before becoming the profile's content-addressed media reference.
- Added standalone Web content-addressed local media import/retrieval so cropped avatars survive reloads and render through the shared Avatar component without Tauri. Moved signed Ghost descriptor construction into `ghost-kaspa` so native and Web publish paths share the same validation/signing logic.
- Added architecture regression guards for Web Discover publication, public-node resolution, avatar crop controls, and browser media caching. Advanced Android/iOS build metadata to revision 223; semantic package versions remain 0.1.0.

## r222

- Cleaned the standalone Web Kaspa build after the r221 target split: native live-carrier helpers (`live_event_id` and `is_ghost_carrier`) are now gated out of `wasm32`, eliminating the two dead-code warnings without compiler suppressions.
- Simplified KasKold UX ownership. `Import from KasKold` now exists only in the identity restore flow, the normal Kaspa page keeps only a compact `KasKold-compatible backup` card, and the old all-in-one compatibility/import/signing panel and unused frontend IPC bridge were removed. KasKold recovery/backup uses the vendored Vault runtime directly through the browser-safe controller boundary.
- Added `Use Signer` to Send KAS. Ghost Talk plans the unsigned transaction, prepares a KasKold KSPT request through the official vendored SDK, accepts the signed KasKold response, merges it back into the PSKT, revalidates current mass/fee policy, and broadcasts through Ghost Talk. The standalone browser path resolves a public Kaspa wRPC endpoint when no explicit endpoint is configured; native desktop/mobile use the same shared transaction planner and broadcast validation.
- Split signer planning/broadcast code into focused modules to preserve the repository's strict SRP limits, added regression guards for the restore-only import / compact backup / Send Signer UI contract, and advanced Android/iOS build metadata to revision 222. Semantic package versions remain 0.1.0.

## r221

- Fixed the standalone Web release build against kaspa-portal 1.0.1 by compile-time gating the native-only BlockAdded subscription adapter on non-WASM targets. Kaspa Portal exposes `NetworkApi::subscribe_block_added` and `next_block_added` only outside `wasm32`; browser builds retain the shared connection/query/transaction APIs while native desktop/mobile keep the live BlockAdded pump. Added an architecture regression guard so native-only Portal notification calls cannot leak into future Web builds. Updated source-package validation for the target-only frontend build layout: generated `target/build/frontend` is intentionally absent from source archives, while the checked-in Web `index.html`/`Trunk.toml` remain mandatory. Advanced Android/iOS build metadata to revision 221; semantic package versions remain 0.1.0.

## r220

- Fixed the standalone Web release incorrectly depending on the Tauri JavaScript API for account bootstrap. Browser builds now dispatch profile state, wallet create/import/unlock/recovery, and HYDRA identity initialization/unlock through a browser-native host instead of `window.__TAURI__`; HYDRA uses its encrypted IndexedDB persistence while profile metadata remains origin-local.
- Split native-only HYDRA file/process locking from the WASM feature graph, enabled Kaspa Portal's WASM feature on browser builds, added shared encrypted JSON vault helpers, and made `getrandom`'s JavaScript entropy backend explicit for `wasm32-unknown-unknown`. Unsupported native-host-only actions now report that the action is unavailable in the standalone Web release rather than the misleading `Tauri API is unavailable in this browser context` error. Added browser regressions and advanced Android/iOS build metadata to revision 220; semantic package versions remain 0.1.0.

## r219

- Added a first-class Web release for `examples/ghost-talk-app`. The double-clickable `build-web-release.cmd` entry point builds the pinned Rust/WASM frontend in release mode and stages the complete directly-servable site under `target/dist/web/release/`, including a recursive `ARTIFACTS.txt` manifest.
- Web builds now force Cargo output into the repository-level `target/`, clean `target/build/frontend` before each release to prevent stale frontend files, and fail if the generated `index.html` is missing. Added architecture regression checks for the Web target/dist contract. Android `versionCode` and iOS `bundleVersion` now track revision 219 while semantic package versions remain 0.1.0.

## r218

- Fixed Windows Android debug builds that could lose the requested debug profile when the operation was relaunched through UAC for symbolic-link support. The resolved `release`/`debug` profile is now explicitly forwarded to the elevated wrapper, and the wrapper invokes `build.ps1 -Debug` for debug builds instead of relying on environment inheritance.
- Hardened Android artifact staging: a build now fails with the exact Gradle output root and any APK/AAB files it found if the requested profile cannot be staged. Debug/release discovery also accepts the profile token in either the Gradle path or artifact filename. Added regression checks for explicit UAC profile forwarding. Android `versionCode` and iOS `bundleVersion` now track revision 218 while semantic package versions remain 0.1.0.

## r217

- Centralized application build output under the repository-level `target/` tree. Cargo now uses the top-level target directory for platform builds, Trunk writes the generated frontend to `target/build/frontend`, and elevated Android transcripts now live under `target/logs/android`.
- Added deterministic artifact staging to `target/dist/<platform>/<release|debug>/`. Android builds stage APK/AAB files and Windows builds stage the runnable EXE plus generated installers; Linux and iOS build scripts use the same distribution layout. Added Windows/Android debug build wrappers and exact artifact-path summaries after successful builds.
- Android build cleanup removes Gradle build/cache/jniLibs output from Tauri's generated project after each build attempt, keeping user-facing and transient build products out of source directories. Android `versionCode` and iOS `bundleVersion` now track revision 217 while semantic package versions remain 0.1.0.

## r216

- Fixed the persistent Rusty-Kaspa Android build failure correctly: Tauri Android release builds now explicitly target ARM64 and ARMv7 instead of the CLI default of all four ABIs. Rusty-Kaspa v2.0.1 `kaspa-hashes/build.rs` panics on x86_64 Android before the crate's `no-asm` Rust feature can take effect, so x86/x86_64 Android targets are no longer provisioned or built.
- Android run scripts now select a connected ARM64/ARMv7 device and fail immediately with the upstream ABI limitation when only x86/x86_64 emulators are connected. Updated mobile docs and regression guards; Android `versionCode` and iOS `bundleVersion` now track revision 216 while semantic package versions remain 0.1.0.

## r215

- Preserved Windows Android failures instead of losing them when the elevated PowerShell window closes. Elevated build/run operations now write a persistent transcript under `target/android-logs`, keep the Administrator window open on failure until acknowledged, and print the last 160 log lines back into Project Repo Manager before returning the non-zero exit code.
- Added mobile regression checks requiring persistent elevated Android diagnostics. Android `versionCode` and iOS `bundleVersion` now track revision 215; semantic package versions remain 0.1.0.

## r214

- Simplified Windows Android elevation after Project Repo Manager exposed another hidden-child/log-bridge stall. When Tauri's Windows jniLibs symlink probe fails, Ghost Talk now opens one visible Administrator PowerShell window and runs the exact Android build/run operation there; PRM waits for and propagates the child exit code.
- Removed the gsudo/hidden bridge/log-tail machinery from the Android elevation path. No Developer Mode mutation or reboot loop is used.
- Android `versionCode` and iOS `bundleVersion` now track revision 214; semantic package versions remain 0.1.0.

## r213

- Fixed the Windows Android elevated-output bridge so native stderr is never merged into PowerShell's error stream. The elevated bridge now launches the Android entry script as a child process with OS-level `RedirectStandardOutput` and `RedirectStandardError`; rustup/Cargo progress written to stderr is streamed back to Project Repo Manager but can no longer terminate the build under `$ErrorActionPreference = Stop`.
- Android `versionCode` and iOS `bundleVersion` now track revision 213; semantic package versions remain 0.1.0.

## r212

- Replaced the gsudo-based Windows Android elevation handoff with a UAC child-process log bridge. Project Repo Manager no longer needs to provide interactive stdin/TTY access to the elevated Android process: the child writes UTF-8 output to a temporary log, the parent tails it into the original managed terminal, and the real child exit code is propagated.
- Added cancellation cleanup for elevated Android process trees and disabled Cargo/Rust color in the elevated bridge to keep PRM terminal decoding deterministic. Removed gsudo from the Android bootstrap critical path. Android `versionCode` and iOS `bundleVersion` now track revision 212; semantic package versions remain 0.1.0.

## r211

- Fixed Android/x86_64 and mobile-simulator Rusty-Kaspa builds by enabling `kaspa-hashes/no-asm` only on Android and iOS, preserving the reviewed Rusty-Kaspa v2.0.1 commit while using its portable Keccak backend on mobile.
- Added a mobile architecture regression contract for the Rusty-Kaspa hash pin and mobile-only `no-asm` feature boundary.

## r210

- Fixed Windows Android gsudo bootstrap return-value corruption: successful WinGet output was emitted on PowerShell's success stream, so assigning `Ensure-GhostGsudo` captured the installer transcript together with `gsudo.exe` and produced `CommandNotFoundException`. Checked native commands now stream normal stdout through `Out-Host` and return no accidental pipeline data.
- Routed gsudo installation through the shared checked-command runner and added mobile architecture regression checks that require output isolation and forbid direct WinGet success-stream leakage. Android `versionCode` and iOS `bundleVersion` now track revision 210; semantic package versions remain 0.1.0.

## r209

- Replaced the detached `Start-Process -Verb RunAs` Android elevation handoff with gsudo so UAC elevation remains attached to the Project Repo Manager console, streams output in place, and propagates the elevated Android command exit code.
- Windows Android bootstrap now installs gsudo through WinGet when missing and refreshes PATH in-process before retrying elevation.

# Changelog

## r208

- Replaced the Windows Android Developer Mode/reboot loop with an operation-level UAC fallback. Build/run now perform a real symlink probe before expensive work; when unprivileged symlinks are denied, Ghost Talk relaunches the exact PowerShell entry script elevated, waits for it, and propagates its exit code. If symlink creation still fails while elevated, the script reports a real policy/filesystem problem instead of requesting another reboot.
- Moved the symlink prerequisite out of shared Android environment setup so bootstrap/init remains non-elevated and only Tauri build/run operations request elevation when required. Added mobile regression checks forbidding the old Developer Mode registry mutation/restart path. Android `versionCode` and iOS `bundleVersion` now track revision 208; semantic package versions remain 0.1.0.

## r207

- Fixed Windows Android builds that reached the release `.so` successfully and then failed when Tauri tried to symlink it into `gen/android/.../jniLibs`. Added a pre-build Windows symlink probe and automatic one-time UAC elevation to enable the Microsoft-documented Developer Mode registry setting when required; if Windows has not activated unprivileged symlinks yet, the bootstrap now stops before compiling and requests one restart instead of failing after a multi-minute build.
- Split Windows host prerequisite handling into `scripts/android/windows_host.ps1`, added mobile regression checks for the symlink bootstrap, and suppressed the vendored KasKold Vault FFI re-export warning in non-test builds without changing exported C symbols. Android `versionCode` and iOS `bundleVersion` now track revision 207; semantic package versions remain 0.1.0.

## r206

- Fixed the Windows Android first-run JDK probe after Microsoft OpenJDK 17 installation: `java -version` writes its version banner to stderr, and the old `2>&1` probe promoted that harmless output to a terminating `NativeCommandError` under strict PowerShell error handling. Added `Invoke-GhostNativeCapture` to capture native stdout/stderr safely, check the real process exit code, restore strict mode afterward, and reuse the same version-probe path for Java, Trunk, and Tauri.
- Added a mobile architecture regression check forbidding the unsafe `& $Java -version 2>&1` pattern. Android `versionCode` and iOS `bundleVersion` now track repository revision 206; semantic package versions remain 0.1.0.

## r205

- Added first-class Android and iOS Ghost Talk applications as Tauri 2 mobile shells over the same Rust/Yew frontend and native command surface used on desktop, preserving one UX/UI and one implementation for chat, Rooms, reactions/Mask, KasKold interoperability, wallet operations, HYDRA, media, and Studio. Added mobile-safe shared CSS for viewport safe areas, coarse-pointer touch targets, dynamic viewport height, and iOS form-focus behavior.
- Replaced the placeholder mobile runners with self-bootstrap build/run flows under `examples/ghost-talk-app/scripts`: Android provisions pinned Rust/Trunk/Tauri, Java 17, verified Android command-line tools, API 36/build-tools 36.0.0, NDK 30, and Rust Android targets; iOS provisions Rust/Trunk/Tauri, iOS Rust targets, Homebrew/CocoaPods, validates full Xcode, and supports Sonoma with Xcode 15+. Generated native projects are initialized on first use and receive deterministic Ghost Talk microphone/camera/LAN/privacy configuration.
- Added mobile architecture regression checks and a mobile implementation guide. Android `versionCode` and iOS `bundleVersion` track repository revision 205 while the Ghost Talk semantic package version remains 0.1.0.

## r204

- Fixed the five measured CRAP<=25 failures reported by the combined LCOV gate without weakening coverage policy. `allowed_by_policy`, broadcast configuration validation, authenticated KKTP projection, reaction-send validation, and Kaspa archive locator validation are now structurally CC<=4, capping zero-coverage CRAP at 20 or lower instead of 30.
- Split authenticated KKTP projection into its own semantic session module to preserve the strict <300-line Rust source limit, added a direct Room channel-policy authorization test, and added an architecture regression guard for the five known zero-coverage CRAP boundaries.

## r203

- Fixed the HYDRA stale-profile reopen error mapper to use the actual upstream `hydra_msg::HydraMsgError` type instead of the nonexistent `hydra_msg::HydraError`, resolving the Rust compile failure reached after the CRAP/rustfmt cleanup. Added a HYDRA architecture regression guard that requires the real upstream error type and rejects the stale name.

## r202

- Applied the two Rust 1.95 rustfmt changes required after the r201 KasKold CRAP-safe dispatch refactor: wrapped the XPrv import method chain and collapsed the `key_backup` signature to the formatter-required single line. No runtime, coverage-threshold, KasKold format, signing, Room, or UI behavior changed.

## r201

- Fixed the CRAP<=25 coverage gate risk introduced by the KasKold compatibility tranche by splitting text import, byte import, and backup dispatch into cohesive helpers whose structural complexity is at most 4; even an unmeasured host dispatch helper therefore remains below CRAP 25 instead of scoring 30-42 at zero coverage.
- Added focused KasKold compatibility tests for mnemonic/SeedQR/XPrv/raw-key text import, portable/recovery byte import, plain/portable backup dispatch, and unsupported-format rejection. Reduced HYDRA stale-profile reopen complexity from 5 to 4 by extracting reopened-profile error mapping, and added an architecture guard that keeps KasKold host dispatch CRAP-safe without weakening the CRAP threshold.

## r200

- Applied the final Rust 1.95 rustfmt wrapping in the Studio episode publisher so the `publish_episode(...).await` match remains on the formatter-required single line. No runtime, broadcast, archive, podcast, KasKold, signing, Room, or UI behavior changed.

## r199

- Fixed the Rust 1.95 WASM Clippy `-D warnings` failures in Studio without lint exemptions: grouped permanent-archive publication state in `ArchivePublishTask`, changed station submission to consume the existing `CatalogUi`, and introduced `EpisodePublishRequest` for podcast publication instead of eight-argument functions.
- Replaced the RTMP/local-recording `then(...).unwrap_or_default()` rendering chains with explicit `if`/`else` HTML branches and added an architecture regression contract for the Studio Clippy boundary. No broadcast, archive, podcast, KasKold, signing, or Room behavior changed.

## r198

- Applied the Rust 1.95 rustfmt wrapping required for the two `wallet_history(std::slice::from_ref(...))` calls introduced by the r197 Clippy cleanup. Extracted the recovered archive-transaction lookup into a cohesive helper because the formatter wrapping otherwise pushed `reconcile_in_flight` over the normal SRP review threshold. No behavior, protocol, KasKold, signing, or UI semantics changed.

## r197

- Fixed the next Rust 1.95 Clippy `-D warnings` layer in `ghost-talk-native`: replaced cloned one-element address slices with `std::slice::from_ref`, removed needless references in persistent realtime AAD construction, replaced `Result::err().expect()` branches with direct `match` handling, removed the now-unneeded `PublicGhostProfile` struct update, and deleted the stale seven-argument `too_many_arguments` expectation.
- Proactively removed the same unreached `err().expect()` pattern from HYDRA native-profile open recovery while preserving the existing stale-lock recovery behavior. No protocol, KasKold, transaction-signing, or UI semantics changed.

## r196

- Fixed Rust 1.95 Clippy `-D warnings` failures by replacing boolean `then_some(...).unwrap_or(...)` chains with explicit `if`/`else` branches in HYDRA profile-open recovery, profile lease errors, contact-label selection, KKTP session-end role ordering, and Kasia availability labels.
- Fixed `field_reassign_with_default` in the broadcast catalog test by constructing `CreatorProfile` values with struct update syntax. Proactively scanned the first-party Rust tree for the same direct Clippy patterns so the workspace lint run does not immediately stop on another identical occurrence.

## r195

- Fixed the WASM Mask composer compile error by precomputing the literal asterisk display outside the Yew `html!` body and rendering the resulting string, preserving one `*` per typed character without exposing the real draft.
- Fixed the KasKold WASM review bridge by re-exporting `KasKoldReviewResult` from `model` and removing the unused `KasKoldWalletSummary` re-export that would fail warnings-as-errors. Added architecture regression checks for both compile-facing contracts.

## r194

- Resolved the KasKold/Ghost Talk Cargo dependency conflict by aligning the vendored KasKold SDK/protocol WASM binding pins with the exact wasm-bindgen family already required by the pinned Kaspa stack: `wasm-bindgen = 0.2.100` and `js-sys = 0.3.77`. The KasKold Rust API, PSKT behavior, import/backup formats, and native Vault integration are unchanged.
- Added an architecture regression guard requiring the vendored KasKold SDK and protocol manifests to remain on the compatible wasm-bindgen/js-sys family while Ghost Talk's Kaspa dependencies are pinned there, preventing an upstream KasKold refresh from reintroducing an unsatisfiable Cargo graph.

## r193

- Applied the complete Rust 1.95 `ghost-wasm` formatter output for the KasKold compatibility UI/controller/native bridge after r192 made the native side format-clean. No KasKold import, backup, signing, or chat/Rooms behavior changed.
- Kept the architecture thresholds strict after rustfmt expanded the KasKold signing callback: extracted the asynchronous signing work into a dedicated `SignTask`/`spawn_sign` helper instead of adding an SRP exemption.

## r192

- Applied the complete rustfmt output from the first successful Cargo metadata pass after the KasKold compatibility integration. This is formatting/module-order cleanup only across `ghost-api` and `ghost-talk-native`; no runtime behavior, import/backup format, or PSKT signing logic changed.
- Kept the explicit KasKold workspace exclusions and path guards from r191 intact while making the newly added compatibility modules `cargo fmt --all -- --check` clean according to the Windows Rust 1.95 formatter output.

## r191

- Fixed Cargo workspace ownership for the vendored KasKold compatibility crates. The root workspace now excludes all six KasKold crate directories explicitly instead of relying on the ineffective `external/kaskold/*` wildcard. This keeps `kaskold-sdk`, `vault-runtime`, and their sibling compatibility crates outside the Ghost Talk workspace while still allowing `ghost-talk-native` to consume them as local path dependencies.
- Strengthened the KasKold architecture regression check so every vendored KasKold crate must have an explicit root-workspace exclusion, preventing Cargo metadata/`cargo fmt --all` from failing with “current package believes it's in a workspace when it's not.”

## r190

- Fixed KasKold compatibility path dependencies in `ghost-talk-native`: the vendored `vault-runtime` and `kaskold-sdk` paths now resolve four levels up to the Ghost Talk repository root (`../../../../external/kaskold/...`) instead of escaping the repository into the parent Downloads directory.
- Explicitly excluded `external/kaskold/*` from the root Ghost Talk Cargo workspace while retaining those crates as native path dependencies, and added an architecture regression check that resolves both KasKold dependency paths on disk.

## r189

- Reworked chat interaction layout: Mask mode now renders one literal `*` per composer character while retaining the real draft internally; the Call/Mask/Advanced/Add contact/Leave/Archive controls stay in one horizontal action strip; each message owns a bottom-left thumbs-up reaction trigger that expands the full reaction vocabulary, with reaction summaries attached to the same message.
- Expanded the Rooms desktop workspace and create-room form so the room list, creation controls, selected-room chat, roster, and owner controls have usable width instead of collapsing into a narrow card.
- Added KasKold 2.0 wallet interoperability using the user-supplied KasKold SDK/Vault source: recovery words, SeedQR/recovery material, account XPrv, raw private key, portable `.kwp`, steganographic JPEG, and Compact SeedQR imports; recovery words, SeedQR, Compact SeedQR, account XPrv, portable encrypted, portable XPrv, and steganographic JPEG backups; and SDK PSKT prepare/complete/finalize signing.
- KasKold PSKT signing preserves the Vault review-before-sign boundary: Ghost Talk renders the decoded KasKold transaction review first and requires a separate approval tied to a SHA-256 token of the exact PSKT and network before signing. Imported KasKold inventory is encrypted under Ghost profile custody and preserved across wallet-state reconciliation.
- Vendored the required KasKold compatibility crates under `external/kaskold/` with upstream licensing/provenance retained, classified them as external compatibility source rather than Ghost Talk first-party workspace crates, and added architecture guards for the KasKold/UI compatibility contracts.

## r188

- Fixed the Windows wasm-pack bootstrap wiring so the verified pinned executable is carried forward as `GHOST_TALK_WASM_PACK_EXE` and invoked by exact path. The runner no longer discards a successful bootstrap result and performs a redundant second PATH/PATHEXT lookup before browser tests.
- Added a mandatory matrix regression guard requiring exact-path wasm-pack handoff from bootstrap to the browser-test gate.

## r187

- Windows QA now provisions pinned `wasm-pack 0.14.0` into the same isolated `%LOCALAPPDATA%\GhostTalk\qa-tools` root used for coverage tooling, instead of requiring a global `wasm-pack` on the inherited PATH. The bootstrap verifies the exact installed executable with the pinned QA environment and reuses it on later runs.
- Added a matrix regression guard for the pinned Windows wasm-pack bootstrap.

## r186

- Fixed Windows coverage-tool verification after a successful pinned `cargo-llvm-cov` install. The QA bootstrap now verifies the tool through Cargo's supported subcommand entry point (`cargo llvm-cov --version`) instead of invoking `cargo-llvm-cov.exe` directly, eliminating the false "installed but unavailable" failure seen with the isolated QA tool directory.
- Added a mandatory regression guard requiring Cargo-subcommand verification for the pinned coverage tool.

## r185

- Fixed the Windows `cargo-llvm-cov` post-install probe to execute with the same isolated QA environment (`PATH`, `RUSTUP_HOME`, `CARGO_HOME`, and pinned toolchain) used for installation. This prevents a successfully installed `%LOCALAPPDATA%\GhostTalk\qa-tools\bin\cargo-llvm-cov.exe` from being falsely reported as unavailable.
- Extended the test-matrix regression guard to require environment-aware executable probes for Windows QA tools.

## r184

- Fixed the Windows coverage bootstrap executable probe so `_runnable` accepts probe arguments consistently. This removes the `TypeError: _runnable() takes 1 positional argument but 2 were given` crash encountered before `cargo-llvm-cov` installation/version verification.
- Added a test-matrix regression guard for the variadic executable-probe contract used by the Windows QA coverage bootstrap.

## r183

- Made Windows coverage bootstrap self-contained: the isolated pinned Rust 1.95.0 toolchain now includes `llvm-tools-preview`, and the application QA runner automatically installs pinned `cargo-llvm-cov 0.8.7` with `--locked` into `%LOCALAPPDATA%\GhostTalk\qa-tools` when it is missing. The user's system Cargo installation remains untouched.

## r182

- Applied the final rustfmt layout required by the standalone `ghost-wasm` formatting gate for the WASM native-command re-export list. No runtime behavior changed.

## r181

- Fixed the first real `wasm32-unknown-unknown` compile pass after the Kasia portability split: re-exported the reaction/media native bridge calls through the WASM facade, removed a Kasia sync borrow conflict, restored the missing reaction `spawn_local` import, corrected Yew conditional rendering for media import status, and preserved Studio status handles across submit callbacks.
- Disabled Cargo incremental compilation in the deterministic Windows QA runner to avoid non-fatal Windows `metadata.rmeta` incremental-cache copy warnings caused by transient file locks during validation.

## r180

- Applied the remaining rustfmt re-export ordering in `ghost-kasia/src/lib.rs` after the WASM/native feature split. This is formatting-only and does not change Kasia behavior or feature ownership.

## r179

- Split `ghost-kasia` into a portable default-free model/wire surface and an explicit `native` crypto/indexer feature. `ghost-wasm` and `ghost-runtime` consume only the portable Kasia DTO/mapping layer, while `ghost-talk-native` enables the full ECIES/indexer implementation. This keeps `secp256k1-sys` and its C toolchain requirements out of `wasm32-unknown-unknown` builds without changing native KaChat/Kasia interoperability.
- Enabled the `getrandom` 0.2 browser backend for the portable Kasia mapping layer so WASM route-ID generation uses Web Crypto instead of inheriting a native-only entropy path.

All notable user-facing and downstream-facing changes are recorded here. Ghost Talk remains on package version `0.1.0` during the current development line; repository revisions are tracked separately.

## r178

- Made Windows QA honor the repository-pinned Rust `1.95.0` toolchain even when the host only has a standalone Rust MSI installation. When global `rustup` is absent, QA now bootstraps a checksum-verified, isolated rustup environment under the user profile instead of modifying the standalone Rust installation.
- The isolated toolchain provisions the pinned minimal compiler plus `rustfmt`, `clippy`, and `wasm32-unknown-unknown` before WASM gates run, eliminating the standalone-installer cross-target dead end.

## r177

- Fixed the reaction protocol test module to import the shared `BASE64` engine from its actual `events` owner, restoring `cargo check --workspace --all-targets` after the reaction-test module split. No production behavior changed.

## r176

- Applied the final `ghost-wasm` rustfmt cleanup left by the r175 Room ingress split: compacted two state helper signatures and removed one extra blank line. No behavior changed.

## r175

- Applied the complete `ghost-wasm` rustfmt diff surfaced by the Windows quality gates across 81 WASM/UI source files, covering 234 formatter hunks without changing behavior.
- Split Room state-ingress ownership into a dedicated semantic module after rustfmt expansion pushed the parent module over the strict `<300`-line SRP limit.
- Extracted the Studio permanent-archive publish task so the UI callback remains below the normal SRP review threshold without adding an exemption.

## r174

- Restored rustfmt-clean import ordering/wrapping in the native HYDRA message-delivery and mailbox handshake send-preparation modules so the mandatory `cargo fmt --all -- --check` gate passes after the recent module splits.

## r173

- Fixed Windows QA startup for valid Rust installations where Cargo is available but `rustup.exe` is not colocated or installed. Cargo is now the only mandatory Rust executable at startup.
- Rust discovery now resolves Cargo independently across inherited PATH, Cargo homes, Scoop, and common MSYS2 locations; `rustup` and `rustc` are discovered independently and treated as optional helpers.
- WASM target verification can use `rustc` directly. `rustup` is required only when `wasm32-unknown-unknown` is actually missing and must be installed, with a precise failure message in that case.

## Unreleased

### Added
- Added authenticated Ghost-native message reactions for direct chats and Rooms: Like 👍, Love ❤️, Haha 😂, Sad 😢, Surprised 😮, Dislike 👎, Disgust 🤢, Angry 😡, and Fear 😨. Reactions are attached to the target message, use one-reaction-per-actor semantics, and can be changed or removed.

### Changed
- Kept the `ghost-talk-native` profile-state lock guard fully private and changed `acquire_profile_state_lock` to return an opaque RAII guard, eliminating the `private_interfaces`/private-type boundary entirely while preserving lock lifetime through the caller scope.
- Repaired native clean-checkout compilation after the r159 module splits: fixed HYDRA/message-delivery module paths, handshake send-preparation visibility/imports, wallet broadcast projection re-exporting, media-store ownership access, and the canonical GTCD version owner.
- Added the missing direct `ghost-domain` dependency to `ghost-talk-native` and the missing direct `ghost-protocol` dependency to standalone `ghost-wasm`; architecture now rejects any first-party crate used directly without a declared dependency.
- Made Tauri `frontendDist` valid in a clean source checkout with a checked-in bootstrap frontend; platform build scripts now stage the generated WASM bundle into that owned frontend directory before Tauri packaging.
- Documented every public named field in the published `ghost-talk` SDK error/outcome enums so the crate satisfies its enforced `#![deny(missing_docs)]` contract.
- Added serde support and a round-trip regression for `KasiaHandshake`, fixing the native/API serialization compile failure in `KasiaReceivedHandshake` without changing the custom KaChat wire encoder/decoder.
- Corrected the Kaspa archive publish API trait contract so `KaspaArchivePublishResult` no longer requires `Eq` from the wallet broadcast projection; this fixes the native workspace compile failure while retaining `PartialEq` for comparisons.
- Consolidated application/runtime ownership and normalized Rust module layout.
- Centralized outbound mailbox unlock, fee parsing, and submission setup.
- Strengthened architecture, dependency, complexity, and test-matrix guardrails.
- Added early native compilation gates so optional WASM provisioning cannot hide source breakage.
- Added downstream contribution, security, CI, and release-engineering policy.

### Removed
- Removed unconsumed first-party crates and unused first-party path dependencies instead of retaining legacy compatibility surface.

## r172

- Fixed Windows quality-gate startup when Project Repo Manager inherits a PATH without `rustup`: Ghost Talk now resolves the Rust toolchain before MSVC initialization, including `%CARGO_HOME%\bin` and the standard `%USERPROFILE%\.cargo\bin` rustup installation.
- Added a shared `_rust-env.cmd` used by both Windows entry points; it verifies `cargo.exe` and `rustup.exe`, preserves their exact paths, and re-prepends the resolved Cargo bin after Visual Studio environment setup so `cargo`, `rustup`, `wasm-pack`, and installed Cargo subcommands remain discoverable.
- Hardened the Python Windows orchestrator with independent Rust-tool discovery and exact-rustup execution for WASM target provisioning, plus matrix regressions requiring Python/Rust resolution to occur before MSVC can rewrite PATH.

## r171

- Fixed the Windows reference-app quality launcher to resolve Python **before** Visual Studio/MSVC environment initialization, preserving the exact `sys.executable` path even when `VsDevCmd.bat` rewrites `PATH`.
- Hardened Python discovery for modern Python 3.14+ Windows installs with Python Install Manager (`%LOCALAPPDATA%\Python\pythoncore-*`), WindowsApps aliases, and PEP 514 registry `ExecutablePath` entries, in addition to the existing direct-command, traditional CPython, MSYS2, and Scoop probes.
- Replaced the fragile `FOR /F` command-substitution probe with a direct executable probe through a temporary file, and added a matrix regression requiring Python discovery to precede MSVC setup.

## r170

- Fixed Windows Python discovery to trust an actually runnable `python`/`py -3`/`python3` command directly instead of requiring `where.exe` to approve it first. The helper now resolves the interpreter from `sys.executable`, then invokes that exact executable for the quality-gate orchestrator.
- Replaced hard-coded per-user CPython minor-version probes with wildcard discovery for `Python*` installs and added machine-wide `Program Files` discovery, while retaining MSYS2 and Scoop fallbacks.
- Added a test-matrix regression guard that rejects reintroducing `where %~1` as a prerequisite for a runnable Python command.

## r169

- Removed the accidental PATH-only Python assumption from the Windows quality-gate launcher. Both Windows entry points now discover and execute a real Python 3 interpreter from `py`/`python`/`python3`, standard per-user CPython installs, common MSYS2 environments including `C:\\msys64\\mingw64\\bin\\python3.exe`, or Scoop. Windows Store stubs are still rejected because every candidate must execute a Python 3 probe successfully.
- Added a regression guard requiring the shared Windows Python discovery helper and its MSYS2 fallback so the reference-app quality runner remains usable on Windows development hosts where Python is installed but not exported on CMD `PATH`.

## r168

- Fixed the Windows reference-app launcher so a broken/non-runnable `python` App Execution Alias can no longer be mistaken for a successful QA run. Python candidates are now executed as probes before selection, and the child orchestrator exit code is captured outside CMD parenthesized blocks so a launch failure cannot reuse the preceding `where` command's zero `%ERRORLEVEL%`.
- Hardened Windows RC.EXE bootstrap reporting so the resource-compiler line always resolves to a concrete executable path or fails the gate instead of announcing a blank ready state.
- Added a test-matrix regression guard that rejects the stale `%ERRORLEVEL%` capture pattern.

## r167

- Replaced the Windows reference-app batch gate loop with a single-process Python orchestrator that executes every Cargo/WASM/coverage gate sequentially with `shell=False`, stops on the first failing gate, and holds an exclusive per-checkout lock so overlapping quality runs cannot occur.
- Bound every Windows QA subprocess to the repository-pinned Rust toolchain and added an exact `ghost-wasm` `wasm32-unknown-unknown` Cargo probe immediately after target provisioning, so a missing/mismatched target fails once before the expensive test matrix.

## r166

- Gate the `ghost-wasm` `ReactionKind` model re-export to `wasm32`, matching the browser-only modules that consume it and keeping native warnings-as-errors builds clean.

