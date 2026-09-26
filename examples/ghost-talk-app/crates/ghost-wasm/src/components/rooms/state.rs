use super::render::render_rooms_page;
pub(crate) use crate::{
    components::recipient_input::{resolve_recipient_target, RecipientInput},
    format_message_timestamp,
    model::{Chat, Profile, ProfilePatch, Room, RoomBan, RoomMember, RoomMessage},
    now_ms,
};
pub(crate) use wasm_bindgen_futures::spawn_local;
pub(crate) use web_sys::HtmlSelectElement;
use yew::prelude::*;

mod invite_flow;
pub(crate) use invite_flow::{invite_member_callback, room_owner_matches};
mod actions;
pub(crate) use actions::build_room_actions;

#[derive(Properties, PartialEq)]
pub struct RoomsProps {
    pub profile: Profile,
    pub password: String,
    pub on_update: Callback<ProfilePatch>,
    pub on_add_contact: Callback<String>,
}

#[derive(Clone)]
pub(crate) struct RoomsUi {
    pub(crate) selected_id: UseStateHandle<String>,
    pub(crate) name: UseStateHandle<String>,
    pub(crate) mode: UseStateHandle<String>,
    pub(crate) invite: UseStateHandle<String>,
    pub(crate) draft: UseStateHandle<String>,
    pub(crate) status: UseStateHandle<String>,
    pub(crate) busy: UseStateHandle<bool>,
    pub(crate) ban_target: UseStateHandle<String>,
    pub(crate) ban_amount: UseStateHandle<String>,
    pub(crate) ban_unit: UseStateHandle<String>,
    pub(crate) accepted_bootstrap_room: UseStateHandle<Option<(String, String)>>,
    pub(crate) voice: Option<crate::components::call::RoomVoiceContext>,
}

#[derive(Clone)]
pub(crate) struct RoomActions {
    pub(crate) create: Callback<MouseEvent>,
    pub(crate) invite_member: Callback<MouseEvent>,
    pub(crate) send_message: Callback<MouseEvent>,
    pub(crate) leave_room: Callback<MouseEvent>,
    pub(crate) disband_room: Callback<MouseEvent>,
    pub(crate) kick_member: Callback<String>,
    pub(crate) confirm_ban: Callback<MouseEvent>,
    pub(crate) unban_member: Callback<String>,
    pub(crate) accept_bootstrap_invite: Callback<String>,
    pub(crate) decline_bootstrap_invite: Callback<String>,
    pub(crate) accept_existing_room: Callback<String>,
}

#[component(RoomsView)]
pub fn rooms_view(props: &RoomsProps) -> Html {
    let ui = RoomsUi {
        selected_id: use_state(|| {
            props
                .profile
                .rooms
                .first()
                .map(|room| room.id.clone())
                .unwrap_or_default()
        }),
        name: use_state(String::new),
        mode: use_state(|| "private".to_string()),
        invite: use_state(String::new),
        draft: use_state(String::new),
        status: use_state(String::new),
        busy: use_state(|| false),
        ban_target: use_state(String::new),
        ban_amount: use_state(|| "60".to_string()),
        ban_unit: use_state(|| "minutes".to_string()),
        accepted_bootstrap_room: use_state(|| None::<(String, String)>),
        voice: use_context::<crate::components::call::RoomVoiceContext>(),
    };

    use_persisted_acceptance_effect(&props.profile, &ui);
    use_acceptance_completion_effect(&props.profile, &ui);
    use_selection_effect(&props.profile, &ui);

    let selected = selected_room(&props.profile, &ui.selected_id);
    let actions = build_room_actions(props, &ui, selected.clone());
    render_rooms_page(props, &ui, selected, &actions)
}

#[hook]
pub(crate) fn use_persisted_acceptance_effect(profile: &Profile, ui: &RoomsUi) {
    let persisted_accepted = profile.chats.iter().find_map(|chat| {
        let request = chat.incoming_request()?;
        let invite = request.room_invite.as_ref()?;
        (request.state == "accepted").then(|| (invite.room_id.clone(), invite.room_name.clone()))
    });
    let accepted = ui.accepted_bootstrap_room.clone();
    let status = ui.status.clone();
    use_effect_with(persisted_accepted, move |persisted| {
        if accepted.is_none() {
            if let Some((room_id, room_name)) = persisted.as_ref() {
                accepted.set(Some((room_id.clone(), room_name.clone())));
                if status.is_empty() {
                    status.set(format!("Invitation to {room_name} accepted. Establishing the encrypted HYDRA room session…"));
                }
            }
        }
        || ()
    });
}

#[hook]
pub(crate) fn use_acceptance_completion_effect(profile: &Profile, ui: &RoomsUi) {
    let rooms = profile.rooms.clone();
    let chats = profile.chats.clone();
    let accepted_value = (*ui.accepted_bootstrap_room).clone();
    let accepted = ui.accepted_bootstrap_room.clone();
    let selected_id = ui.selected_id.clone();
    let status = ui.status.clone();
    use_effect_with(
        (rooms, chats, accepted_value),
        move |(rooms, chats, current)| {
            if let Some((room_id, room_name)) = current.as_ref() {
                if room_acceptance_ready(rooms, room_id) {
                    selected_id.set(room_id.clone());
                    status.set(format!(
                        "Joined {room_name}. Encrypted HYDRA room session established and ready."
                    ));
                    accepted.set(None);
                } else if bootstrap_transport_ready(chats, room_id)
                    && !(*status).starts_with("Encrypted HYDRA room session established")
                {
                    status.set(format!("Encrypted HYDRA room session established for {room_name}. Synchronizing room…"));
                }
            }
            || ()
        },
    );
}

pub(crate) fn room_acceptance_ready(rooms: &[Room], room_id: &str) -> bool {
    rooms
        .iter()
        .any(|room| room.id == room_id && !room.pending_acceptance())
}

pub(crate) fn bootstrap_transport_ready(chats: &[Chat], room_id: &str) -> bool {
    chats.iter().any(|chat| {
        chat.room_transport_only()
            && chat.bootstrap_complete()
            && !chat.left()
            && !chat.peer_left()
            && chat.incoming_request().is_some_and(|request| {
                request.state == "accepted"
                    && request
                        .room_invite
                        .as_ref()
                        .is_some_and(|invite| invite.room_id == room_id)
            })
    })
}

#[hook]
pub(crate) fn use_selection_effect(profile: &Profile, ui: &RoomsUi) {
    let room_ids = profile
        .rooms
        .iter()
        .map(|room| room.id.clone())
        .collect::<Vec<_>>();
    let accepting_bootstrap = ui.accepted_bootstrap_room.is_some();
    let selected_id = ui.selected_id.clone();
    use_effect_with(
        (room_ids, accepting_bootstrap),
        move |(room_ids, accepting)| {
            if !*accepting && selection_needs_fallback(room_ids, selected_id.as_str()) {
                selected_id.set(room_ids[0].clone());
            }
            || ()
        },
    );
}

pub(crate) fn selection_needs_fallback(room_ids: &[String], selected_id: &str) -> bool {
    !room_ids.is_empty() && (selected_id.is_empty() || !room_ids.iter().any(|id| id == selected_id))
}

pub(crate) fn selected_room(
    profile: &Profile,
    selected_id: &UseStateHandle<String>,
) -> Option<Room> {
    profile
        .rooms
        .iter()
        .find(|room| room.id == **selected_id)
        .cloned()
}

fn room_mode(value: &str) -> ghost_rooms::RoomMode {
    match value {
        "community" => ghost_rooms::RoomMode::CommunityVoice,
        "stage" => ghost_rooms::RoomMode::Stage,
        "radio" => ghost_rooms::RoomMode::Radio,
        _ => ghost_rooms::RoomMode::PrivateGroup,
    }
}

pub(crate) fn create_room_callback(
    profile: Profile,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let room_name = ui.name.trim().to_string();
        let mode = room_mode(&ui.mode);
        match crate::controllers::room::create_room(&profile, room_name, mode) {
            Ok((room_id, patch)) => {
                on_update.emit(patch);
                ui.selected_id.set(room_id);
                ui.name.set(String::new());
                ui.status.set(
                    "Room created. Add members by contact, KNS, dot.k, or Kaspa address.".into(),
                );
            }
            Err(error) => ui.status.set(error),
        }
    })
}
