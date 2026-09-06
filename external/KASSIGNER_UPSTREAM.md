# Ghost Talk KasSigner SDK integration

Ghost Talk r56 retains the current supplied KasSigner consumer package integrated in r53 as
`@kassigner/sdk` **2.0.0** under `external/kassigner-sdk/pkg/`. KasSigner keeps its
first-party semantic version pinned at `2.0.0`; the refreshed package is therefore
identified by its exact artifact hashes rather than by inventing a new semantic version.

## Current supplied package

| File | SHA-256 |
| --- | --- |
| `kassigner_sdk_bg.wasm` | `3eb4cc606adc3c7042a5d84507927c5dd32a6a4f3a16cc79f17beace9bb27ca9` |
| `kassigner_sdk.js` | `4bd0f2e145347a4baa61602923a8961097e0ce653ecdb2e98c97daa5109cf017` |
| `kassigner_sdk.d.ts` | `18c2a3fd76481ab09bf9e2efb4eda02e176adb16e90ca587a8cac8b9d957d7d1` |
| `kassigner_sdk_bg.wasm.d.ts` | `03062fb3c8e8e73f5327503a4b2626c1d0629d2354211bfe91b1a53aeb30050b` |
| `package.json` | `6d6ea66bcb1ef37f22832272f03b95d044a3e7c4acb195c2a93a0594eb543b5e` |

The package advertises SDK version `2.0.0`, protocol version `2.0.0`, compact
KSPT generation 4, 32 inputs, 8 outputs, 64 QR frames, 91-byte multi-frame
fragments, 134-byte single-frame payloads, QR frame version 1, and session-bound
QR framing. `qa/checks/kassigner-sdk-probe.mjs` loads the actual supplied WASM and
verifies those values plus the required protocol exports on every QA run where
Node.js is available.

Ghost Talk's Tauri backend uses the vendored Rust `kassigner-sdk`,
`kassigner-protocol`, `shared-signer`, and `rqrr-nostd` sources. Their public
SDK/protocol version and hardware capability constants are checked against the
supplied consumer package. The generated package does not contain Rust source,
so this document does not claim that the existing Rust source tree is a
byte-for-byte source snapshot of the refreshed WASM build.

Ghost Talk owns wallet/network policy, UTXO selection, fees, outputs, change,
Kaspa Portal access, and broadcast. KasSigner owns only pairing/signing protocol,
QR session handling, signed-response validation/merge, and finalization.
