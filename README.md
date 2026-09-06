# Ghost Talk

Revision: `r77`

**r77 TypeScript voice-build repair:** Ghost Talk now copies each decoded base64 media window into a concrete `ArrayBuffer` before `AudioContext.decodeAudioData()`. This removes the TypeScript 5.9 DOM `BlobPart` generic mismatch (`Uint8Array<ArrayBufferLike>` vs `ArrayBufferView<ArrayBuffer>`) without changing the r76 Kinesis-style capture, ordering, jitter buffering, or gapless playout behavior.

**r76 live-voice continuity repair:** Ghost Talk now follows the proven Kaspa Kinesis capture/playout model for full-duplex voice. Finalizing a complete 350 ms Opus/WebM window immediately starts the next recorder before encryption or network submission, so a slow Kaspa fallback broadcast cannot create missing microphone time. Received call-local sequence numbers are jitter-buffered and restored in order; each complete window is decoded to PCM and scheduled contiguously on one `AudioContext` clock instead of creating a new `HTMLAudioElement` after the previous element ends. Auto's WebRTC media data channel is now ordered and reliable rather than unordered with one retransmission. HYDRA encryption, exact-SID routing, Kaspa fallback, Kaspa Portal, and KasSigner are unchanged.

**r75 automatic direct-route + live-voice signaling repair:** the Advanced transport policy now has only **Auto** and **Kaspa only**. Auto automatically negotiates one SID-bound WebRTC realtime connection after an authenticated KKTP chat becomes active; completed offer/answer SDP is carried through the existing authenticated Kaspa realtime carrier, and direct application bytes are HYDRA-encrypted natively before the WebRTC data channel sees them. If ICE/WebRTC is unavailable or the peer channel drops, live media falls back to Kaspa; Kaspa only never opens a peer connection. Live voice request/accept/decline/hangup control also uses the exact authenticated SID-bound realtime carrier instead of peer-only routing, and call state updates `callRef` synchronously so a fast Accept cannot arrive before the caller has recorded its active call. Ghost Talk serializes mailbox spends per profile and waits for an accepted transaction's updated live UTXO view before the next Auto/voice/text carrier may plan, preventing simultaneous signaling and call-control broadcasts from racing the same input. The long peer Kaspa address in the voice modal now wraps inside the dialog. Kaspa Portal and KasSigner are unchanged.

**r74 Kaspa delivery semantics, session isolation, and chat-follow repair:** a successful Kaspa `SubmitTransaction` is now the user-visible **Delivered** boundary for ordinary durable chat because the carrier has been accepted by the Kaspa network; Ghost Talk no longer leaves successfully broadcast messages stuck at “Awaiting delivery.” The signed acknowledgement retained for `pq_finish` is bootstrap/session-activation evidence only, not ordinary message-delivery status. Authenticated KKTP plaintext now carries its verified session SID all the way to the UI and is admitted only to the exact active thread with that SID; historical Discovery/Response/message/session-end carriers cannot overwrite or repopulate a newer same-peer chat. On restart recovery, a replacement SID is persisted only after its recovery `pq_init` successfully broadcasts, keeping the strict SID filter restart-safe. The selected conversation also auto-scrolls to the newest appended message. Kaspa Portal and KasSigner are unchanged.

**r73 Kaspa status-indicator synchronization repair:** fixes the top-left Kaspa connection dot remaining in the blinking yellow `connecting` state after the shared Portal connection is already healthy and able to broadcast transactions. The frontend now attaches the network, wallet-live, and directory listeners before starting the native wallet monitor or issuing the first live wallet RPC, so a fast `connecting → connected` transition cannot be emitted before the WebView listener exists. A successful `refreshWallet()` result also explicitly confirms `connected` because it obtains current UTXOs/DAA through the same live Kaspa Portal gateway used for transaction submission. This changes only Ghost Talk UI/runtime synchronization; Kaspa Portal and KasSigner remain unchanged.

**r72 Ghost-created KasSigner signing repair:** fixes the r71 frontend TypeScript regression and restores the intended hardware-signing model for normal Ghost Talk wallets. A Ghost-created wallet does not need to import a kpub before KasSigner can sign it: immediately before hardware planning, native Ghost Talk reopens or reuses the encrypted wallet secret, derives the canonical receive/change projection from the same mnemonic/passphrase/account path, preserves only the current address cursors, and validates every Portal `kaspaPortalDerivation { branch, index }` against that seed-derived script before translating it to the equivalent KasSigner `kassignerDerivation`. The separate imported-kpub/fingerprint path remains only for explicitly watch-only KasSigner-backed profiles. The normal `sendKaspa()` software wrapper no longer carries the accidental `accountFingerprint` field that broke `npm run build`. KasSigner SDK/protocol/firmware remain unchanged, and the r70 high-resolution Android QR scanner repair is retained.

**r71 (superseded by r72) KasSigner wallet-ownership binding repair:** introduced strict kpub-derived ownership validation but incorrectly assumed every hardware-signing profile was an imported watch-only KasSigner account. That assumption broke the existing Ghost-created-wallet → same-mnemonic-on-KasSigner signing path and also introduced a TypeScript argument mismatch. r72 replaces that assumption with source-appropriate validation.

**r69 Kaspa Portal 1.0.1 integration cleanup:** Ghost Talk imports the published `kaspa-portal = "1.0.1"` crate and relies on its public native integration directly. Portal owns the persistent wRPC socket, native `Send` futures, request/notification multiplexing, reconnect/subscription replay, decoded `BlockAdded` API, payload-aware transaction planning, and authoritative mass/fee analysis. Ghost Talk no longer carries a Portal worker thread/current-thread runtime shim, raw `tokio-tungstenite` BlockAdded socket/decoder, or its own approximate mass formula. The 80 KiB Ghost Talk carrier limit remains a logical application bound; Portal 1.0.1/KSPT v1 permits 65,535 payload bytes per physical transaction, so GHST fragments only when a logical carrier genuinely crosses that wire-format boundary. No Kaspa Portal source is copied or patched in this repository.

Ghost Talk is a Kaspa-native, privacy-focused chat application: simple messenger UX by default, with power-user controls for HYDRA steganography, Kaspa/direct routing, presenter/radio rooms, diagnostics and network policy.

## Platforms

- Windows
- Debian / **Devuan** (the Linux release-test target)
- Android
- iOS 15+ using **Xcode 16.2 on macOS Sonoma**
- WASM/PWA

Ubuntu is intentionally **not** a release-test target.

## Security and transport ownership

- **HYDRA-MSG** owns authenticated direct sessions, post-quantum messaging, group authorization and optional steganography.
- **KKTP v2** defines direct-chat discovery, consent, session identity, mailbox ordering and replay semantics; see [`docs/spec/kktp.md`](docs/spec/kktp.md) for the HYDRA-PQ deployment profile.
- **KasSigner hardware signing** is optional for manual KAS sends/consolidation: Ghost Talk keeps wallet/broadcast policy, translates Portal output derivation hints into the SDK's equivalent metadata, and the pinned `@kassigner/sdk` 2.0.0 consumer package prepares QR requests, validates the signed response, and finalizes hardware-approved PSKT material.
- **Kaspa Portal** owns the persistent Kaspa connection, wallet/chain RPC, transaction planning/broadcast, mass/fee analysis, BlockAdded subscription/decoding, reconnect/replay behavior, and indexer integration. Ghost Talk consumes those public APIs and only fans decoded live events into its application protocol.
- **Kaspa Kinesis Voice** is reference-only. Its Opus/Kaspa transport concepts informed the Kaspa-only voice path; its legacy session crypto is not a runtime dependency.
- Ghost Talk owns accounts, contacts/KNS discovery, synchronization, routing, rooms, voice orchestration, games, storage and platform UX.

## Current application flow

The native app starts at a local ID picker (auto-login is allowed only when exactly one ID exists). A normal software-backed Ghost Talk ID is created/restored from exactly one 24-word BIP39 root; the Kaspa wallet follows the selected standard/custom BIP32 path while the HYDRA identity is deterministically derived from the same BIP39 seed under the separate `GhostTalk/HYDRA-ID/v1` domain. A KasSigner-backed ID can instead import/scan only an account kpub: before that public account is accepted, Ghost Talk generates a one-time domain-separated identity challenge bound to the network, account fingerprint, local Ghost Talk profile, HYDRA identity, nonce and creation time; the user must review/sign it with KasSigner’s Sign Message workflow and Ghost Talk verifies the returned BIP340 signature against the account kpub, so copying somebody else’s kpub is not enough to claim it. That financial wallet remains watch-only and every spend is hardware-approved, while Ghost Talk creates a separate low-value local mailbox/HYDRA root so chat carrier traffic never requires a hardware scan. The Ghost Talk 24-word recovery shown for that mode restores the messaging identity/mailbox only; the KasSigner financial wallet remains recoverable from KasSigner’s own wallet backup. The app then exposes real Chats, Contacts, Discover, Rooms, Games, Kaspa and Settings views. Chat rows open directly, incoming notifications can jump to the exact thread, outgoing messages render immediately with Sending/Sent/Delivered/Not sent state, and chats can be archived locally or explicitly left without pretending the immutable on-chain history was deleted. The Kaspa view rotates receive/change addresses, derives the current balance/UTXO count/current DAA directly from the selected public Kaspa node through Kaspa Portal wRPC, sends/receives KAS, renders receive QR codes, consolidates UTXOs, and optionally routes manual financial signing through KasSigner hardware. REST is archival-only and may contribute transaction history only when a transaction is proven older than 48 hours; it is never a source of current balance, current UTXOs, current DAA, live messaging, or connection status.
For realtime routing, **Auto** attempts a direct WebRTC connection as soon as the authenticated chat SID is active. Kaspa remains the authenticated signaling and durable/offline fallback carrier; **Kaspa only** disables the direct peer connection completely. The WebRTC channel receives only opaque HYDRA ciphertext, never plaintext or raw session keys.

KasSigner animated signing requests provide Back, Pause/Resume, and Forward frame controls; Portal change derivations are translated into KasSigner metadata before request generation so hardware review can independently verify change ownership. When scanning a multi-frame signed response, the camera view shows one progress dot per frame after the first successful scan and fills each dot as that frame is captured. All KasSigner camera flows share the same rear-camera/focus fallback and preserve up to 1280×960 source detail for dense hardware QR frames.

Ghost Talk owns one selected `PortalFacade` at a time. Candidate endpoints are attempted sequentially, and endpoint failover is allowed only after the active Portal reports an unrecoverable transport failure. Portal 1.0.1 owns the single persistent wRPC connection used for wallet/chain RPCs, transaction submission, and `BlockAdded`; Ghost Talk subscribes through Portal's public `subscribe_block_added()` / `next_block_added()` API and fans the decoded events to mailbox consumers. An ambiguously submitted `SubmitTransaction` is never replayed automatically by Ghost Talk.

For live mailbox delivery, the `BlockAdded` notification itself carries the block. Ghost Talk scans that block’s transaction payloads directly and immediately admits `KKTP:` plus the supported Ghost carrier prefixes; there is no REST `GetBlocks` request, output-address metadata gate, UTXO-change wakeup, or periodic live polling between the Kaspa notification and protocol processing. Current wallet state continues to come from Kaspa Portal `ChainApi` calls to the selected public node. REST/indexer access is strictly archival: only transactions whose block time proves they are older than 48 hours may be retrieved there, and those records never influence current balance, UTXOs, DAA, connection status, profile-currentness, or live carrier delivery. Mailbox and ordinary KAS transactions use Portal 1.0.1 payload-aware planning and Portal's exact signed-PSKT mass/fee analysis; Ghost Talk no longer maintains a second mass formula. HD receive/change cursor movement updates the monitor in place and does not create another Portal connection.

Protocol debugging is disabled by default. Settings → **Protocol debugging** can enable a bounded native event log and open the **Protocol Debug** window. The window shows timestamped Discovery/Response/PQ-control/carrier transitions plus a redacted live HYDRA/KKTP state snapshot (SID, role, state, mailbox ID, sequence counters and pending stage identifiers). It intentionally excludes message plaintext, passwords, recovery words, private keys and ciphertext. Valid Response anchors that temporarily lack their local pending-request context are retained as bounded orphan responses and retried after local state reconciliation rather than being silently discarded.

First-contact bootstrap uses the documented **Ghost Talk KKTP v2 / HYDRA-PQ profile**: canonical Discovery/Response consent anchors establish one random `sid` and fixed A/B roles, then HYDRA's ML-DSA-65-authenticated X25519 + ML-KEM-768 `INIT → RESP → FINISH` runs as explicit KKTP PQ handshake stages; each complete KKTP handshake-control context is itself ML-DSA-65 signed before its SID/roles/stage/payload can affect state. Normal direct messages bind `sid`, role-ordered mailbox ID, direction and per-direction sequence both outside and inside the HYDRA-authenticated ciphertext. After activation, ordinary text and recorded-voice messages retain one logical KKTP carrier per message; that logical carrier is encoded into one or more bounded GHST physical Kaspa transactions when required by the KSPT payload limit. A successful `SubmitTransaction` commits that exact prepared packet locally as **Delivered** because the durable carrier has been accepted by the Kaspa network; Ghost Talk does **not** generate a second on-chain receipt transaction for ordinary messages. The authenticated acknowledgement retained for `pq_finish` is bootstrap/session-activation evidence only and does not redefine ordinary message delivery. If submission fails or is ambiguous, the stable logical message ID reuses the exact prepared SID/sequence/HYDRA ciphertext rather than encrypting again or burning another sequence number. The responder's signed acknowledgement is retained only for the special `pq_finish` bootstrap proof, where it confirms that the responder established the ratchet and observed the embedded first message. Live call windows remain intentionally lossy and outside the strict KKTP text sequence, so a missed 350 ms audio carrier cannot stall durable chat. In Auto mode those windows use the post-handshake HYDRA-protected WebRTC channel when available and fall back to the exact-SID Kaspa realtime carrier otherwise.

Leaving an active direct chat emits a canonical KKTP v2 `session_end` anchor that is ML-DSA-65 signed by the HYDRA identity and then BIP340 signed by the sender's Kaspa routing key; after verification, the peer retires that exact `sid`, closes the ratchet, and blocks further traffic from the ended thread until an explicit fresh chat establishes a new `sid`. Bootstrap carriers are not timer-rebroadcast after successful submission. Exact cached INIT/FINISH bytes remain available only for an actual failed/ambiguous send retry, and the signed `pq_finish` acknowledgement remains the peer-confirmed activation point. Profiles created by earlier scanner generations receive a one-time full address-history reconciliation before normal overlap scanning resumes, so already-on-chain requests are recovered after upgrading. Public GTCD discovery/indexing runs on its own best-effort background cursor and cannot delay private mailbox delivery. GHST carrier fragments are retained across restarts until complete, then passed through the selected profile's native HYDRA state. Only successfully authenticated/decrypted HYDRA plaintext whose verified KKTP SID exactly matches the active persisted thread becomes a chat message; historical same-peer SIDs are discarded rather than reassigned to a newer conversation. HYDRA ratchet sessions remain memory-only: a restart that invalidates an old-session ciphertext triggers a fresh authenticated peer-bound handshake, and the replacement SID becomes durable only after its recovery offer is accepted by Kaspa. By default, the wallet/HYDRA password is entered once to unlock the active app session and KAS sends reuse that in-memory authorization. Settings can require password re-entry for every KAS send, or explicitly enable device-local automatic unlock; changing either policy requires the current wallet password, and automatic unlock stores an unlock-equivalent credential outside profile JSON with a clear local-device trust warning.

## HYDRA steganography

Ghost Talk pins HYDRA's public `feat: add stego` integration at commit `a8b4b317cef85edce9d0f0528bb53125ebf6333b`. Stego sends first produce HYDRA compact authenticated envelopes and then apply `hydra-stego`. The model-free **Deterministic** profile is enabled and executable in Ghost Talk. HYDRA's **Fast Unicode**, **Fast Hybrid**, and **Arithmetic** profiles require an explicitly configured deterministic local language-model backend; until Ghost Talk has such a backend configured, those choices are visibly disabled rather than silently falling back to another carrier.

## Kaspa payload policy

`80 KiB` is Ghost Talk's **logical application ceiling**, not a Kaspa consensus claim. The published `kaspa-portal 1.0.1` KSPT-v1 transaction model accepts at most 65,535 payload bytes in one physical transaction. GHST therefore reserves its 30-byte fragment header from that limit (`GHST_DATA_MAX = 65,505`) and uses multiple physical transactions only when a logical carrier crosses the real KSPT-v1 boundary. Portal's payload-aware planner and exact signed analysis remain authoritative for transaction mass and fees; Ghost Talk does not patch or duplicate those rules.

## Double-click build and run scripts

All platform launchers live under their own `scripts/<platform>/` folder. Windows launchers are `.cmd` files; Ghost Talk does not use first-party `.ps1` launchers.

| Platform | Build | Run |
| --- | --- | --- |
| Windows | `scripts/windows/build.cmd` | `scripts/windows/run.cmd` |
| Devuan Linux | `scripts/linux/build.sh` | `scripts/linux/run.sh` |
| Android from Windows | `scripts/android/build.cmd` | `scripts/android/run.cmd` |
| Android from Linux/macOS | `scripts/android/build.sh` | `scripts/android/run.sh` |
| iOS / Xcode 16.2 | `scripts/ios/build.command` | `scripts/ios/run.command` |
| WASM from Windows | `scripts/wasm/build.cmd` | `scripts/wasm/run.cmd` |
| WASM from Linux/macOS | `scripts/wasm/build.sh` | `scripts/wasm/run.sh` |

Windows and macOS `.command` launchers are intended to be double-clicked. The Devuan `.sh` desktop launchers re-open themselves in an available terminal emulator when launched graphically.

Initial setup is available through `scripts/windows/bootstrap.cmd` and `scripts/linux/bootstrap.sh`.

Windows `build.cmd`, `run.cmd`, and `run-all-tests.cmd` automatically discover Visual Studio Build Tools / `VsDevCmd.bat` and the newest installed x64 Windows SDK `RC.EXE`. You do not need to launch Ghost Talk from a Developer Command Prompt. If the Windows SDK is missing, the script fails early with the exact Visual Studio Installer components required instead of allowing `tauri-winres` to panic.

## Run every test

Double-click either:

- Windows: `scripts/run-all-tests.cmd`
- Linux/macOS: `scripts/run-all-tests.sh`

Both call the same `qa/run-all-tests.py` orchestrator. It runs all Ghost Talk first-party checks/tests, compiles the `ghost-kaspa` upstream feature against the real Cargo-registry `kaspa-portal 1.0.1` dependency, runs examples and frontend tests/build, executes HYDRA's full release gates through mutation testing, and runs coverage-guided fuzzing last. Ghost Talk does not duplicate or modify Kaspa Portal's repository-internal QA because Portal is an external package dependency, not repository-owned source. Missing required toolchains are hard failures rather than silent skips in the exhaustive runner.

Long runs can resume at any main boundary with `--from SECTION`. On Windows, for example, `scripts\run-all-tests.cmd --from examples` resumes at the standalone examples. Valid sections in order are `static`, `rust`, `examples`, `frontend`, `portal`, `hydra`, `hygiene`, and `fuzz`; the shell wrapper accepts the same arguments. Use `python qa/run-all-tests.py --list-sections` to print them. Resuming trusts the earlier sections from the previous run and continues through every later section.

For a quick architecture/static check only:

```sh
python3 qa/run-all.py --static-only
```

See `docs/spec/requirements.md`, `docs/spec/project-structure.md`, `docs/roadmap.md`, and `docs/project/implementation-status.md`.

## License

Ghost Talk first-party code is GPL-2.0-or-later. See `LICENSE` and `LICENSES.md`.

## Revision policy

Ghost Talk keeps its package/application semantic version pinned at **v0.1.0** during this development line. Repository/archive iterations use a separate monotonically increasing revision counter (`REVISION`), beginning with **r1**. Current revision: **r73**. A repository change increments only `rN`; it does not change Cargo, npm, or Tauri from `0.1.0`.
