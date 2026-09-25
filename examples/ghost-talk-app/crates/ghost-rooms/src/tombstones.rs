use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomTombstone {
    pub room_id: String,
    pub owner_hydra_id: String,
    #[serde(default)]
    pub revision: u64,
}

impl RoomTombstone {
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

pub struct RoomTombstoneService;

impl RoomTombstoneService {
    pub fn revision(
        tombstones: &[RoomTombstone],
        room_id: &str,
        owner_hydra_id: &str,
    ) -> Option<u64> {
        tombstones
            .iter()
            .find(|item| item.room_id == room_id && item.owner_hydra_id == owner_hydra_id)
            .map(|item| item.revision)
    }

    pub fn record(
        tombstones: &mut Vec<RoomTombstone>,
        room_id: &str,
        owner_hydra_id: &str,
        revision: u64,
    ) {
        if let Some(existing) = tombstones
            .iter_mut()
            .find(|item| item.room_id == room_id && item.owner_hydra_id == owner_hydra_id)
        {
            existing.revision = existing.revision.max(revision);
        } else {
            tombstones.push(RoomTombstone {
                room_id: room_id.to_owned(),
                owner_hydra_id: owner_hydra_id.to_owned(),
                revision,
            });
        }
        const MAX_ROOM_TOMBSTONES: usize = 256;
        if tombstones.len() > MAX_ROOM_TOMBSTONES {
            tombstones.drain(0..tombstones.len() - MAX_ROOM_TOMBSTONES);
        }
    }

    pub fn clear(tombstones: &mut Vec<RoomTombstone>, room_id: &str, owner_hydra_id: &str) {
        tombstones.retain(|item| item.room_id != room_id || item.owner_hydra_id != owner_hydra_id);
    }
}
