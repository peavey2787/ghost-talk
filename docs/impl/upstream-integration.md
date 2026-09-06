# Upstream integration

HYDRA source is pinned under `external/hydra-msg`. `ghost-hydra` exposes Ghost-owned DTOs and, with feature `upstream`, uses `Hydra::send`, `send_compact`, `receive`, `receive_compact`, handshakes, and `hydra-stego::Stego`.

Kaspa Portal is **not copied into this repository**. The workspace imports exactly `kaspa-portal = "1.0.1"` from Cargo. `ghost-kaspa` is a thin consumer facade around Portal's public SDK and does not modify Portal source, constants, codecs, resolver policy, or transport implementation. Portal 1.0.1 owns the persistent native wRPC connection, `Send`-safe request futures, request/notification multiplexing, reconnect plus subscription replay, decoded `BlockAdded` notifications, payload-aware transaction planning, and authoritative mass/fee analysis. Ghost Talk therefore has no private Portal executor thread, raw Kaspa WebSocket/notification decoder, or duplicate mass estimator.

Ghost Talk retains only application-layer responsibilities: endpoint selection/failover policy, decoded-event fanout, KKTP/HYDRA processing, and GHST fragmentation when a logical carrier exceeds KSPT v1's 65,535-byte physical payload limit. Because GHST reserves a 30-byte header, each physical fragment carries at most 65,505 logical bytes. The 80 KiB Ghost Talk limit is a logical application ceiling and may therefore require two physical transactions; it does not alter Portal or Kaspa limits.

Kinesis Voice is a source/reference extraction only. It provides microphone/Opus/chunk/jitter/reference behavior and its 32 KiB conservative constant; no Kinesis JavaScript or legacy voice crypto is linked into Ghost Talk.
