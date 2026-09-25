# Upstream integration

## HYDRA

HYDRA remains an independent repository. The reference application consumes `hydra-msg`, `hydra-core`, `hydra-stego`, and `hydra-group` directly from `https://github.com/peavey2787/hydra-msg.git`, with every direct crate pinned to the same exact commit:

```text
a8b4b317cef85edce9d0f0528bb53125ebf6333b
```

`ghost-hydra` is the application facade around those upstream APIs. HYDRA source is not vendored into Ghost Talk.

## Kaspa Portal

Kaspa Portal is not copied into this repository. The nested application workspace imports exactly `kaspa-portal = "1.0.1"`. `ghost-kaspa` owns application-specific endpoint/failover and KKTP/GHST composition around the published Portal API; it does not patch Portal source.

## Ghost Talk voice SDK

The public `crates/ghost-talk` package owns media stream IDs, sequencing, reorder/gap policy, limits, wire chunks, decoded-duration scheduling and playback state.

Browser-facing access to the public media wire API is Rust compiled to WebAssembly from repository `crates/ghost-talk-wasm/`. The reference application's frontend is likewise Rust/WASM under `crates/ghost-wasm/`.

The application owns only communication layers around complete encoded `VoiceChunk` bytes.
