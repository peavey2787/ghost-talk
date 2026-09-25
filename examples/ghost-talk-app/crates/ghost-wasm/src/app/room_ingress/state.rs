use crate::model::{ChatStore, IncomingRequest, RoomBan, RoomMember, RoomTombstone};
use ghost_rooms::RoomStore;

pub(super) struct RoomStateUpdate {
    pub(super) room_id: String,
    pub(super) name: String,
    pub(super) owner_hydra_id: String,
    pub(super) owner_kaspa_address: String,
    pub(super) owner_label: String,
    pub(super) access: ghost_rooms::RoomAccess,
    pub(super) text_policy: ghost_rooms::ChannelPolicy,
    pub(super) audio_policy: ghost_rooms::ChannelPolicy,
    pub(super) broadcast_enabled: bool,
    pub(super) revision: u64,
    pub(super) members: Vec<RoomMember>,
    pub(super) bans: Vec<RoomBan>,
}

pub(super) fn handle_room_state(
    rooms: &mut RoomStore,
    tombstones: &mut Vec<RoomTombstone>,
    chats: &mut ChatStore,
    from: &str,
    update: RoomStateUpdate,
) {
    if from != update.owner_hydra_id || stale_tombstoned_room(rooms, tombstones, &update) {
        return;
    }
    let bootstrap_chat_index = accepted_room_bootstrap(chats, from, &update.room_id, &update.name);
    let bootstrap_accepted = bootstrap_chat_index.is_some();
    if bootstrap_accepted {
        ghost_rooms::RoomTombstoneService::clear(
            tombstones,
            &update.room_id,
            &update.owner_hydra_id,
        );
    }
    upsert_room_state(rooms, update, bootstrap_accepted);
    if let Some(chat_index) = bootstrap_chat_index {
        let _ = ghost_chat::ChatService::clear_incoming_request_at_index(chats, chat_index);
    }
}

fn stale_tombstoned_room(
    rooms: &RoomStore,
    tombstones: &[RoomTombstone],
    update: &RoomStateUpdate,
) -> bool {
    let exists = rooms.iter().any(|room| room.id == update.room_id);
    !exists
        && ghost_rooms::RoomTombstoneService::revision(
            tombstones,
            &update.room_id,
            &update.owner_hydra_id,
        )
        .is_some_and(|left_revision| update.revision <= left_revision)
}

fn accepted_room_bootstrap(
    chats: &ChatStore,
    from: &str,
    room_id: &str,
    room_name: &str,
) -> Option<usize> {
    chats.iter().position(|chat| {
        chat.room_transport_only()
            && chat.peer_hydra_handle() == Some(from)
            && chat
                .incoming_request()
                .is_some_and(|request| accepted_room_request(request, room_id, room_name))
    })
}

fn accepted_room_request(request: &IncomingRequest, room_id: &str, room_name: &str) -> bool {
    request.state == "accepted"
        && request
            .room_invite
            .as_ref()
            .is_some_and(|invite| invite.room_id == room_id && invite.room_name == room_name)
}

fn upsert_room_state(rooms: &mut RoomStore, update: RoomStateUpdate, bootstrap_accepted: bool) {
    if let Some(room) = ghost_rooms::RoomService::by_id(rooms, &update.room_id) {
        if room.owner_hydra_id != update.owner_hydra_id || update.revision < room.revision() {
            return;
        }
        apply_existing_room_state(rooms, update, bootstrap_accepted);
        return;
    }
    ghost_rooms::RoomService::push(
        rooms,
        ghost_rooms::RoomService::new_room(ghost_rooms::NewRoom {
            id: update.room_id,
            name: update.name,
            owner_hydra_id: update.owner_hydra_id,
            owner_kaspa_address: update.owner_kaspa_address,
            owner_label: update.owner_label,
            access: update.access,
            text_policy: update.text_policy,
            audio_policy: update.audio_policy,
            broadcast_enabled: update.broadcast_enabled,
            pending_acceptance: !bootstrap_accepted,
            revision: update.revision,
            members: update.members,
            bans: update.bans,
        }),
    );
}

fn apply_existing_room_state(rooms: &mut RoomStore, update: RoomStateUpdate, accepted: bool) {
    let id = update.room_id.clone();
    let _ = ghost_rooms::RoomService::apply_state(
        rooms,
        &id,
        ghost_rooms::RoomState {
            name: update.name,
            owner_kaspa_address: update.owner_kaspa_address,
            owner_label: update.owner_label,
            access: update.access,
            text_policy: update.text_policy,
            audio_policy: update.audio_policy,
            broadcast_enabled: update.broadcast_enabled,
            revision: update.revision,
            members: update.members,
            bans: update.bans,
            accepted,
        },
    );
}
