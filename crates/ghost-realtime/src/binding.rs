use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::session_topic;

pub const MAX_BOUND_SESSIONS: usize = 4096;

/// This node's transport identity and currently advertised dial addresses.
/// Also accepts p2p-net's snake_case `LocalNodeBinding` shape.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalP2pBinding {
    #[serde(alias = "peer_id")]
    pub peer_id: String,
    #[serde(default, alias = "dial_addresses")]
    pub dial_addresses: Vec<String>,
}

impl LocalP2pBinding {
    /// Every announced dial address must name its destination PeerId so the
    /// remote side can check it against the authenticated binding.
    pub fn with_peer_suffixes(mut self) -> Self {
        let suffix = format!("/p2p/{}", self.peer_id);
        for address in &mut self.dial_addresses {
            if !address.ends_with(&suffix) {
                *address = format!("{}{suffix}", address.trim_end_matches('/'));
            }
        }
        self
    }
}

/// An authenticated session's binding from SID + HYDRA identity to the peer's
/// transport PeerId. A PeerId is never itself a Ghost identity.
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
    /// Bind a SID; rebinding an active SID to a different peer fails closed.
    pub fn bind(&mut self, binding: SessionPeerBinding) -> Result<(), String> {
        if binding.peer_id.trim().is_empty() {
            return Err("p2p binding peer id is empty".into());
        }
        if let Some(existing) = self.by_sid.get(&binding.sid) {
            if existing.hydra_id != binding.hydra_id || existing.peer_id != binding.peer_id {
                return Err("active SID is already bound to a different authenticated peer".into());
            }
            return Ok(());
        }
        if self.by_sid.len() >= MAX_BOUND_SESSIONS {
            return Err("p2p session binding limit reached".into());
        }
        self.by_sid.insert(binding.sid, binding);
        Ok(())
    }

    /// True only when a packet's SID, HYDRA sender, and transport source all
    /// match one authenticated binding.
    pub fn verify_source(&self, sid: &[u8; 16], hydra_id: &[u8; 32], peer_id: &str) -> bool {
        self.by_sid
            .get(sid)
            .is_some_and(|binding| binding.hydra_id == *hydra_id && binding.peer_id == peer_id)
    }

    pub fn get(&self, sid: &[u8; 16]) -> Option<&SessionPeerBinding> {
        self.by_sid.get(sid)
    }

    pub fn contains_peer(&self, peer_id: &str) -> bool {
        self.by_sid
            .values()
            .any(|binding| binding.peer_id == peer_id)
    }

    pub fn retire(&mut self, sid: &[u8; 16]) -> bool {
        self.by_sid.remove(sid).is_some()
    }

    pub fn topic(&self, sid: &[u8; 16]) -> Option<String> {
        self.by_sid.contains_key(sid).then(|| session_topic(sid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(sid: u8, peer: &str) -> SessionPeerBinding {
        SessionPeerBinding {
            sid: [sid; 16],
            hydra_id: [2; 32],
            peer_id: peer.into(),
        }
    }

    #[test]
    fn binding_is_fail_closed_for_conflicting_transport_identity() {
        let mut bindings = SessionPeerBindings::default();
        bindings.bind(binding(1, "peer-a")).unwrap();
        assert!(bindings.bind(binding(1, "peer-a")).is_ok());
        assert!(bindings.bind(binding(1, "peer-b")).is_err());
        assert!(bindings.bind(binding(2, " ")).is_err());
        assert!(bindings.verify_source(&[1; 16], &[2; 32], "peer-a"));
        assert!(!bindings.verify_source(&[1; 16], &[2; 32], "peer-b"));
        assert!(!bindings.verify_source(&[1; 16], &[3; 32], "peer-a"));
        assert!(bindings.contains_peer("peer-a"));
        assert_eq!(bindings.get(&[1; 16]).unwrap().peer_id, "peer-a");
        assert_eq!(bindings.topic(&[1; 16]), Some(session_topic(&[1; 16])));
        assert_eq!(bindings.topic(&[9; 16]), None);
    }

    #[test]
    fn retired_sid_cannot_validate_transport_source() {
        let mut bindings = SessionPeerBindings::default();
        bindings.bind(binding(1, "peer-a")).unwrap();
        assert!(bindings.retire(&[1; 16]));
        assert!(!bindings.retire(&[1; 16]));
        assert!(!bindings.verify_source(&[1; 16], &[2; 32], "peer-a"));
        assert!(!bindings.contains_peer("peer-a"));
    }

    #[test]
    fn binding_table_is_bounded() {
        let mut bindings = SessionPeerBindings::default();
        for index in 0..MAX_BOUND_SESSIONS {
            let mut sid = [0u8; 16];
            sid[..8].copy_from_slice(&(index as u64).to_le_bytes());
            let entry = SessionPeerBinding {
                sid,
                hydra_id: [1; 32],
                peer_id: "peer".into(),
            };
            bindings.bind(entry).unwrap();
        }
        assert!(bindings.bind(binding(0xff, "peer")).is_err());
    }

    #[test]
    fn local_binding_accepts_p2p_net_shape_and_names_its_peer() {
        let upstream: LocalP2pBinding =
            serde_json::from_str(r#"{"peer_id":"me","dial_addresses":["/a/","/b/p2p/me"]}"#)
                .unwrap();
        let normalized = upstream.with_peer_suffixes();
        assert_eq!(normalized.dial_addresses, vec!["/a/p2p/me", "/b/p2p/me"]);
        let bare: LocalP2pBinding = serde_json::from_str(r#"{"peer_id":"me"}"#).unwrap();
        assert!(bare.dial_addresses.is_empty());
    }

    #[test]
    fn bindings_serialize_with_camel_case_fields() {
        let local = LocalP2pBinding {
            peer_id: "p".into(),
            dial_addresses: vec!["/a".into()],
        };
        let json = serde_json::to_string(&local).unwrap();
        assert!(json.contains("peerId") && json.contains("dialAddresses"));
    }
}
