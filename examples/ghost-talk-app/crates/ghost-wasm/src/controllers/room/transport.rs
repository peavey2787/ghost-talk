use crate::model::{ChatStore, Profile, RoomMember, RoomWire, WalletProjection};

pub(crate) fn remove_pending_room_payloads(
    chats: &mut ChatStore,
    room_id: &str,
    only_address: Option<&str>,
) -> Vec<String> {
    let removals = chats
        .iter()
        .filter_map(|chat| {
            if only_address.is_some_and(|address| {
                !chat
                    .peer_kaspa_address()
                    .is_some_and(|peer| peer.eq_ignore_ascii_case(address))
            }) {
                return None;
            }
            let ids = chat
                .messages()
                .iter()
                .filter(|message| {
                    message.pending
                        && message.direction == "out"
                        && matches!(
                            decode_room_wire(&message.body),
                            Some(RoomWire::State { room_id: ref queued_room_id, .. })
                                if queued_room_id == room_id
                        )
                })
                .map(|message| message.id.clone())
                .collect::<Vec<_>>();
            (!ids.is_empty()).then(|| (chat.id.clone(), ids))
        })
        .collect::<Vec<_>>();
    let affected = removals
        .iter()
        .map(|(chat_id, _)| chat_id.clone())
        .collect::<Vec<_>>();
    for (chat_id, message_ids) in removals {
        let _ = ghost_chat::ChatService::remove_messages_by_id(chats, &chat_id, &message_ids);
    }
    affected
}

pub(crate) fn room_member_session_established(profile: &Profile, member: &RoomMember) -> bool {
    let Some(handle) = member.hydra_handle.as_deref() else {
        return false;
    };
    profile.chats.iter().any(|chat| {
        chat.peer_hydra_handle() == Some(handle)
            && chat.bootstrap_complete()
            && !chat.left()
            && !chat.peer_left()
    })
}

pub(crate) fn room_member_session_restoring(profile: &Profile, member: &RoomMember) -> bool {
    let Some(handle) = member.hydra_handle.as_deref() else {
        return false;
    };
    profile.chats.iter().any(|chat| {
        chat.peer_hydra_handle() == Some(handle)
            && chat.transport_restore_pending()
            && !chat.left()
            && !chat.peer_left()
    })
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RoomBroadcastResult {
    pub(crate) failures: Vec<String>,
    pub(crate) wallet_progress: Option<WalletProjection>,
}

impl RoomBroadcastResult {
    pub(crate) fn apply_wallet(self, wallet: &mut Option<crate::model::WalletRecord>) {
        if let Some(progress) = self.wallet_progress {
            crate::model::WalletStateService::merge_progress(wallet, progress);
        }
    }
}

pub(crate) async fn send_room_wire_to_member(
    profile: &Profile,
    password: &str,
    member: &RoomMember,
    wire: &RoomWire,
) -> Result<WalletProjection, String> {
    let handle = member.hydra_handle.as_deref().ok_or_else(|| {
        format!(
            "{} has not established a secure HYDRA identity yet",
            member.label
        )
    })?;
    send_room_wire_to_route(
        profile,
        password,
        &member.label,
        handle,
        &member.kaspa_address,
        wire,
    )
    .await
}

pub(crate) async fn send_room_wire_to_route(
    profile: &Profile,
    password: &str,
    label: &str,
    hydra_handle: &str,
    kaspa_address: &str,
    wire: &RoomWire,
) -> Result<WalletProjection, String> {
    let active_route = profile.chats.iter().find(|chat| {
        chat.peer_hydra_handle() == Some(hydra_handle)
            && chat.bootstrap_complete()
            && !chat.left()
            && !chat.peer_left()
    });
    let Some(active_route) = active_route else {
        if profile.chats.iter().any(|chat| {
            chat.peer_hydra_handle() == Some(hydra_handle)
                && chat.transport_restore_pending()
                && !chat.left()
                && !chat.peer_left()
        }) {
            return Err(format!(
                "Secure session with {label} is restoring after restart. Try again in a moment."
            ));
        }
        return Err(format!(
            "Secure session with {label} is not established yet."
        ));
    };
    let destination = active_route
        .peer_kaspa_address()
        .filter(|value| !value.is_empty())
        .unwrap_or(kaspa_address);
    let payload = encode_room_wire(wire)?;
    let message_id = crate::random_id()?;
    let sent = crate::native::send_mailbox_message_existing_session(
        profile,
        password,
        hydra_handle,
        destination,
        &payload,
        &message_id,
    )
    .await?;
    if sent.pending_handshake {
        return Err(format!("Secure session with {label} is not active yet."));
    }
    Ok(sent.public)
}

pub(crate) async fn broadcast_room_wire_to_members(
    profile: &Profile,
    password: &str,
    members: &[RoomMember],
    wire: &RoomWire,
    exclude_hydra: Option<&str>,
) -> RoomBroadcastResult {
    let mut working = profile.clone();
    let mut failures = Vec::new();
    let mut changed = false;
    for member in members {
        if member
            .hydra_handle
            .as_deref()
            .is_some_and(|handle| Some(handle) == exclude_hydra)
        {
            continue;
        }
        if !room_member_session_established(&working, member) {
            failures.push(member.label.clone());
            continue;
        }
        match send_room_wire_to_member(&working, password, member, wire).await {
            Ok(progress) => {
                changed |=
                    crate::model::WalletStateService::merge_progress(&mut working.wallet, progress);
            }
            Err(_) => failures.push(member.label.clone()),
        }
    }
    RoomBroadcastResult {
        failures,
        wallet_progress: changed
            .then(|| working.wallet.as_ref().map(|wallet| wallet.public.clone()))
            .flatten(),
    }
}

pub(crate) fn room_state_wire(room: &crate::model::Room) -> RoomWire {
    RoomWire::State {
        room_id: room.id.clone(),
        name: room.name.clone(),
        owner_hydra_id: room.owner_hydra_id.clone(),
        owner_kaspa_address: room.owner_kaspa_address.clone(),
        owner_label: room.owner_label.clone(),
        access: room.access(),
        text_policy: room.text_policy(),
        audio_policy: room.audio_policy(),
        broadcast_enabled: room.broadcast_enabled(),
        revision: room.revision(),
        members: room.members().to_vec(),
        bans: room.bans().to_vec(),
    }
}

pub(crate) fn encode_room_wire(wire: &RoomWire) -> Result<String, String> {
    serde_json::to_string(wire)
        .map(|json| format!("{}{}", super::ROOM_WIRE_PREFIX, json))
        .map_err(|error| error.to_string())
}

pub(crate) fn decode_room_wire(body: &str) -> Option<RoomWire> {
    body.strip_prefix(super::ROOM_WIRE_PREFIX)
        .and_then(|json| serde_json::from_str(json).ok())
}
