# Payload and mass policy

Ghost Talk defines an 80 KiB **logical application ceiling** for its own carrier planning. This is not a Kaspa consensus transaction-payload claim. The imported `kaspa-portal = "1.0.1"` crate keeps compact KSPT at version 1, whose `u16` payload-length field permits at most **65,535 payload bytes per physical transaction**.

Kaspa Portal is imported unmodified from Cargo. Ghost Talk does not change Portal payload constants, transaction storage, KSPT encodings, parser widths, mass rules, or fee rules. GHST reserves 30 bytes of each physical transaction payload for fragment metadata, leaving **65,505 logical bytes per GHST transaction**. Logical carriers above that physical boundary are split into bounded GHST frames and reassembled before HYDRA/KKTP decoding; normal 4 KiB/32 KiB carriers remain single-transaction payloads.

Portal 1.0.1's payload-aware planner is the transaction-planning authority. After software or KasSigner signing, Ghost Talk asks Portal to analyze the exact signed/finalizable PSKB using Portal's node-aware normal fee estimate and central compute/transient/storage-mass policy before broadcast. Ghost Talk does not maintain a second approximate mass formula.

Kinesis Voice's 32 KiB chunk constant is preserved in `external/kaspa-kinesis-voice-reference/` as evidence of an application-level conservative limit, not a consensus limit.
