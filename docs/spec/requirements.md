# Ghost Talk v1 requirements

## Accounts
- Multiple local accounts; exactly one may opt into auto-login.
- Software-backed IDs create/import exactly one 24-word BIP39 mnemonic; the same seed domain-separates Kaspa and HYDRA. KasSigner-backed IDs may instead import an account kpub as a watch-only financial wallet; Ghost Talk then creates/restores one separate 24-word messaging/mailbox root because a public kpub cannot securely derive a private HYDRA identity. The KasSigner wallet recovery remains external to Ghost Talk.
- KasSigner-backed financial wallets never store a Kaspa private key in Ghost Talk. Manual financial sends/consolidation require the matching hardware signer; chat carrier/control traffic uses the separate low-value software mailbox wallet so messaging does not require a hardware scan per packet.
- For a normal Ghost-created wallet signed with KasSigner, the encrypted Ghost Talk mnemonic/passphrase/account path is the authoritative derivation source; Ghost Talk must derive the receive/change projection natively immediately before planning and must not require a separately imported kpub. For an explicitly watch-only KasSigner-backed profile, the stored account kpub plus paired fingerprint remains authoritative. In both modes, any Portal output derivation hint is translated only after the PSKT output script matches the exact authoritative branch/index.
- Importing a KasSigner account kpub is not sufficient proof of ownership. Before a KasSigner-backed ID can complete creation or open as current, Ghost Talk requires the matching hardware to sign a domain-separated readable ownership challenge bound to the network, account fingerprint, Ghost Talk profile, HYDRA identity, nonce and creation time; Ghost Talk verifies the BIP340 signature against the imported account kpub.
- Named Kaspa derivation presets, plus validated custom derivation path.
- Password/PIN unlock with memory-hard KDF and failure backoff. Manual wallet + mailbox unlock is the default; settings may require per-send re-entry or explicitly enable device-local automatic unlock, and changing either policy requires the current wallet password. OS hardware/key-store wrapping should be used where available.
- Devuan must not require systemd or one specific desktop keyring.

## Messaging/social
- HYDRA-encrypted 1:1 messages, replies, reactions and receipts.
- Kaspa durable/offline carrier with idempotent reassembly.
- HYDRA secure groups and Broadcast presenter/moderator/audience role semantics.
- Local `***` privacy mask is distinct from wire steganography `S`.
- HYDRA stego profiles: Off, Deterministic, Fast Unicode, Fast Hybrid, Arithmetic (model profiles capability-gated).
- Contacts: Close Friend, Friend, Acquaintance, Anonymous; blocked is a separate local authority bit.
- Search friendly name, KNS, Kaspa address, HYDRA fingerprint.
- Friend/close-friend invite, accept/decline, block/unblock.
- Unknown-first-contact policy: normal requests or auto-ignore. Auto-ignored requests are retained locally in a reviewable Ignored requests view until accepted or deleted; saved contacts are not auto-ignored.

## KNS/discovery
- `.kas` resolves to owner Kaspa address.
- Public discovery is opt-in. Address history locates the newest valid owner-signed GTCD descriptor on the stable Ghost Talk receive index; a newer signed unlist/update supersedes older records.
- GTCD binds a Kaspa owner address to HYDRA public contact material and optional public username/description/interests; KNS API is not the identity trust anchor.
- GTCD is never required for private messaging. A raw valid Kaspa address can receive a signed GTCR carrying only the sender's authenticated public bootstrap material; the exact receiving Kaspa address signs GTCA acceptance before the first queued plaintext is encrypted and delivered.
- Mapping changes are surfaced before sensitive sends, and public-profile lookup failure does not block private address-only bootstrap.

## Sync/recovery
- Restore checkpoint; subscribe/buffer live; backfill checkpoint→high-water; dedupe; replay live buffer; enter live.
- Persist raw carrier/event before durable checkpoint advancement.
- Kaspa Portal is authoritative for current DAG/wallet/live-carrier state. REST/indexer access is archival-only for transactions proven older than 48 hours and must never supply current balance, UTXOs, DAA, connection status, or live mailbox delivery.
- Never hard-code "three days" as a protocol rule.

## Voice/radio
- 1:1 and group Opus voice.
- User-facing realtime transport choices are exactly Auto and Kaspa only.
- Auto establishes a post-handshake WebRTC connection when possible and falls back to Kaspa; WebRTC is not a separate user preference.
- Kaspa carries exact-SID authenticated offer/answer signaling; completed SDP avoids per-candidate signaling transactions.
- Kaspa only never establishes a direct peer connection.
- Direct application bytes are HYDRA-encrypted/authenticated before WebRTC sees them; the same authenticated session identity applies on both routes.
- Radio is independent room audio policy; text/audio may independently be Off, Interactive, Presenters Only.
- Audience cannot forge presenter media; Kaspa voice has per-call spend ceiling.

## Games
- Friend challenge/accept/reject; Pong v1; deterministic state/hash.
- Direct realtime route preferred; Kaspa carries authenticated setup/checkpoints/results.

## Platforms
Windows; Debian/Devuan; Android; iOS 15+ on Xcode 16.2/Sonoma; WASM/PWA. Ubuntu is not a release-test target.
## Direct contact and restart delivery

- User-facing peer entry is a Kaspa address or KNS name; raw HYDRA handles are internal-only. A verified GTCD may pre-resolve public identity metadata, but otherwise GTCR/GTCA exchanges authenticated HYDRA public material directly over Kaspa without requiring a public directory listing. Saving a peer as a contact is optional and public discoverability never implies permission to bypass first-chat consent.
- HYDRA ratchet/session secrets are memory-only and are never checkpointed to profile storage. If a previously broadcast ciphertext arrives after the recipient restarted and the old ratchet is unavailable, Ghost Talk performs a fresh authenticated handshake with that verified peer.
- For ordinary durable chat, successful acceptance of every required physical carrier by Kaspa is the user-visible delivery boundary: the logical message becomes **Delivered** immediately after broadcast succeeds. Every logical message still has a stable 128-bit id so failed/ambiguous submission retries reuse the exact prepared SID/sequence/ciphertext and duplicate observations are suppressed. The signed acknowledgement retained for `pq_finish` proves bootstrap/session activation only; ordinary active-session messages do not create a second receipt transaction.
- Application delivery is bound to the authenticated KKTP `sid`: a plaintext from an older same-peer session MUST NOT be appended to a newer thread. Historical Discovery/Response/session-end records may refresh only their exact SID and MUST NOT overwrite the active SID. Restart recovery may replace a live thread SID only after the replacement recovery offer has successfully broadcast.
