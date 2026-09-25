# Ghost Talk broadcast relay protocol

Ghost Talk exposes one Studio workflow on desktop, Android, iOS, and Web. Local recording is independent of live delivery: a user can record a Room broadcast locally and later use the Studio permanent-archive flow to publish that completed recording to Kaspa. Live delivery uses either native direct RTMP/RTMPS where the platform supports it or this browser-safe relay transport.

## Transport

A relay endpoint is a WebSocket URL. Production endpoints MUST use `wss://`; `ws://` is accepted only for localhost development. The same relay client runs in the shared WASM frontend on standalone Web and inside the desktop/mobile WebViews.

When the socket opens the client sends a JSON start control message:

```json
{"type":"start","version":1,"session_id":"<32 hex chars>","rtmp_server":"rtmps://host/app","rtmp_stream_key":"<secret>"}
```

The relay MUST validate the session id, protocol version, RTMP/RTMPS destination, credentials, authorization, quotas, and frame limits before publishing. Stream keys are secrets and MUST NOT be logged or included in diagnostics.

Media frames are binary and use this versioned framing:

```text
4 bytes   ASCII GTRF
1 byte    protocol version (1)
8 bytes   sequence, unsigned big-endian
8 bytes   timestamp milliseconds, unsigned big-endian
4 bytes   payload length, unsigned big-endian
N bytes   encoded Ghost voice payload
```

The payload is the encoded media body already validated by Ghost Talk's shared broadcast controller. Sequences are contiguous within a session. A relay MUST reject malformed, oversized, out-of-order, or unsupported-version frames rather than attempting to repair them silently.

To end a stream the client sends:

```json
{"type":"stop","version":1,"session_id":"<32 hex chars>"}
```

The relay then finalizes its RTMP/RTMPS publisher and closes the associated session. Relay failures are surfaced through the same Studio sink-failure result used by native recording and RTMP sinks.

## Platform parity

Desktop may publish directly to RTMP/RTMPS when no relay is configured. Android, iOS, and standalone Web use the relay for live RTMP/RTMPS delivery. All four platforms can record locally through the same Studio workflow, and a completed local recording can be explicitly archived to Kaspa through the same signed, resumable archive flow. The transport difference is an implementation boundary; the user-facing broadcast/session contract is shared.
