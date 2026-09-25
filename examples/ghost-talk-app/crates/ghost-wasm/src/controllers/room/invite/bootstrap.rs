use crate::model::{Message, Profile, ResolvedGhostPeer, Room, RoomMember};

pub(super) struct RoomBootstrap<'a> {
    pub(super) room: &'a Room,
    pub(super) member: &'a RoomMember,
    pub(super) peer: &'a ResolvedGhostPeer,
    pub(super) chat_id: String,
    pub(super) wire_id: String,
    pub(super) payload: String,
}

pub(super) async fn start_room_bootstrap(
    mut profile: Profile,
    password: &str,
    bootstrap: RoomBootstrap<'_>,
) -> Result<(Profile, String), String> {
    let chat_id = bootstrap.chat_id.clone();
    let request_id = crate::random_id()?;
    if !ghost_chat::HydraSessionManager::begin(
        &mut profile.chats,
        &bootstrap.chat_id,
        Some(request_id.clone()),
        Some("initiator".into()),
    ) {
        return Err("Room transport chat disappeared.".into());
    }
    record_room_bootstrap_message(&mut profile.chats, &bootstrap, &request_id)?;
    let sent = crate::native::send_room_contact_request(
        &profile,
        password,
        &bootstrap.peer.kaspa_address,
        &request_id,
        &bootstrap.room.id,
        &bootstrap.room.name,
    )
    .await?;
    crate::model::WalletStateService::merge_progress(&mut profile.wallet, sent.public);
    let _ = ghost_chat::ChatService::mark_message_sent_by_contact_request(
        &mut profile.chats,
        &chat_id,
        &request_id,
        sent.transaction_id,
    );
    Ok((
        profile,
        format!(
            "Room invite sent to {}. It will join after secure acceptance.",
            bootstrap.member.label
        ),
    ))
}

fn record_room_bootstrap_message(
    chats: &mut crate::model::ChatStore,
    bootstrap: &RoomBootstrap<'_>,
    request_id: &str,
) -> Result<(), String> {
    let message = Message {
        id: bootstrap.wire_id.clone(),
        wire_id: Some(bootstrap.wire_id.clone()),
        session_sid: Some(request_id.to_owned()),
        direction: "out".into(),
        body: bootstrap.payload.clone(),
        created_at: crate::now_ms(),
        send_state: Some("sending".into()),
        pending: true,
        pending_stage: Some("request".into()),
        contact_request_id: Some(request_id.to_owned()),
        ..Default::default()
    };
    ghost_chat::ChatService::record_message(chats, &bootstrap.chat_id, message)
        .then_some(())
        .ok_or_else(|| "Room transport chat disappeared.".to_string())
}
