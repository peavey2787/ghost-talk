//! Public-room announcements for the room directory: which rooms this
//! profile may advertise, signing them for the swarm, and publishing them
//! on Kaspa.

use crate::controllers::account::{broadcast_patch, BroadcastPatch};
use crate::model::{BroadcastResult, Profile, Room};
use crate::native::{room_announcement, AnnouncementCommand};
use ghost_rooms::{RoomAnnouncement, KASPA_LISTING_LIFETIME_MS};

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
    room_announcement(AnnouncementCommand::Sign, profile, password, announcement).await
}

/// Publish a public room's listing as a Kaspa payload to the owner's stable
/// address. Kaspa listings last a day and reach peers that are not on the
/// swarm; the payload costs the usual network fee.
pub(crate) async fn announce_on_kaspa(
    profile: &Profile,
    password: &str,
    room: &Room,
) -> Result<BroadcastPatch, String> {
    let now = crate::now_ms().max(0.0) as u64;
    let announcement = RoomAnnouncement::for_room(room, now, KASPA_LISTING_LIFETIME_MS)
        .ok_or_else(|| "Only public rooms can be announced".to_string())?;
    let result: BroadcastResult = room_announcement(
        AnnouncementCommand::Publish,
        profile,
        password,
        announcement,
    )
    .await?;
    Ok(broadcast_patch(profile, result))
}
