//! Game/app peer routing over a p2p-net node (Kaspa Kinesis sessions).

use std::collections::HashMap;

use p2p_net::{NodeHandle, PeerId};

/// Bidirectional map between application peer ids and transport PeerIds. A
/// binding can never be silently re-pointed at another identity.
#[derive(Default)]
pub struct PeerDirectory {
    game_to_transport: HashMap<u64, PeerId>,
    transport_to_game: HashMap<PeerId, u64>,
}

impl PeerDirectory {
    pub fn bind(&mut self, game_peer_id: u64, transport_peer_id: PeerId) -> Result<(), String> {
        if self
            .game_to_transport
            .get(&game_peer_id)
            .is_some_and(|existing| existing != &transport_peer_id)
        {
            return Err(format!(
                "game peer {game_peer_id} attempted to change transport identity"
            ));
        }
        if self
            .transport_to_game
            .get(&transport_peer_id)
            .is_some_and(|existing| *existing != game_peer_id)
        {
            return Err("transport peer attempted to claim multiple game identities".into());
        }
        self.game_to_transport
            .insert(game_peer_id, transport_peer_id);
        self.transport_to_game
            .insert(transport_peer_id, game_peer_id);
        Ok(())
    }

    pub fn transport(&self, game_peer_id: u64) -> Option<&PeerId> {
        self.game_to_transport.get(&game_peer_id)
    }

    pub fn game(&self, transport_peer_id: &PeerId) -> Option<u64> {
        self.transport_to_game.get(transport_peer_id).copied()
    }

    pub fn remove_transport(&mut self, transport_peer_id: &PeerId) -> Option<u64> {
        let game = self.transport_to_game.remove(transport_peer_id)?;
        self.game_to_transport.remove(&game);
        Some(game)
    }

    pub fn transport_entries(&self) -> impl Iterator<Item = (&PeerId, &u64)> {
        self.transport_to_game.iter()
    }
}

/// Send an addressed application message to a bound game peer.
pub async fn send_to_game_peer(
    node: &NodeHandle,
    peers: &PeerDirectory,
    game_peer_id: u64,
    topic: &str,
    payload: Vec<u8>,
) -> Result<(), String> {
    let transport = peers
        .transport(game_peer_id)
        .copied()
        .ok_or_else(|| format!("game peer {game_peer_id} is not connected"))?;
    node.send_message(transport, topic, payload)
        .await
        .map_err(|error| format!("Ghost Talk realtime send failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_binds_each_identity_once() {
        let (a, b) = (PeerId::random(), PeerId::random());
        let mut peers = PeerDirectory::default();
        peers.bind(2, a).unwrap();
        peers.bind(2, a).unwrap();
        assert!(peers.bind(2, b).is_err());
        assert!(peers.bind(3, a).is_err());
        assert_eq!(peers.transport(2), Some(&a));
        assert_eq!(peers.game(&a), Some(2));
        assert_eq!(peers.transport_entries().count(), 1);
        assert_eq!(peers.remove_transport(&a), Some(2));
        assert_eq!(peers.remove_transport(&a), None);
        assert_eq!(peers.transport(2), None);
        peers.bind(3, a).unwrap();
        assert_eq!(peers.game(&a), Some(3));
    }
}
