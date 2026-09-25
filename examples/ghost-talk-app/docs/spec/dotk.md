# DotK (`.k`) integration

Ghost Talk treats DotK as a verified mainnet name-to-Kaspa-address resolver. A DotK name is **not** a HYDRA identity and never replaces `PeerBinding`; after a `.k` name resolves, normal GTCD/GTCR/GTCA and HYDRA authentication continue using the resolved Kaspa address.

## Why an index is still needed

The public DotK directory at `https://api.dotk.name/v1` is an **untrusted name index**, not an ownership authority. For a query such as `alice.k`, it supplies the candidate current owner fields needed to locate the deed.

Ghost Talk cannot currently remove the indexing step for arbitrary name lookup. DotK v4 derives the deed state/address from both the normalized name and the current owner. Given only `alice.k`, the client knows the name key but does not yet know the owner bytes/type needed to derive the deed address. Kaspa v2.0.1 exposes covenant IDs on UTXO entries and supports UTXO lookup by address, but the standard RPC does not provide a global name/covenant-key lookup that maps a DotK name directly to its current deed.

This means the **public** directory is replaceable, but the indexing function is not. `GHOST_DOTK_DIRECTORY_URL` may point at a self-hosted chain-derived DotK index. If Kaspa/DotK later exposes a direct name-key/covenant lookup, Ghost Talk can remove the directory/index dependency entirely.

A malicious, stale, or incomplete index cannot make Ghost Talk accept a false owner: the index only gives Ghost Talk `ownerType + owner` as a candidate to verify. It can cause lookup failure or omission, and it can observe queried names, so operators who want availability/privacy independence should use a self-hosted index. Directory-supplied address/deed/registry metadata is ignored.

## Resolution contract

A literal `.k` input always receives a fresh chain proof. Contact aliases and the public-profile cache may display a previously resolved DotK name, but they are not allowed to satisfy an explicit `name.k` lookup from cached application state.

`ghost-names::DotkResolver` performs the following mainnet-only flow:

1. Normalize the complete `.k` name using the DotK 1–32 ASCII lowercase letter/digit/internal-hyphen grammar.
2. Read the candidate owner row from `https://api.dotk.name/v1/names/{bare-name}` (or the configured replacement index). Treat it as an untrusted hint.
3. Read only `ownerType` and `owner` from that row. Ghost Talk does not trust or consume directory-supplied payment addresses, deed addresses, registry IDs, or scripts.
4. Locally derive the v4 owner payment address, ACTIVE deed state, deed P2SH address, and exact script public key from the pinned deployment, normalized requested name, and candidate owner. Covenant-owned names deliberately have no payment/peer address and are rejected.
5. Obtain the active mainnet wRPC endpoint from the shared `NodeSession`/Kaspa gateway and connect with the pinned Rusty-Kaspa v2.0.1 client.
6. Query the locally derived deed address through typed `get_utxos_by_addresses`. A candidate must be non-coinbase, exactly 1 KAS, use the locally derived script, and carry the locally pinned DotK registry `covenant_id` in the consensus UTXO entry.
7. Query that deed address again and require the exact same outpoint to remain live. Only then return the locally derived Kaspa owner address.

Any disagreement, malformed response, unsupported owner scheme, spent/stale deed, foreign covenant, unavailable node/index, or timeout fails closed without selecting a recipient.

## Pinned deployment and Kaspa SDK

The reviewed DotK mainnet deployment is embedded at `crates/ghost-names/src/dotk_mainnet.json`. The architecture gate pins that file by SHA-256:

```text
0696babcb5c47a965088597afe849147fa0ab2681a3972af8c9451a12e48b382
```

Ghost Talk pins the Rusty-Kaspa wRPC client plus DotK address/script derivation dependencies to release commit `cfafeb4c093fa37a303f1b9f19c58f986b870ce3` (v2.0.1). This is the Toccata-capable release whose RPC UTXO model exposes `covenant_id`.

DotK verification no longer uses the Kaspire HTTP proof bridge. The live covenant proof is made directly against the endpoint selected by Ghost Talk's shared Kaspa session. Wallet planning, broadcast, subscriptions, carrier observation, and DotK proof therefore share the same configured network endpoint authority, while each subsystem keeps its own scoped client/lifecycle.

## User-facing behavior

`.k` is accepted anywhere the shared recipient control accepts a KNS name: start chat, add contact, Discover start-chat, room invite, and Send KAS. Testnet rejects `.k` names and asks for a direct Kaspa address. Resolved contacts/chats retain `dotk_name` only as display metadata; authenticated association continues to require the canonical Kaspa-address + HYDRA `PeerBinding`.
