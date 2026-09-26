# ghost-realtime

Single owner of the Ghost Talk realtime carrier contract, shared by the Ghost
Talk application and Kaspa Kinesis:

- **GTR1** framing (`Gtr1Envelope`, `encode_gtr1`, `decode_gtr1`, `GTR1_MAGIC`)
  and the cross-carrier `ReplayIdentity` / bounded `ReplayWindow`;
- canonical p2p-net topics: `CONTROL_TOPIC`, `REALTIME_TOPIC`, `VOICE_TOPIC`,
  `room_topic(..)` and the SID-derived `session_topic(..)`;
- the `Automatic` (p2p-net, then Kaspa) / `KaspaOnly` route policy;
- authenticated SID/HYDRA/PeerId session bindings and the `GhostP2pTransport`
  contract implemented by platform adapters;
- with the default `p2p-net` feature: `PeerDirectory`, `send_to_game_peer` and
  `ghost_node_config` for hosts that run a p2p-net `NodeHandle`.

```toml
[dependencies]
ghost-realtime = { git = "https://github.com/peavey2787/ghost-talk.git" }
```

Consumers that only need the wire contract (GTR1, topics, route policy) can
depend on it with `default-features = false`; the default `p2p-net` feature adds
the node policy and peer helpers used by native and browser hosts.
