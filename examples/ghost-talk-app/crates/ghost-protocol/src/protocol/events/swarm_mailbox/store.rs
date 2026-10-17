//! Bounded first-in-first-out store of no-spend swarm mail held for peers
//! that were briefly offline. Entries expire by age and the oldest are
//! dropped once the byte budget is exceeded. Replies skip packets another
//! holder replayed after the query arrived, and queries that come too soon
//! after this holder's previous reply get none.

use std::collections::{HashMap, VecDeque};

use ghost_domain::settings::SwarmRetention;

use super::{swarm_frame_id, SwarmMailQuery, SwarmMailboxPacket};

/// Minimum spacing between two replies from one holder.
pub const REPLY_INTERVAL_MS: u64 = 15_000;

struct Held {
    id: String,
    encoded: Vec<u8>,
    sent_at_secs: u64,
}

#[derive(Default)]
pub struct SwarmMailStore {
    held: VecDeque<Held>,
    /// Last time each held packet was seen on the topic (ms).
    seen_ms: HashMap<String, u64>,
    bytes: usize,
    last_reply_ms: Option<u64>,
}

impl SwarmMailStore {
    /// Record one packet seen on (or published to) the mailbox topic. True
    /// when the packet is newly held.
    pub fn hold(
        &mut self,
        packet: &SwarmMailboxPacket,
        now_ms: u64,
        policy: SwarmRetention,
    ) -> bool {
        let id = swarm_frame_id(&packet.frame);
        if let Some(seen) = self.seen_ms.get_mut(&id) {
            *seen = now_ms;
            return false;
        }
        if !policy.enabled {
            return false;
        }
        let encoded = packet.encode();
        self.bytes += encoded.len();
        self.seen_ms.insert(id.clone(), now_ms);
        self.held.push_back(Held {
            id,
            encoded,
            sent_at_secs: packet.sent_at_secs,
        });
        self.prune(now_ms, policy);
        true
    }

    /// Packets to replay for `query`, which arrived at `asked_at_ms`. Empty
    /// while retention is off or within the reply interval.
    pub fn reply(
        &mut self,
        query: SwarmMailQuery,
        asked_at_ms: u64,
        now_ms: u64,
        policy: SwarmRetention,
    ) -> Vec<Vec<u8>> {
        self.prune(now_ms, policy);
        let recent = self
            .last_reply_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < REPLY_INTERVAL_MS);
        if !policy.enabled || recent {
            return Vec::new();
        }
        self.last_reply_ms = Some(now_ms);
        let seen_ms = &mut self.seen_ms;
        self.held
            .iter()
            .filter(|held| held.sent_at_secs >= query.since_secs)
            .filter_map(|held| {
                let seen = seen_ms.get_mut(&held.id)?;
                (*seen <= asked_at_ms).then(|| {
                    *seen = now_ms;
                    held.encoded.clone()
                })
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    fn prune(&mut self, now_ms: u64, policy: SwarmRetention) {
        let oldest_kept = (now_ms / 1000).saturating_sub(policy.max_age_secs());
        let max_bytes = if policy.enabled {
            policy.max_bytes()
        } else {
            0
        };
        while let Some(front) = self.held.front() {
            if front.sent_at_secs >= oldest_kept && self.bytes <= max_bytes {
                break;
            }
            self.drop_front();
        }
    }

    fn drop_front(&mut self) {
        if let Some(dropped) = self.held.pop_front() {
            self.bytes -= dropped.encoded.len();
            self.seen_ms.remove(&dropped.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fragment;

    fn packet(seed: u8, sent_at_secs: u64) -> SwarmMailboxPacket {
        let frame = fragment(ghost_core::Id128([seed; 16]), b"KKTP:ANCHOR:{}")
            .unwrap()
            .remove(0);
        SwarmMailboxPacket::for_destination("kaspa:qpeer", frame, sent_at_secs).unwrap()
    }

    const ON: SwarmRetention = SwarmRetention {
        enabled: true,
        minutes: 10,
        megabytes: 1,
    };

    fn since(since_secs: u64) -> SwarmMailQuery {
        SwarmMailQuery { since_secs }
    }

    #[test]
    fn holds_once_and_replays_since_the_query_time() {
        let mut store = SwarmMailStore::default();
        assert!(store.hold(&packet(1, 100), 100_000, ON));
        assert!(!store.hold(&packet(1, 100), 100_500, ON));
        assert!(store.hold(&packet(2, 200), 200_000, ON));
        let replay = store.reply(since(150), 201_000, 201_000, ON);
        assert_eq!(replay, vec![packet(2, 200).encode()]);
        assert!(
            store.reply(since(0), 202_000, 202_000, ON).is_empty(),
            "rate limited"
        );
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn packets_another_holder_replayed_after_the_query_are_not_repeated() {
        let mut store = SwarmMailStore::default();
        store.hold(&packet(1, 100), 100_000, ON);
        store.hold(&packet(2, 100), 100_000, ON);
        store.hold(&packet(1, 100), 101_500, ON);
        let replay = store.reply(since(0), 101_000, 102_000, ON);
        assert_eq!(replay, vec![packet(2, 100).encode()]);
    }

    #[test]
    fn age_and_byte_limits_drop_the_oldest_first() {
        let mut store = SwarmMailStore::default();
        store.hold(&packet(1, 0), 0, ON);
        store.hold(&packet(2, 700), 700_000, ON);
        assert_eq!(
            store.len(),
            1,
            "the 10-minute window dropped the first packet"
        );
        let numbered = |index: u128| SwarmMailboxPacket {
            frame: fragment(ghost_core::Id128(index.to_le_bytes()), b"x")
                .unwrap()
                .remove(0),
            ..packet(0, 1)
        };
        let size = numbered(0).encode().len();
        let count = ON.max_bytes() / size + 2;
        let mut full = SwarmMailStore::default();
        for index in 0..count {
            full.hold(&numbered(index as u128), 1000, ON);
        }
        assert_eq!(full.len(), ON.max_bytes() / size);
    }

    #[test]
    fn disabled_retention_holds_and_replays_nothing() {
        let off = SwarmRetention {
            enabled: false,
            ..ON
        };
        let mut store = SwarmMailStore::default();
        assert!(!store.hold(&packet(1, 1), 1_000, off));
        assert!(store.is_empty());
        store.hold(&packet(1, 1), 1_000, ON);
        assert!(store.reply(since(0), 2_000, 2_000, off).is_empty());
        assert!(store.is_empty(), "turning retention off forgets held mail");
    }
}
