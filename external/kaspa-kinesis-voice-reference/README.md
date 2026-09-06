# Kaspa Kinesis Voice reference extraction

Reference-only extraction from the pinned `KaspaKinesis-voice.zip` archive (SHA-256 `b9352dd0ee7e1454373e76950c56bd640b2de15652791440eefc7069df44ca28`).

Included only to preserve the working microphone/Opus/chunk/jitter/Kaspa-voice ideas inspected during Ghost Talk design. No file in this directory is a Cargo/npm runtime dependency of Ghost Talk. In particular, its legacy session cryptography is not reused.

The upstream voice configuration contains `MAX_PAYLOAD_BYTES = 32 * 1024`; this is an application-level conservative Kinesis constant, not a Kaspa consensus payload ceiling.
