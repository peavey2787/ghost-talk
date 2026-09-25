use super::state::{decode_room_wire, native, room_state_wire};
use crate::controllers::room::encode_room_wire;
use crate::model::{
    ChatStore, ContactStore, HydraContactAcceptedProjection, MailboxSendResult, Profile,
    RoomMember, RoomWire, WalletRecord,
};
use ghost_contacts::ContactService;
use ghost_domain::identity::{optional_binding_matches, PeerBinding};
use ghost_rooms::RoomStore;

pub(super) async fn handle_contact_accepted(
    mut profile: Profile,
    password: &str,
    accepted: HydraContactAcceptedProjection,
) -> Result<Profile, String> {
    let Some((chat_index, message_index)) = contact_accept_target(&profile, &accepted.request_id)
    else {
        return Ok(profile);
    };
    let matching_contact_id =
        update_accepted_contact(&profile.chats, &mut profile.contacts, chat_index, &accepted);
    update_room_member_routes(
        &profile.chats,
        &mut profile.rooms,
        chat_index,
        &accepted,
        matching_contact_id.as_deref(),
    );
    prepare_accepted_chat(&mut profile.chats, chat_index, message_index, &accepted);
    let body = accepted_queued_body(
        &mut profile.chats,
        &profile.rooms,
        chat_index,
        message_index,
    )?;
    let message_id = queued_message_id(&profile.chats, chat_index, message_index);
    let sent = native::send_mailbox_message(
        &profile,
        password,
        &accepted.peer_hydra_id,
        &accepted.peer_address,
        &body,
        &message_id,
    )
    .await?;
    apply_contact_accept_send(
        &mut profile.wallet,
        &mut profile.chats,
        chat_index,
        message_index,
        sent,
    );
    Ok(profile)
}

fn contact_accept_target(profile: &Profile, request_id: &str) -> Option<(usize, usize)> {
    profile
        .chats
        .iter()
        .enumerate()
        .find_map(|(chat_index, chat)| {
            chat.messages()
                .iter()
                .position(|message| message.contact_request_id.as_deref() == Some(request_id))
                .map(|message_index| (chat_index, message_index))
        })
}

fn update_accepted_contact(
    chats: &ChatStore,
    contacts: &mut ContactStore,
    chat_index: usize,
    accepted: &HydraContactAcceptedProjection,
) -> Option<String> {
    // A contact-accept is correlated to the exact outbound request/thread. Do
    // not rediscover its owner globally from only an address or HYDRA id.
    let contact_id = chats[chat_index]
        .contact_id()
        .map(str::to_owned)
        .or_else(|| {
            PeerBinding::new(
                accepted.peer_address.clone(),
                accepted.peer_hydra_id.clone(),
            )
            .ok()
            .and_then(|peer| ContactService::resolve_peer_id(contacts, &peer))
        });
    if let Some(id) = contact_id.as_deref() {
        if let Ok(peer) = PeerBinding::new(
            accepted.peer_address.clone(),
            accepted.peer_hydra_id.clone(),
        ) {
            let _ = ghost_contacts::ContactService::bind_peer_by_id(contacts, id, &peer);
        }
    }
    contact_id
}

fn update_room_member_routes(
    chats: &ChatStore,
    rooms: &mut RoomStore,
    chat_index: usize,
    accepted: &HydraContactAcceptedProjection,
    matching_contact_id: Option<&str>,
) {
    let prior_address = chats[chat_index].peer_kaspa_address().map(str::to_owned);
    let matches: Vec<(String, usize, Option<String>)> = rooms
        .iter()
        .flat_map(|room| {
            room.members()
                .iter()
                .enumerate()
                .filter(|&(_, member)| {
                    room_member_route_matches(member, prior_address.as_deref(), accepted)
                })
                .map(|(member_index, member)| {
                    let contact_id = member
                        .contact_id
                        .clone()
                        .or_else(|| matching_contact_id.map(str::to_string));
                    (room.id.clone(), member_index, contact_id)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    for (room_id, member_index, contact_id) in matches {
        let _ = ghost_rooms::RoomService::bind_member_peer(
            rooms,
            &room_id,
            member_index,
            contact_id,
            accepted.peer_address.clone(),
            accepted.peer_hydra_id.clone(),
        );
    }
}

fn room_member_route_matches(
    member: &RoomMember,
    prior_address: Option<&str>,
    accepted: &HydraContactAcceptedProjection,
) -> bool {
    let accepted_peer = PeerBinding::new(
        accepted.peer_address.clone(),
        accepted.peer_hydra_id.clone(),
    )
    .ok();
    prior_address.is_some_and(|address| member.kaspa_address.eq_ignore_ascii_case(address))
        || accepted_peer.as_ref().is_some_and(|peer| {
            optional_binding_matches(
                Some(member.kaspa_address.as_str()),
                member.hydra_handle.as_deref(),
                peer,
            )
        })
}

fn prepare_accepted_chat(
    chats: &mut ChatStore,
    chat_index: usize,
    message_index: usize,
    accepted: &HydraContactAcceptedProjection,
) {
    if let Ok(peer) = PeerBinding::new(
        accepted.peer_address.clone(),
        accepted.peer_hydra_id.clone(),
    ) {
        let _ = ghost_chat::ChatService::bind_peer_at_index(chats, chat_index, &peer);
    }
    let _ = ghost_chat::HydraSessionManager::begin_at_index(
        chats,
        chat_index,
        Some(accepted.request_id.clone()),
        Some("initiator".into()),
    );
    let _ =
        ghost_chat::ChatService::set_message_handshake_ready_at(chats, chat_index, message_index);
}

fn accepted_queued_body(
    chats: &mut ChatStore,
    rooms: &RoomStore,
    chat_index: usize,
    message_index: usize,
) -> Result<String, String> {
    let queued = chats[chat_index].messages()[message_index].body.clone();
    let body = refresh_room_state_payload(rooms, &queued)?;
    let _ = ghost_chat::ChatService::replace_message_body_at(
        chats,
        chat_index,
        message_index,
        body.clone(),
    );
    Ok(body)
}

fn refresh_room_state_payload(rooms: &RoomStore, queued: &str) -> Result<String, String> {
    let Some(RoomWire::State { room_id, .. }) = decode_room_wire(queued) else {
        return Ok(queued.to_string());
    };
    let Some(room) = rooms.iter().find(|room| room.id == room_id) else {
        return Ok(queued.to_string());
    };
    encode_room_wire(&room_state_wire(room))
}

fn queued_message_id(chats: &ChatStore, chat_index: usize, message_index: usize) -> String {
    let message = &chats[chat_index].messages()[message_index];
    message
        .wire_id
        .clone()
        .unwrap_or_else(|| message.id.clone())
}

fn apply_contact_accept_send(
    wallet: &mut Option<WalletRecord>,
    chats: &mut ChatStore,
    chat_index: usize,
    message_index: usize,
    sent: MailboxSendResult,
) {
    crate::model::WalletStateService::merge_progress(wallet, sent.public.clone());
    let txid = (!sent.pending_handshake).then_some(sent.transaction_id);
    let _ = ghost_chat::ChatService::apply_message_send_result_at(
        chats,
        chat_index,
        message_index,
        txid,
        sent.pending_handshake,
        sent.pending_id,
        sent.pending_handshake.then_some("handshake".into()),
    );
}
