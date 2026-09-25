use crate::{ChannelPolicy, Room, RoomAccess, RoomBan, RoomMember, RoomMessage, RoomStore};
use ghost_domain::reaction::merge_reactions;

#[derive(Clone, Debug, PartialEq)]
pub struct NewRoom {
    pub id: String,
    pub name: String,
    pub owner_hydra_id: String,
    pub owner_kaspa_address: String,
    pub owner_label: String,
    pub access: RoomAccess,
    pub text_policy: ChannelPolicy,
    pub audio_policy: ChannelPolicy,
    pub broadcast_enabled: bool,
    pub pending_acceptance: bool,
    pub revision: u64,
    pub members: Vec<RoomMember>,
    pub bans: Vec<RoomBan>,
}

pub struct RoomState {
    pub name: String,
    pub owner_kaspa_address: String,
    pub owner_label: String,
    pub access: RoomAccess,
    pub text_policy: ChannelPolicy,
    pub audio_policy: ChannelPolicy,
    pub broadcast_enabled: bool,
    pub revision: u64,
    pub members: Vec<RoomMember>,
    pub bans: Vec<RoomBan>,
    pub accepted: bool,
}

/// Sole mutation authority for durable room records.
pub struct RoomService;

impl RoomService {
    pub fn by_id<'a>(rooms: &'a RoomStore, id: &str) -> Option<&'a Room> {
        rooms.iter().find(|room| room.id == id)
    }

    pub fn by_index(rooms: &RoomStore, index: usize) -> Option<&Room> {
        rooms.get(index)
    }

    pub fn new_room(input: NewRoom) -> Room {
        Room {
            id: input.id,
            name: input.name,
            owner_hydra_id: input.owner_hydra_id,
            owner_kaspa_address: input.owner_kaspa_address,
            owner_label: input.owner_label,
            access: input.access,
            text_policy: input.text_policy,
            audio_policy: input.audio_policy,
            broadcast_enabled: input.broadcast_enabled,
            pending_acceptance: input.pending_acceptance,
            revision: input.revision,
            members: input.members,
            bans: input.bans,
            messages: Vec::new(),
        }
    }

    pub fn push(rooms: &mut RoomStore, room: Room) {
        rooms.push_owned(room);
    }
    pub fn remove_by_id(rooms: &mut RoomStore, id: &str) -> bool {
        let before = rooms.len();
        rooms.retain_owned(|room| room.id != id);
        rooms.len() != before
    }

    pub fn remove_if_unchanged(rooms: &mut RoomStore, before: &Room) -> bool {
        let unchanged = rooms
            .iter()
            .any(|current| current.id == before.id && current == before);
        unchanged && Self::remove_by_id(rooms, &before.id)
    }

    pub fn accept(rooms: &mut RoomStore, id: &str) -> Option<(String, String)> {
        let room = room_by_id_mut(rooms, id)?;
        room.accept();
        Some((room.owner_hydra_id.clone(), room.name.clone()))
    }

    pub fn add_message(rooms: &mut RoomStore, id: &str, message: RoomMessage) -> bool {
        let Some(room) = room_by_id_mut(rooms, id) else {
            return false;
        };
        room.add_message(message);
        true
    }

    pub fn add_message_at_index(rooms: &mut RoomStore, index: usize, message: RoomMessage) -> bool {
        let Some(room) = rooms.as_mut_slice().get_mut(index) else {
            return false;
        };
        room.add_message(message);
        true
    }

    pub fn set_message_reaction(
        rooms: &mut RoomStore,
        room_id: &str,
        message_id: &str,
        actor_id: &str,
        kind: Option<ghost_domain::reaction::ReactionKind>,
    ) -> bool {
        let Some(room) = room_by_id_mut(rooms, room_id) else {
            return false;
        };
        let Some(message) = room
            .messages
            .iter_mut()
            .find(|message| message.id == message_id)
        else {
            return false;
        };
        message.set_reaction(actor_id, kind)
    }

    pub fn add_member(
        rooms: &mut RoomStore,
        id: &str,
        member: RoomMember,
        now: f64,
    ) -> Option<Room> {
        let room = room_by_id_mut(rooms, id)?;
        room.retain_active_bans(now);
        room.add_member(member);
        room.bump_revision();
        Some(room.clone())
    }

    pub fn bind_member_peer(
        rooms: &mut RoomStore,
        room_id: &str,
        member_index: usize,
        contact_id: Option<String>,
        kaspa_address: String,
        hydra_handle: String,
    ) -> bool {
        let Some(room) = room_by_id_mut(rooms, room_id) else {
            return false;
        };
        let Some(member) = room.members.get_mut(member_index) else {
            return false;
        };
        member.bind_peer(contact_id, kaspa_address, hydra_handle);
        true
    }

    pub fn remove_member_by_hydra(rooms: &mut RoomStore, id: &str, hydra_id: &str) -> Option<Room> {
        let room = room_by_id_mut(rooms, id)?;
        room.remove_member_by_hydra(hydra_id);
        room.bump_revision();
        Some(room.clone())
    }

    pub fn remove_member_by_address(
        rooms: &mut RoomStore,
        id: &str,
        address: &str,
    ) -> Option<Room> {
        let room = room_by_id_mut(rooms, id)?;
        room.remove_member_by_address(address);
        room.bump_revision();
        Some(room.clone())
    }

    pub fn ban_member(rooms: &mut RoomStore, id: &str, ban: RoomBan) -> Option<Room> {
        let room = room_by_id_mut(rooms, id)?;
        room.ban_member(ban);
        room.bump_revision();
        Some(room.clone())
    }

    pub fn unban(rooms: &mut RoomStore, id: &str, address: &str) -> Option<Room> {
        let room = room_by_id_mut(rooms, id)?;
        room.unban_address(address);
        room.bump_revision();
        Some(room.clone())
    }

    pub fn apply_state(rooms: &mut RoomStore, id: &str, state: RoomState) -> bool {
        let Some(room) = room_by_id_mut(rooms, id) else {
            return false;
        };
        room.apply_state(state);
        true
    }

    /// Reconcile a room result while preserving newer messages and acceptance
    /// state that changed after the asynchronous command started.
    pub fn reconcile_upsert(rooms: &mut RoomStore, before: Option<&Room>, mut changed: Room) {
        let Some(latest) = rooms
            .as_mut_slice()
            .iter_mut()
            .find(|room| room.id == changed.id)
        else {
            if before.is_none() {
                rooms.push_owned(changed);
            }
            return;
        };
        let changed_messages = std::mem::take(&mut changed.messages);
        let baseline_messages = before.map(Room::messages).unwrap_or_default();
        reconcile_room_fields(latest, before, &mut changed);
        changed.messages = merge_messages(baseline_messages, changed_messages, &latest.messages);
        *latest = changed;
    }

    pub fn peer_needed_by_room(
        rooms: &RoomStore,
        local_hydra_id: Option<&str>,
        peer_hydra_id: &str,
    ) -> bool {
        let Some(local_hydra_id) = local_hydra_id else {
            return false;
        };
        rooms.iter().any(|room| {
            if room.owner_hydra_id == local_hydra_id {
                room.members
                    .iter()
                    .any(|member| member.hydra_handle.as_deref() == Some(peer_hydra_id))
            } else {
                room.owner_hydra_id == peer_hydra_id
            }
        })
    }
}

fn reconcile_room_fields(latest: &Room, before: Option<&Room>, changed: &mut Room) {
    if latest.revision > changed.revision {
        *changed = latest.clone();
        return;
    }
    let preserve_acceptance = before.is_some_and(|before| {
        latest.pending_acceptance != before.pending_acceptance
            && changed.pending_acceptance == before.pending_acceptance
    });
    if preserve_acceptance {
        changed.pending_acceptance = latest.pending_acceptance;
    }
}

fn room_by_id_mut<'a>(rooms: &'a mut RoomStore, id: &str) -> Option<&'a mut Room> {
    rooms.as_mut_slice().iter_mut().find(|room| room.id == id)
}

fn merge_messages(
    before: &[crate::RoomMessage],
    mut changed: Vec<crate::RoomMessage>,
    latest: &[crate::RoomMessage],
) -> Vec<crate::RoomMessage> {
    for concurrent in latest {
        if let Some(slot) = changed
            .iter_mut()
            .find(|message| message.id == concurrent.id)
        {
            if let Some(baseline) = before.iter().find(|message| message.id == concurrent.id) {
                slot.reactions =
                    merge_reactions(&baseline.reactions, &slot.reactions, &concurrent.reactions);
            }
        } else {
            changed.push(concurrent.clone());
        }
    }
    changed.sort_by(|left, right| left.created_at.total_cmp(&right.created_at));
    changed
}
