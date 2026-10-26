//! Room media fan-out: a member sends to the owner, and the owner relays
//! to every other member over each member's authenticated session.

use crate::{
    controllers::call::RealtimeSender,
    live_voice::RoomMediaPacket,
    model::{Profile, RealtimeControl, Room},
};

use super::Medium;

/// A packet may arrive straight from its sender or relayed by the owner.
pub(super) fn valid_hop(
    profile: &Profile,
    control: &RealtimeControl,
    room: &Room,
    speaker: &str,
) -> bool {
    let peer = profile
        .chats
        .iter()
        .find(|chat| chat.id == control.chat_id)
        .and_then(|chat| chat.peer_hydra_handle());
    peer == Some(speaker) || peer == Some(room.owner_hydra_id.as_str())
}

pub(super) fn relay_if_owner(
    profile: &Profile,
    room: &Room,
    packet: &RoomMediaPacket,
    medium: Medium,
    realtime: &RealtimeSender,
) {
    if profile.hydra_identity_id.as_deref() != Some(room.owner_hydra_id.as_str())
        || packet.speaker_hydra_id == room.owner_hydra_id
    {
        return;
    }
    if let Ok(body) = medium.encode(packet) {
        fan(
            profile,
            room,
            Some(&packet.speaker_hydra_id),
            medium,
            &body,
            realtime,
        );
    }
}

pub(super) fn fan(
    profile: &Profile,
    room: &Room,
    exclude_hydra: Option<&str>,
    medium: Medium,
    body: &str,
    realtime: &RealtimeSender,
) {
    if profile.hydra_identity_id.as_deref() == Some(room.owner_hydra_id.as_str()) {
        for member in room.members() {
            if member.hydra_handle.as_deref() == exclude_hydra {
                continue;
            }
            if let Some(chat_id) = realtime_chat_for(profile, member.hydra_handle.as_deref()) {
                enqueue(realtime, medium, chat_id, body);
            }
        }
    } else if let Some(chat_id) = realtime_chat_for(profile, Some(&room.owner_hydra_id)) {
        enqueue(realtime, medium, chat_id, body);
    }
}

fn enqueue(realtime: &RealtimeSender, medium: Medium, chat_id: String, body: &str) {
    match medium {
        Medium::Voice => realtime.enqueue(chat_id, body.to_owned()),
        Medium::Video => realtime.enqueue_video(chat_id, body.to_owned()),
    }
}

fn realtime_chat_for(profile: &Profile, hydra: Option<&str>) -> Option<String> {
    let hydra = hydra?;
    profile
        .chats
        .iter()
        .find(|chat| {
            chat.peer_hydra_handle() == Some(hydra)
                && chat.bootstrap_complete()
                && !chat.left()
                && !chat.peer_left()
        })
        .map(|chat| chat.id.clone())
}
