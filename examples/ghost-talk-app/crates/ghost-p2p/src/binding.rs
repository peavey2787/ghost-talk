use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const MAX_BOUND_SESSIONS: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalP2pBinding {
    pub peer_id: String,
    pub dial_addresses: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPeerBinding {
    pub sid: [u8; 16],
    pub hydra_id: [u8; 32],
    pub peer_id: String,
}

#[derive(Default)]
pub struct SessionPeerBindings {
    by_sid: HashMap<[u8; 16], SessionPeerBinding>,
}

impl SessionPeerBindings {
    pub fn bind(&mut self, binding: SessionPeerBinding) -> Result<(), String> {
        if binding.peer_id.trim().is_empty() {
            return Err("p2p binding peer id is empty".into());
        }
        if !self.by_sid.contains_key(&binding.sid) && self.by_sid.len() >= MAX_BOUND_SESSIONS {
            return Err("p2p session binding limit reached".into());
        }
        if let Some(existing) = self.by_sid.get(&binding.sid) {
            if existing.hydra_id != binding.hydra_id || existing.peer_id != binding.peer_id {
                return Err("active SID is already bound to a different authenticated peer".into());
            }
            return Ok(());
        }
        self.by_sid.insert(binding.sid, binding);
        Ok(())
    }

    pub fn verify_source(&self, sid: &[u8; 16], hydra_id: &[u8; 32], peer_id: &str) -> bool {
        self.by_sid.get(sid).is_some_and(|binding| {
            binding.hydra_id == *hydra_id && binding.peer_id == peer_id
        })
    }

    pub fn get(&self, sid: &[u8; 16]) -> Option<&SessionPeerBinding> {
        self.by_sid.get(sid)
    }

    pub fn contains_peer(&self, peer_id: &str) -> bool {
        self.by_sid.values().any(|binding| binding.peer_id == peer_id)
    }

    pub fn retire(&mut self, sid: &[u8; 16]) -> bool {
        self.by_sid.remove(sid).is_some()
    }

    pub fn topic(&self, sid: &[u8; 16]) -> Option<String> {
        self.by_sid
            .contains_key(sid)
            .then(|| ghost_protocol::session_topic(sid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(peer: &str) -> SessionPeerBinding {
        SessionPeerBinding {
            sid: [1; 16],
            hydra_id: [2; 32],
            peer_id: peer.into(),
        }
    }

    #[test]
    fn binding_is_fail_closed_for_conflicting_transport_identity() {
        let mut bindings = SessionPeerBindings::default();
        bindings.bind(binding("peer-a")).unwrap();
        assert!(bindings.bind(binding("peer-a")).is_ok());
        assert!(bindings.bind(binding("peer-b")).is_err());
        assert!(bindings.verify_source(&[1; 16], &[2; 32], "peer-a"));
        assert!(!bindings.verify_source(&[1; 16], &[2; 32], "peer-b"));
    }

    #[test]
    fn retired_sid_cannot_validate_transport_source() {
        let mut bindings = SessionPeerBindings::default();
        bindings.bind(binding("peer-a")).unwrap();
        assert!(bindings.retire(&[1; 16]));
        assert!(!bindings.verify_source(&[1; 16], &[2; 32], "peer-a"));
    }
}
