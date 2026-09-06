# Architecture

```text
React/Tauri/WASM UI
        ↓
   ghost-runtime
  ↙ domain/services ↘
ghost-hydra       ghost-kaspa
   ↓                  ↓
HYDRA-MSG          Kaspa Portal
```

Only `ghost-hydra` may import HYDRA crates; only `ghost-kaspa` may import Kaspa Portal. Kinesis is never a runtime dependency.

Kaspa is the durable/offline carrier and the authenticated signaling/fallback layer. The only user-facing realtime policies are Auto and Kaspa only. Auto automatically negotiates a WebRTC peer connection after the KKTP/HYDRA session becomes active and falls back to Kaspa when direct ICE connectivity is unavailable or drops; Kaspa only never opens a direct connection. Direct bytes remain HYDRA-encrypted/authenticated, so transport selection does not change identity or session security.
