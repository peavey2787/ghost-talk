# Ghost Talk v1 requirements

## Accounts
- Multiple local accounts; exactly one may opt into auto-login.
- IDs create/import exactly one 24-word BIP39 mnemonic; the same seed domain-separates Kaspa and HYDRA.
- The encrypted Ghost Talk mnemonic/passphrase/account path is the authoritative derivation source for the Kaspa receive/change projection and the recoverable HYDRA identity root.
- Ghost Talk signs its own Kaspa wallet transactions from the unlocked local vault.
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
- Search friendly name, KNS, dot.k, Kaspa address, HYDRA fingerprint.
- Friend/close-friend invite, accept/decline, block/unblock.
- Unknown-first-contact policy: normal requests or auto-ignore. Auto-ignored requests are retained locally in a reviewable Ignored requests view until accepted or deleted; saved contacts are not auto-ignored.

## Name services / discovery
- `.kas` resolves to its KNS owner Kaspa address.
- `.k` is mainnet-only. The DotK directory/index supplies only a candidate owner; Ghost Talk locally derives the pinned v4 deed and accepts the address only after a fresh direct v2.0.1 wRPC query proves an unspent 1-KAS deed UTXO with the exact script and pinned registry `covenant_id`.
- Public discovery is opt-in. Publishing/registering a GTCD establishes one stable Ghost Talk registration address for that profile; a newer signed unlist/update supersedes older records. This registration address is separate from ordinary wallet receiving. The Kaspa wallet MUST use its rotating external receive chain, advance away from receive addresses that are observed as used, persist the receive cursor, and retain every observed wallet address so historical gathering always includes earlier receive/change addresses.
- GTCD binds a Kaspa owner address to HYDRA public contact material and optional public username/description/interests; KNS/DotK services are not the identity trust anchor.
- GTCD is never required for private messaging. A raw valid Kaspa address can receive a signed GTCR carrying only the sender's authenticated public bootstrap material; the exact receiving Kaspa address signs GTCA acceptance before the first queued plaintext is encrypted and delivered.
- Mapping changes are surfaced before sensitive sends, and public-profile lookup failure does not block private address-only bootstrap.

## Sync/recovery
- Restore checkpoint; subscribe/buffer live; backfill checkpoint→high-water; dedupe; replay live buffer; enter live.
- Persist raw carrier/event before durable checkpoint advancement.
- Kaspa Portal is authoritative for current DAG/wallet/live-carrier state. REST/indexer access is restricted to the explicit/import-time transaction-history gather and must never supply current balance, UTXOs, DAA, connection status, or live mailbox delivery. The monitor MUST NOT schedule background REST address-history scans.
- Kaspa connection status is driven by live node subscription lifecycle, not periodic health polling. Wallet balance/UTXO state is maintained from address-scoped `NotifyUtxosChanged` notifications and virtual DAA state from `NotifyVirtualDaaScoreChanged`. `BlockAdded` is reserved for Ghost carrier/message scanning and MUST NOT trigger wallet-state queries. After one subscription-start/reconnect UTXO baseline (required because `UtxosChanged` is a delta stream), no live wallet-state query or timer-based refresh is permitted. Wallet transaction history merges transaction IDs/addresses observed from the live UTXO stream with a separate explicit “Gather old transactions” REST operation across the deterministic receive/change lookahead; that explicit operation also runs once on wallet restore/import and must not impose an arbitrary transaction-count cap.
- Never hard-code "three days" as a protocol rule.

## Voice/radio
- 1:1 and group Opus voice.
- User-facing realtime transport choices are exactly Auto and Kaspa only.
- Auto uses p2p-net after the authenticated KKTP/HYDRA session is active and falls back to Kaspa; physical transport mechanics are never separate user preferences.
- Kaspa carries authenticated transport announcements and acknowledgements that bind the p2p PeerId and bounded dial addresses to the exact active SID/HYDRA peer. Browser/native transport negotiation stays inside p2p-net/libp2p.
- Kaspa only never establishes a direct peer connection.
- Direct application bytes use the carrier-neutral GTR1 envelope and the same authenticated HYDRA/KKTP identity on both p2p-net and Kaspa routes; carrier fallback must reuse the exact sealed bytes.
- Radio is independent room audio policy; text/audio may independently be Off, Interactive, Presenters Only.
- Audience cannot forge presenter media; Kaspa voice has per-call spend ceiling.

## Games
- Friend challenge/accept/reject; Pong v1; deterministic state/hash.
- Direct realtime route preferred; Kaspa carries authenticated setup/checkpoints/results.

## Platforms
Windows; Debian/Devuan; Android; iOS 15+ on Xcode 16.2/Sonoma; WASM/PWA. Ubuntu is not a release-test target.
## Direct contact and restart delivery

- User-facing peer entry is a Kaspa address, KNS name, or dot.k name; raw HYDRA handles are internal-only. A verified GTCD may pre-resolve public identity metadata, but otherwise GTCR/GTCA exchanges authenticated HYDRA public material directly over Kaspa without requiring a public directory listing. Saving a peer as a contact is optional and public discoverability never implies permission to bypass first-chat consent.
- HYDRA ratchet/session secrets are memory-only and are never checkpointed to profile storage. If a previously broadcast ciphertext arrives after the recipient restarted and the old ratchet is unavailable, Ghost Talk performs a fresh authenticated handshake with that verified peer.
- For ordinary durable chat, successful acceptance of every required physical carrier by Kaspa is the user-visible delivery boundary: the logical message becomes **Delivered** immediately after broadcast succeeds. Every logical message still has a stable 128-bit id so failed/ambiguous submission retries reuse the exact prepared SID/sequence/ciphertext and duplicate observations are suppressed. The signed acknowledgement retained for `pq_finish` proves bootstrap/session activation only; ordinary active-session messages do not create a second receipt transaction.
- Application delivery is bound to the authenticated KKTP `sid`: a plaintext from an older same-peer session MUST NOT be appended to a newer thread. Historical Discovery/Response/session-end records may refresh only their exact SID and MUST NOT overwrite the active SID. Restart recovery may replace a live thread SID only after the replacement recovery offer has successfully broadcast.
