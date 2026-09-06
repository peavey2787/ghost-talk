# Voice and Radio

Ghost Talk exposes exactly two user-facing realtime transport policies:

- **Auto** — after the authenticated KKTP chat session is active, automatically attempt a direct WebRTC connection. Kaspa carries the SID-bound authenticated offer/answer signaling and remains the fallback if direct ICE connectivity is unavailable or later drops.
- **Kaspa only** — never establish a direct peer connection; realtime control/audio remains on the authenticated Kaspa carrier.

WebRTC is an implementation detail of Auto, not a separate user preference. DCUtR/libp2p is not a selectable route in this build. A future direct mechanism may be added behind Auto without creating another user-facing transport mode. Completed non-trickle ICE SDP is exchanged over Kaspa so Ghost Talk does not create a transaction for every ICE candidate.

All direct application bytes are encrypted/authenticated by the already-established HYDRA session before they enter the WebRTC data channel. WebRTC therefore transports opaque ciphertext and does not replace KKTP/HYDRA identity or session authentication.

Microphone audio is encoded as independently decodable Opus windows. The next recorder window starts immediately after the previous window is finalized and **before** that previous window is encrypted or sent, so Kaspa/WebRTC carrier latency cannot create holes in microphone capture. Each transmitted window carries a monotonically increasing call-local sequence number. The receiver holds an initial jitter buffer, restores sequence order, decodes each complete Opus/WebM window to PCM, and schedules decoded buffers back-to-back on one `AudioContext` timeline so DOM media-element startup/`ended` latency cannot insert artificial gaps between windows. Auto's WebRTC data channel is ordered and reliable; the sequence/jitter layer remains authoritative for Kaspa/DAG fallback ordering.

Live audio/control is deliberately outside the strict durable KKTP text sequence: losing a realtime window must not create a sequence gap that blocks durable chat. The SID-bound `GTR1` carrier supplies the Kaspa fallback and signaling path; direct WebRTC media uses the same active peer/SID through native `hydra_seal_direct` / `hydra_open_direct`.

A call uses Request / Accept / Decline / Hang-up / Mute lifecycle. The caller does not enter connected state until the exact-session Accept arrives. The callee does not send microphone media before accepting. Auto may move media onto the already-negotiated direct channel at any time; if that channel fails, subsequent windows fall back to Kaspa without ending the authenticated chat session.

Kaspa-fallback media windows do not create chat bubbles or per-window delivery acknowledgements. Each such window is still a mailbox transaction and therefore incurs the normal Kaspa transfer/network-fee policy. Direct WebRTC media does not create a Kaspa transaction per audio window.

Room text and audio have independent policies: Off / Interactive / Presenters Only. Thus radio, webinar/presenter, interactive voice and listener-chat are combinations of one Room abstraction, not duplicated subsystems.
