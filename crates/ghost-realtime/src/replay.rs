use std::collections::{HashSet, VecDeque};

use crate::ReplayIdentity;

/// Bounded cross-carrier replay filter. A logical message is accepted once no
/// matter how many carriers (p2p-net, Kaspa fallback) deliver it.
pub struct ReplayWindow {
    capacity: usize,
    seen: HashSet<ReplayIdentity>,
    order: VecDeque<ReplayIdentity>,
}

impl ReplayWindow {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            seen: HashSet::new(),
            order: VecDeque::new(),
        }
    }

    /// Returns `true` the first time an identity is observed.
    pub fn accept(&mut self, identity: ReplayIdentity) -> bool {
        if !self.seen.insert(identity) {
            return false;
        }
        self.order.push_back(identity);
        while self.order.len() > self.capacity {
            if let Some(expired) = self.order.pop_front() {
                self.seen.remove(&expired);
            }
        }
        true
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(message: u8) -> ReplayIdentity {
        ReplayIdentity {
            sender: [1; 32],
            message_id: [message; 16],
            session_id: [2; 16],
        }
    }

    #[test]
    fn deduplicates_across_carriers_within_the_bound() {
        let mut window = ReplayWindow::new(2);
        assert!(window.is_empty());
        assert!(window.accept(identity(1)));
        assert!(!window.accept(identity(1)));
        assert!(window.accept(identity(2)));
        assert!(window.accept(identity(3)));
        assert_eq!(window.len(), 2);
        // The oldest identity aged out of the bounded window.
        assert!(window.accept(identity(1)));
    }

    #[test]
    fn zero_capacity_still_remembers_the_latest_identity() {
        let mut window = ReplayWindow::new(0);
        assert!(window.accept(identity(1)));
        assert!(!window.accept(identity(1)));
    }
}
