use super::{create_room_callback, invite_member_callback, Room, RoomActions, RoomsProps, RoomsUi};
use crate::components::rooms::{
    moderation::{
        accept_bootstrap_callback, confirm_ban_callback, disband_room_callback,
        kick_member_callback, unban_member_callback,
    },
    render::{accept_existing_room_callback, decline_bootstrap_callback},
    room_actions::{leave_room_callback, send_message_callback},
};

pub(crate) fn build_room_actions(
    props: &RoomsProps,
    ui: &RoomsUi,
    selected: Option<Room>,
) -> RoomActions {
    let standard = standard_actions(props, ui, selected.clone());
    let moderation = moderation_actions(props, ui, selected);
    let bootstrap = bootstrap_actions(props, ui);
    RoomActions {
        create: create_room_callback(props.profile.clone(), ui.clone(), props.on_update.clone()),
        invite_member: standard.0,
        send_message: standard.1,
        leave_room: standard.2,
        disband_room: moderation.0,
        kick_member: moderation.1,
        confirm_ban: moderation.2,
        unban_member: moderation.3,
        accept_bootstrap_invite: bootstrap.0,
        decline_bootstrap_invite: bootstrap.1,
        accept_existing_room: bootstrap.2,
    }
}

fn standard_actions(
    props: &RoomsProps,
    ui: &RoomsUi,
    selected: Option<Room>,
) -> (
    yew::Callback<web_sys::MouseEvent>,
    yew::Callback<web_sys::MouseEvent>,
    yew::Callback<web_sys::MouseEvent>,
) {
    (
        invite_member_callback(
            props.profile.clone(),
            props.password.clone(),
            selected.clone(),
            ui.clone(),
            props.on_update.clone(),
        ),
        send_message_callback(
            props.profile.clone(),
            props.password.clone(),
            selected.clone(),
            ui.clone(),
            props.on_update.clone(),
        ),
        leave_room_callback(
            props.profile.clone(),
            props.password.clone(),
            selected,
            ui.clone(),
            props.on_update.clone(),
        ),
    )
}

fn moderation_actions(
    props: &RoomsProps,
    ui: &RoomsUi,
    selected: Option<Room>,
) -> (
    yew::Callback<web_sys::MouseEvent>,
    yew::Callback<String>,
    yew::Callback<web_sys::MouseEvent>,
    yew::Callback<String>,
) {
    (
        disband_room_callback(
            props.profile.clone(),
            props.password.clone(),
            selected.clone(),
            ui.clone(),
            props.on_update.clone(),
        ),
        kick_member_callback(
            props.profile.clone(),
            props.password.clone(),
            selected.clone(),
            ui.clone(),
            props.on_update.clone(),
        ),
        confirm_ban_callback(
            props.profile.clone(),
            props.password.clone(),
            selected.clone(),
            ui.clone(),
            props.on_update.clone(),
        ),
        unban_member_callback(
            props.profile.clone(),
            props.password.clone(),
            selected,
            ui.clone(),
            props.on_update.clone(),
        ),
    )
}

fn bootstrap_actions(
    props: &RoomsProps,
    ui: &RoomsUi,
) -> (
    yew::Callback<String>,
    yew::Callback<String>,
    yew::Callback<String>,
) {
    (
        accept_bootstrap_callback(
            props.profile.clone(),
            props.password.clone(),
            ui.clone(),
            props.on_update.clone(),
        ),
        decline_bootstrap_callback(props.profile.clone(), ui.clone(), props.on_update.clone()),
        accept_existing_room_callback(props.profile.clone(), ui.clone(), props.on_update.clone()),
    )
}
