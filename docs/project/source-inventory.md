# Source inventory

- HYDRA-MSG archive SHA-256: `d4b31b873e1c4a7700188a972caec4a3ce98807146b3fd17b0593e10995b4b51`; runtime owner `ghost-hydra`.
- Kaspa Portal: external Cargo dependency `kaspa-portal = "1.0.1"`; runtime integration owner `ghost-kaspa`. No Kaspa Portal source snapshot is stored in Ghost Talk. Portal owns the persistent wRPC transport, native `Send` futures, BlockAdded wire protocol/decoding, reconnect/subscription replay, payload-aware planning, and mass/fee analysis. Ghost Talk retains only endpoint policy, application event fanout, and GHST fragmentation above the KSPT-v1 65,535-byte physical payload boundary.
- Kaspa Kinesis Voice archive SHA-256: `b9352dd0ee7e1454373e76950c56bd640b2de15652791440eefc7069df44ca28`; reference-only extraction.

Ghost Talk does not patch or shadow Kaspa Portal. Cargo resolves the published package and the upstream-feature Rust compile gates validate Ghost Talk against that dependency boundary. See `docs/spec/payload-mass.md`.
