use super::{
    invoke_unit, json, peer_session_binding, Chat, HydraSessionBindingProjection,
    PeerRouteRegistration, Profile,
};
use crate::model::{ChatStore, Room};

pub(crate) fn remember_room_role(
    roles: &mut std::collections::HashMap<String, Option<String>>,
    peer: String,
    role: &str,
) {
    roles
        .entry(peer)
        .and_modify(|existing| {
            if existing.as_deref() != Some(role) {
                *existing = None;
            }
        })
        .or_insert_with(|| Some(role.to_string()));
}

pub(crate) fn room_session_roles(
    rooms: &[Room],
    local_hydra_id: &str,
) -> std::collections::HashMap<String, Option<String>> {
    let mut roles = std::collections::HashMap::new();
    for room in rooms {
        if room.owner_hydra_id == local_hydra_id {
            for peer in room
                .members()
                .iter()
                .filter_map(|member| member.hydra_handle.clone())
            {
                remember_room_role(&mut roles, peer, "initiator");
            }
        } else {
            remember_room_role(&mut roles, room.owner_hydra_id.clone(), "responder");
        }
    }
    roles
}

pub(crate) fn role_from_chat_history(chat: &Chat, sid: &str) -> Option<String> {
    let accepted_request = chat
        .incoming_request()
        .is_some_and(|request| request.request_id == sid && request.state == "accepted");
    if accepted_request {
        return Some("responder".into());
    }
    chat.messages()
        .iter()
        .find(|message| {
            message.session_sid() == Some(sid) && matches!(message.direction.as_str(), "in" | "out")
        })
        .map(|message| {
            if message.direction == "out" {
                "initiator".into()
            } else {
                "responder".into()
            }
        })
}

pub(crate) fn migrate_session_roles(chats: &mut ChatStore, rooms: &[Room], local_hydra_id: &str) {
    let room_roles = room_session_roles(rooms, local_hydra_id);
    let roles: Vec<(String, String)> = chats
        .iter()
        .filter_map(|chat| {
            infer_missing_session_role(chat, &room_roles).map(|role| (chat.id.clone(), role))
        })
        .collect();
    for (chat_id, role) in roles {
        let _ = ghost_chat::HydraSessionManager::set_role(chats, &chat_id, role);
    }
}

pub(crate) fn infer_missing_session_role(
    chat: &Chat,
    room_roles: &std::collections::HashMap<String, Option<String>>,
) -> Option<String> {
    if chat.session_role().is_some() {
        return None;
    }
    let sid = chat.session_sid()?;
    if let Some(role) = role_from_chat_history(chat, sid) {
        return Some(role);
    }
    if !chat.room_transport_only() {
        return None;
    }
    let peer = chat.peer_hydra_handle()?;
    room_roles.get(peer).and_then(Clone::clone)
}

pub(crate) fn route_priority(chat: &Chat) -> u8 {
    let live = !chat.left() && !chat.peer_left();
    if live && chat.bootstrap_complete() {
        4
    } else if live && chat.session_sid().is_some() {
        3
    } else if live {
        2
    } else {
        1
    }
}

pub(crate) fn route_from_chat(chat: &Chat) -> Option<PeerRouteRegistration> {
    let contact_id = chat.peer_hydra_handle().map(str::to_owned)?;
    let kaspa_address = chat.peer_kaspa_address().map(str::to_owned)?;
    let live = !chat.left() && !chat.peer_left();
    Some(PeerRouteRegistration {
        contact_id,
        kaspa_address,
        display_name: chat.label.clone(),
        session_sid: chat.session_sid().map(str::to_owned),
        session_role: chat.session_role().map(str::to_owned),
        active: live,
        resume_required: live
            && chat.session_sid().is_some()
            && (chat.bootstrap_complete() || chat.transport_restore_pending()),
    })
}

pub(crate) fn canonical_peer_routes(
    profile: &Profile,
) -> (
    Vec<PeerRouteRegistration>,
    std::collections::HashMap<String, bool>,
) {
    let mut selected = std::collections::HashMap::<String, (u8, PeerRouteRegistration)>::new();
    let mut peer_has_live_thread = std::collections::HashMap::<String, bool>::new();
    for chat in &profile.chats {
        let Some(peer) = chat.peer_hydra_handle().map(str::to_owned) else {
            continue;
        };
        let live = !chat.left() && !chat.peer_left();
        peer_has_live_thread
            .entry(peer.clone())
            .and_modify(|value| *value |= live)
            .or_insert(live);
        let Some(route) = route_from_chat(chat) else {
            continue;
        };
        let priority = route_priority(chat);
        let replace = selected
            .get(&peer)
            .is_none_or(|(current, _)| priority > *current);
        if replace {
            selected.insert(peer, (priority, route));
        }
    }
    let routes = selected.into_values().map(|(_, route)| route).collect();
    (routes, peer_has_live_thread)
}

pub(crate) async fn register_peer_routes(
    profile_id: &str,
    identity_id: &str,
    routes: &[PeerRouteRegistration],
) -> Result<(), String> {
    invoke_unit(
        "hydra_register_peer_routes",
        json!({
            "profileId": profile_id,
            "identityId": identity_id,
            "routes": routes,
        }),
    )
    .await
}

pub(crate) async fn inspect_surviving_bindings(
    profile_id: &str,
    routes: &[PeerRouteRegistration],
) -> Result<
    (
        std::collections::HashMap<String, HydraSessionBindingProjection>,
        std::collections::HashMap<String, (Option<String>, Option<String>)>,
    ),
    String,
> {
    let mut surviving = std::collections::HashMap::new();
    let mut restoring = std::collections::HashMap::new();
    for route in routes.iter().filter(|route| route.resume_required) {
        match peer_session_binding(profile_id, &route.contact_id).await? {
            Some(binding) => {
                surviving.insert(route.contact_id.clone(), binding);
            }
            None => {
                restoring.insert(
                    route.contact_id.clone(),
                    (
                        route.session_sid().map(str::to_owned),
                        route.session_role().map(str::to_owned),
                    ),
                );
            }
        }
    }
    Ok((surviving, restoring))
}
