use crate::model::{Chat, Profile, ProfilePatch};

use super::{append_chat, append_room, append_tombstone, wallet_patch};

pub(crate) fn create_room(
    profile: &Profile,
    name: String,
    mode: ghost_rooms::RoomMode,
) -> Result<(String, ProfilePatch), String> {
    if name.trim().is_empty() {
        return Err("Room name is required.".into());
    }
    let owner_hydra_id = profile
        .hydra_identity_id
        .clone()
        .ok_or_else(|| "Unlock the HYDRA identity before creating a room.".to_string())?;
    let owner_kaspa_address =
        primary_room_address(profile).ok_or_else(|| "This ID has no Kaspa address.".to_string())?;
    let room_id = crate::random_id()?;
    let policies = ghost_rooms::RoomPolicies::for_mode(mode);
    let room = ghost_rooms::RoomService::new_room(ghost_rooms::NewRoom {
        id: room_id.clone(),
        name: name.trim().to_string(),
        owner_hydra_id,
        owner_kaspa_address,
        owner_label: profile.label.clone(),
        access: policies.access,
        text_policy: policies.text,
        audio_policy: policies.audio,
        broadcast_enabled: policies.broadcast,
        pending_acceptance: false,
        revision: 1,
        members: Vec::new(),
        bans: Vec::new(),
    });
    let mut patch = ProfilePatch::new(&profile.id);
    patch.room_upsert(None, room);
    Ok((room_id, patch))
}

pub(crate) fn primary_room_address(profile: &Profile) -> Option<String> {
    profile
        .wallet
        .as_ref()?
        .public
        .receive_addresses
        .first()
        .cloned()
        .filter(|address| !address.is_empty())
}

pub(crate) fn decline_bootstrap(
    profile: &Profile,
    chat_id: &str,
) -> (String, Option<ProfilePatch>) {
    let room_name = bootstrap_room_name(profile, chat_id);
    let patch = ghost_chat::ChatService::by_id(&profile.chats, chat_id)
        .cloned()
        .map(|chat| {
            let mut patch = ProfilePatch::new(&profile.id);
            patch.chat_remove(chat);
            patch
        });
    (room_name, patch)
}

pub(crate) fn bootstrap_room_name(profile: &Profile, chat_id: &str) -> String {
    profile
        .chats
        .iter()
        .find(|chat| chat.id == chat_id)
        .and_then(|chat| chat.incoming_request())
        .and_then(|request| request.room_invite.as_ref())
        .map(|invite| invite.room_name.clone())
        .unwrap_or_else(|| "room".into())
}

pub(crate) fn accept_existing(profile: &Profile, room_id: &str) -> Option<(String, ProfilePatch)> {
    let mut updated = profile.clone();
    let (owner, room_name) = ghost_rooms::RoomService::accept(&mut updated.rooms, room_id)?;
    ghost_rooms::RoomTombstoneService::clear(&mut updated.room_tombstones, room_id, &owner);
    let mut patch = ProfilePatch::new(&profile.id);
    append_room(&mut patch, profile, &updated, room_id);
    append_tombstone(&mut patch, profile, &updated, room_id, &owner);
    Some((room_name, patch))
}

pub(crate) async fn accept_bootstrap(
    profile: &Profile,
    password: &str,
    chat_id: &str,
) -> Result<(String, ProfilePatch), String> {
    let (chat, request, invite) = bootstrap_invite_context(profile, chat_id)
        .ok_or_else(|| "Room invitation is no longer available.".to_string())?;
    let before = profile.clone();
    let mut updated = before.clone();
    let sent =
        crate::native::send_contact_accept(&updated, password, &request.signed_request_hex).await?;
    crate::model::WalletStateService::merge_progress(&mut updated.wallet, sent.public);
    let contact_id = room_contact_id(&updated, &request);
    bind_room_transport(&mut updated.chats, &chat, &request, contact_id);
    let mut patch = wallet_patch(&before, &updated);
    append_chat(&mut patch, &before, &updated, &chat.id);
    Ok((invite.room_name, patch))
}

fn bootstrap_invite_context(
    profile: &Profile,
    chat_id: &str,
) -> Option<(
    Chat,
    crate::model::IncomingRequest,
    crate::model::RoomInviteMeta,
)> {
    let chat = profile
        .chats
        .iter()
        .find(|chat| chat.id == chat_id)?
        .clone();
    let request = chat.incoming_request().cloned()?;
    let invite = request.room_invite.clone()?;
    Some((chat, request, invite))
}

fn room_contact_id(profile: &Profile, request: &crate::model::IncomingRequest) -> Option<String> {
    let peer = ghost_domain::identity::PeerBinding::new(
        request.peer_address.clone(),
        request.peer_hydra_id.clone(),
    )
    .ok()?;
    ghost_contacts::ContactService::resolve_peer_id(&profile.contacts, &peer)
}

fn bind_room_transport(
    chats: &mut crate::model::ChatStore,
    chat: &Chat,
    request: &crate::model::IncomingRequest,
    contact_id: Option<String>,
) {
    if let Ok(peer) = ghost_domain::identity::PeerBinding::new(
        request.peer_address.clone(),
        request.peer_hydra_id.clone(),
    ) {
        let _ = ghost_chat::ChatService::accept_incoming_request(
            chats,
            &chat.id,
            contact_id,
            &peer,
            request.request_id.clone(),
            true,
        );
    }
}
