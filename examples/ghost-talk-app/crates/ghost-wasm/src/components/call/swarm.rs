//! Node-level p2p-net swarm state: health for the sidebar indicator (the
//! `ghost-talk-v1` swarm as a whole, separate from per-session route state)
//! and the public room directory heard on it.

use ghost_p2p::P2pNetEvent;
use std::{cell::RefCell, collections::HashSet, rc::Rc};
use yew::prelude::*;

mod room_directory;
pub use room_directory::RoomListings;
pub(crate) use room_directory::{use_room_directory_tracker, RoomDirectoryTracker};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwarmStatus {
    /// No node runs (no wallet network or the realtime route is not Auto).
    #[default]
    Disabled,
    Starting,
    Online {
        peers: usize,
    },
    Offline,
}

impl SwarmStatus {
    /// Shared indicator class: connected, connecting, reconnecting or disconnected.
    pub fn indicator_class(self) -> &'static str {
        match self {
            Self::Online { peers } if peers > 0 => "connected",
            Self::Starting | Self::Online { .. } => "connecting",
            Self::Offline => "reconnecting",
            Self::Disabled => "disconnected",
        }
    }

    pub fn title(self) -> String {
        let namespace = ghost_realtime::GHOST_TALK_DISCOVERY_APP_ID;
        match self {
            Self::Online { peers } if peers > 0 => {
                format!("Swarm {namespace} connected · {peers} peer(s)")
            }
            Self::Starting | Self::Online { .. } => {
                format!("Swarm {namespace} searching for peers")
            }
            Self::Offline => format!("Swarm {namespace} offline · reconnecting"),
            Self::Disabled => format!("Swarm {namespace} off"),
        }
    }
}

/// Tracks connected swarm peers and publishes the derived status.
#[derive(Clone)]
pub(super) struct SwarmTracker {
    status: UseStateHandle<SwarmStatus>,
    peers: Rc<RefCell<HashSet<String>>>,
}

impl SwarmTracker {
    pub(super) fn status(&self) -> SwarmStatus {
        *self.status
    }

    pub(super) fn starting(&self) {
        self.peers.borrow_mut().clear();
        self.status.set(SwarmStatus::Starting);
    }

    pub(super) fn disable(&self) {
        self.peers.borrow_mut().clear();
        self.status.set(SwarmStatus::Disabled);
    }

    pub(super) fn offline(&self) {
        self.peers.borrow_mut().clear();
        self.status.set(SwarmStatus::Offline);
    }

    pub(super) fn apply(&self, event: &P2pNetEvent) {
        match event {
            P2pNetEvent::PeerConnected(peer) => {
                self.peers.borrow_mut().insert(peer.clone());
            }
            P2pNetEvent::PeerDisconnected(peer) => {
                self.peers.borrow_mut().remove(peer);
            }
            P2pNetEvent::Offline => return self.offline(),
            P2pNetEvent::Online | P2pNetEvent::LocalBindingChanged(_) => {}
        }
        let peers = self.peers.borrow().len();
        self.status.set(SwarmStatus::Online { peers });
    }
}

#[hook]
pub(super) fn use_swarm_tracker() -> SwarmTracker {
    SwarmTracker {
        status: use_state(SwarmStatus::default),
        peers: use_mut_ref(HashSet::new),
    }
}

#[cfg(test)]
mod tests {
    use super::SwarmStatus;

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn indicator_classes_follow_peer_reachability() {
        assert_eq!(SwarmStatus::Disabled.indicator_class(), "disconnected");
        assert_eq!(SwarmStatus::Starting.indicator_class(), "connecting");
        assert_eq!(
            SwarmStatus::Online { peers: 0 }.indicator_class(),
            "connecting"
        );
        assert_eq!(
            SwarmStatus::Online { peers: 2 }.indicator_class(),
            "connected"
        );
        assert_eq!(SwarmStatus::Offline.indicator_class(), "reconnecting");
        assert!(SwarmStatus::Online { peers: 2 }.title().contains("2 peer"));
    }
}
