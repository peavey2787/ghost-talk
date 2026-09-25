#![forbid(unsafe_code)]

use std::collections::{HashSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutePreference { Auto, KaspaOnly }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeCarrier { P2pNet, Kaspa }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ReplayKey {
    pub sender_hydra_id: [u8; 32],
    pub sid: [u8; 16],
    pub message_id: [u8; 16],
}

impl ReplayKey {
    pub fn from_gtr1(envelope: &ghost_protocol::Gtr1Envelope) -> Self {
        Self {
            sender_hydra_id: envelope.sender_hydra_id,
            sid: envelope.sid,
            message_id: envelope.message_id,
        }
    }
}

pub struct ReplayWindow {
    capacity: usize,
    seen: HashSet<ReplayKey>,
    order: VecDeque<ReplayKey>,
}

impl ReplayWindow {
    pub fn new(capacity: usize) -> Self {
        Self { capacity: capacity.max(1), seen: HashSet::new(), order: VecDeque::new() }
    }

    pub fn accept(&mut self, key: ReplayKey) -> bool {
        if self.seen.contains(&key) { return false; }
        self.seen.insert(key.clone());
        self.order.push_back(key);
        while self.order.len() > self.capacity {
            if let Some(expired) = self.order.pop_front() { self.seen.remove(&expired); }
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteDecision {
    pub primary: RealtimeCarrier,
    pub fallback: Option<RealtimeCarrier>,
}

pub fn route(preference: RoutePreference, p2p_available: bool) -> RouteDecision {
    match (preference, p2p_available) {
        (RoutePreference::Auto, true) => RouteDecision { primary: RealtimeCarrier::P2pNet, fallback: Some(RealtimeCarrier::Kaspa) },
        _ => RouteDecision { primary: RealtimeCarrier::Kaspa, fallback: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(message: u8) -> ReplayKey {
        ReplayKey { sender_hydra_id: [1; 32], sid: [2; 16], message_id: [message; 16] }
    }

    #[test]
    fn replay_window_deduplicates_across_carriers() {
        let mut window = ReplayWindow::new(2);
        assert!(window.accept(key(1)));
        assert!(!window.accept(key(1)));
        assert!(window.accept(key(2)));
        assert!(window.accept(key(3)));
        assert!(window.accept(key(1)));
    }

    #[test]
    fn auto_prefers_p2p_and_falls_back_to_kaspa() {
        assert_eq!(route(RoutePreference::Auto, true), RouteDecision { primary: RealtimeCarrier::P2pNet, fallback: Some(RealtimeCarrier::Kaspa) });
        assert_eq!(route(RoutePreference::Auto, false).primary, RealtimeCarrier::Kaspa);
        assert_eq!(route(RoutePreference::KaspaOnly, true).primary, RealtimeCarrier::Kaspa);
    }
}
