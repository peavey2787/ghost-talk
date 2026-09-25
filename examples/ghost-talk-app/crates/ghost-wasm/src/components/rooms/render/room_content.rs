use crate::components::rooms::{
    input,
    member_render::{member_detail, render_add_contact_button, render_member_actions},
    prevent_submit,
    roster::{render_active_bans, render_ban_editor},
    state::{
        format_message_timestamp, room_owner_matches, Room, RoomActions, RoomMember, RoomMessage,
        RoomsProps, RoomsUi,
    },
};
use yew::prelude::*;

pub(super) fn render_room_messages(props: &RoomsProps, ui: &RoomsUi, room: &Room) -> Html {
    if room.messages().is_empty() {
        return html! { <div class="card room-messages"><p class="muted">{"No messages yet."}</p></div> };
    }
    let own_hydra = props
        .profile
        .hydra_identity_id
        .as_deref()
        .unwrap_or_default();
    html! {<div class="card room-messages">{for room.messages().iter().map(|message| render_room_message(props, ui, room, message, own_hydra))}</div>}
}

fn render_room_message(
    props: &RoomsProps,
    ui: &RoomsUi,
    room: &Room,
    message: &RoomMessage,
    own_hydra: &str,
) -> Html {
    html! {
        <div class={classes!("room-message", (message.sender_hydra_id == own_hydra).then_some("own"))}>
            <div class="room-message-head"><b>{message.sender_label.clone()}</b><small>{format_message_timestamp(message.created_at)}</small></div>
            <span>{message.body.clone()}</span>
            {crate::components::rooms::reactions::render_room_reactions(props, room, message, ui)}
        </div>
    }
}

pub(super) fn render_room_composer(ui: &RoomsUi, actions: &RoomActions, room: &Room) -> Html {
    html! {<form class="card inline-form room-composer" onsubmit={prevent_submit()}><input value={(*ui.draft).clone()} oninput={input(ui.draft.clone())} placeholder="Message room" disabled={room.pending_acceptance()}/><button type="submit" class="primary" disabled={room.pending_acceptance() || *ui.busy || ui.draft.trim().is_empty()} onclick={actions.send_message.clone()}>{"Send"}</button></form>}
}

pub(super) fn render_room_roster(
    props: &RoomsProps,
    ui: &RoomsUi,
    actions: &RoomActions,
    room: &Room,
    is_owner: bool,
) -> Html {
    html! {<div class="card room-roster"><h3>{"Members"}</h3><div class="room-member-list">{render_room_owner(props, room)}{for room.members().iter().map(|member| render_room_member(props, ui, actions, member, is_owner))}</div>{render_ban_editor(ui, actions, room, is_owner)}{render_active_bans(ui, actions, room, is_owner)}</div>}
}

fn render_room_owner(props: &RoomsProps, room: &Room) -> Html {
    let local_owner = room_owner_matches(&props.profile, room);
    let owner = RoomMember {
        contact_id: None,
        label: room.owner_label.clone(),
        kaspa_address: room.owner_kaspa_address.clone(),
        hydra_handle: Some(room.owner_hydra_id.clone()),
        role: ghost_rooms::Role::Moderator,
    };
    html! {<div class="room-member-row"><div><b>{room.owner_label.clone()}</b><small>{"Owner"}</small></div>{render_add_contact_button(props, &owner, local_owner)}</div>}
}

fn render_room_member(
    props: &RoomsProps,
    ui: &RoomsUi,
    actions: &RoomActions,
    member: &RoomMember,
    is_owner: bool,
) -> Html {
    let detail = member_detail(&props.profile, member, is_owner);
    html! {<div class="room-member-row"><div class="room-member-identity"><b title={member.label.clone()}>{member.label.clone()}</b><small title={detail.clone()}>{detail}</small></div>{render_member_actions(props, ui, actions, member, is_owner)}</div>}
}
