use super::{input, prevent_submit};
use super::{
    roster::render_room_status,
    state::{
        room_owner_matches, Profile, ProfilePatch, RecipientInput, Room, RoomActions, RoomsProps,
        RoomsUi,
    },
};
use web_sys::HtmlSelectElement;
use yew::prelude::*;
mod room_content;
mod security;
use room_content::{render_room_composer, render_room_messages, render_room_roster};
use security::room_security_state;

pub(crate) fn decline_bootstrap_callback(
    profile: Profile,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) -> Callback<String> {
    Callback::from(move |chat_id: String| {
        let (room_name, patch) = crate::controllers::room::decline_bootstrap(&profile, &chat_id);
        if let Some(patch) = patch {
            on_update.emit(patch);
        }
        ui.status
            .set(format!("Invitation to {room_name} declined."));
    })
}

pub(crate) fn accept_existing_room_callback(
    profile: Profile,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) -> Callback<String> {
    Callback::from(move |room_id: String| {
        let Some((room_name, patch)) =
            crate::controllers::room::accept_existing(&profile, &room_id)
        else {
            return;
        };
        on_update.emit(patch);
        ui.selected_id.set(room_id);
        ui.status.set(format!(
            "Joined {room_name}. Encrypted HYDRA room session established and ready."
        ));
    })
}

#[derive(Clone)]
pub(crate) struct PendingRoomInvite {
    pub(crate) chat_id: String,
    pub(crate) owner_label: String,
    pub(crate) meta: crate::model::RoomInviteMeta,
}

pub(crate) fn render_rooms_page(
    props: &RoomsProps,
    ui: &RoomsUi,
    selected: Option<Room>,
    actions: &RoomActions,
) -> Html {
    let notices = pending_room_invites(&props.profile);
    html! {
        <section class="page rooms-page">
            <div class="page-head"><div><h2>{"Rooms"}</h2><p>{"Persistent group chats relayed over authenticated HYDRA peer sessions."}</p></div></div>
            <div class="rooms-layout">
                {render_rooms_sidebar(props, ui, actions, &notices)}
                {render_selected_room(props, ui, actions, selected)}
            </div>
            {render_room_status(ui)}
        </section>
    }
}

pub(crate) fn pending_room_invites(profile: &Profile) -> Vec<PendingRoomInvite> {
    profile
        .chats
        .iter()
        .filter_map(|chat| {
            let request = chat.incoming_request()?;
            let meta = request.room_invite.as_ref()?;
            (request.state == "pending").then(|| PendingRoomInvite {
                chat_id: chat.id.clone(),
                owner_label: chat.label.clone(),
                meta: meta.clone(),
            })
        })
        .collect()
}

pub(crate) fn render_rooms_sidebar(
    props: &RoomsProps,
    ui: &RoomsUi,
    actions: &RoomActions,
    notices: &[PendingRoomInvite],
) -> Html {
    html! {
        <aside class="card rooms-list">
            {render_pending_room_invites(ui, actions, notices)}
            {render_create_room_form(ui, &actions.create)}
            {render_room_list(&props.profile, ui)}
        </aside>
    }
}

pub(crate) fn render_pending_room_invites(
    ui: &RoomsUi,
    actions: &RoomActions,
    notices: &[PendingRoomInvite],
) -> Html {
    if notices.is_empty() {
        return Html::default();
    }
    html! {
        <div class="room-invite-notices">
            {for notices.iter().map(|notice| render_pending_invite(ui, actions, notice))}
        </div>
    }
}

pub(crate) fn render_pending_invite(
    ui: &RoomsUi,
    actions: &RoomActions,
    notice: &PendingRoomInvite,
) -> Html {
    let accept_id = notice.chat_id.clone();
    let decline_id = notice.chat_id.clone();
    let accept = actions.accept_bootstrap_invite.clone();
    let decline = actions.decline_bootstrap_invite.clone();
    html! {
        <div class="room-invite-notice">
            <div><b>{format!("Room invite · {}", notice.meta.room_name)}</b><small>{format!("From {}", notice.owner_label)}</small></div>
            <div class="button-row">
                <button type="button" class="primary" disabled={*ui.busy} onclick={Callback::from(move |_| accept.emit(accept_id.clone()))}>{"Accept"}</button>
                <button type="button" disabled={*ui.busy} onclick={Callback::from(move |_| decline.emit(decline_id.clone()))}>{"Decline"}</button>
            </div>
        </div>
    }
}

pub(crate) fn render_create_room_form(ui: &RoomsUi, create: &Callback<MouseEvent>) -> Html {
    html! {
        <form class="inline-form room-create-form" onsubmit={prevent_submit()}>
            <input value={(*ui.name).clone()} oninput={input(ui.name.clone())} placeholder="Room name"/>
            <select value={(*ui.mode).clone()} onchange={{
                let mode = ui.mode.clone();
                Callback::from(move |event: Event| {
                    let select: HtmlSelectElement = event.target_unchecked_into();
                    mode.set(select.value());
                })
            }}>
                <option value="private">{"Private group"}</option>
                <option value="community">{"Community voice"}</option>
                <option value="stage">{"Stage"}</option>
                <option value="radio">{"Radio"}</option>
            </select>
            <button type="submit" class="primary" disabled={ui.name.trim().is_empty()} onclick={create.clone()}>{"Create"}</button>
        </form>
    }
}

pub(crate) fn render_room_list(profile: &Profile, ui: &RoomsUi) -> Html {
    if profile.rooms.is_empty() {
        return html! { <p class="muted">{"No rooms yet."}</p> };
    }
    html! { <>{for profile.rooms.iter().map(|room| render_room_row(ui, room))}</> }
}

pub(crate) fn render_room_row(ui: &RoomsUi, room: &Room) -> Html {
    let view = crate::view_models::RoomViewModel::from(room);
    let id = view.id.clone();
    let selected_id = ui.selected_id.clone();
    let participants = format!(
        "{} participant{}",
        view.participant_count,
        if view.participant_count == 1 { "" } else { "s" }
    );
    html! {
        <button type="button" class={classes!("room-row", (*ui.selected_id == view.id).then_some("active"))} onclick={Callback::from(move |_| selected_id.set(id.clone()))}>
            <b>{view.name.clone()}</b>
            <small>{participants}</small>
        </button>
    }
}

pub(crate) fn participant_count(room: &Room) -> String {
    format!(
        "{} participant{}",
        room.members().len() + 1,
        if room.members().is_empty() { "" } else { "s" }
    )
}

pub(crate) fn render_selected_room(
    props: &RoomsProps,
    ui: &RoomsUi,
    actions: &RoomActions,
    selected: Option<Room>,
) -> Html {
    match selected {
        Some(room) => render_room_chat(props, ui, actions, &room),
        None => html! { <div class="card muted">{"Create or select a room."}</div> },
    }
}

pub(crate) fn render_room_chat(
    props: &RoomsProps,
    ui: &RoomsUi,
    actions: &RoomActions,
    room: &Room,
) -> Html {
    let is_owner = room_owner_matches(&props.profile, room);
    html! {
        <div class="room-chat">
            {render_room_header(props, ui, actions, room, is_owner)}
            {render_pending_acceptance(ui, actions, room)}
            {render_room_messages(props, ui, room)}
            {render_room_composer(ui, actions, room)}
            {render_room_roster(props, ui, actions, room, is_owner)}
        </div>
    }
}

pub(crate) fn render_room_header(
    props: &RoomsProps,
    ui: &RoomsUi,
    actions: &RoomActions,
    room: &Room,
    is_owner: bool,
) -> Html {
    html! {
        <div class="card room-header">
            <div><h3>{room.name.clone()}</h3><small>{format!("Owner: {} · {}", room.owner_label, participant_count(room))}</small>{room_security_state(room)}</div>
            <div class="room-header-actions">
                {render_room_invite_form(props, ui, actions, is_owner)}
                {render_room_exit_action(ui, actions, is_owner)}
            </div>
        </div>
    }
}

pub(crate) fn render_room_invite_form(
    props: &RoomsProps,
    ui: &RoomsUi,
    actions: &RoomActions,
    is_owner: bool,
) -> Html {
    if !is_owner {
        return Html::default();
    }
    let invite = ui.invite.clone();
    html! {
        <form class="room-invite" onsubmit={prevent_submit()}>
            <RecipientInput profile={props.profile.clone()} value={(*ui.invite).clone()} on_change={Callback::from(move |value:String| invite.set(value))} placeholder="Contact, KNS, dot.k, or Kaspa address" />
            <button type="submit" disabled={*ui.busy || ui.invite.trim().is_empty()} onclick={actions.invite_member.clone()}>{"Invite"}</button>
        </form>
    }
}

pub(crate) fn render_room_exit_action(ui: &RoomsUi, actions: &RoomActions, is_owner: bool) -> Html {
    if is_owner {
        html! { <button type="button" class="danger-link" disabled={*ui.busy} onclick={actions.disband_room.clone()}>{"Disband room"}</button> }
    } else {
        html! { <button type="button" class="danger-link" disabled={*ui.busy} onclick={actions.leave_room.clone()}>{"Leave room"}</button> }
    }
}

pub(crate) fn render_pending_acceptance(ui: &RoomsUi, actions: &RoomActions, room: &Room) -> Html {
    if !room.pending_acceptance() {
        return Html::default();
    }
    let room_id = room.id.clone();
    let accept = actions.accept_existing_room.clone();
    html! {
        <div class="card room-pending-banner">
            <div><b>{format!("{} invited you to this room", room.owner_label)}</b><small>{"Accept to participate, or decline to leave the room."}</small></div>
            <div class="button-row">
                <button type="button" class="primary" onclick={Callback::from(move |_| accept.emit(room_id.clone()))}>{"Accept"}</button>
                <button type="button" class="danger-link" disabled={*ui.busy} onclick={actions.leave_room.clone()}>{"Decline"}</button>
            </div>
        </div>
    }
}
