//! Public-room announcements for the swarm room directory: which rooms this
//! profile may advertise, and signing them with the owner's Kaspa key.

use crate::model::{Profile, Room};
use ghost_rooms::RoomAnnouncement;

/// Rooms this profile owns. `RoomAnnouncement::for_room` decides which of
/// them are public and announceable.
pub(crate) fn owned_rooms(profile: &Profile) -> Vec<Room> {
    let Some(owner) = profile.hydra_identity_id.as_deref() else {
        return Vec::new();
    };
    profile
        .rooms
        .iter()
        .filter(|room| room.owner_hydra_id == owner)
        .cloned()
        .collect()
}

pub(crate) async fn sign_announcement(
    profile: &Profile,
    password: &str,
    announcement: RoomAnnouncement,
) -> Result<RoomAnnouncement, String> {
    crate::native::sign_room_announcement(profile, password, announcement).await
}
