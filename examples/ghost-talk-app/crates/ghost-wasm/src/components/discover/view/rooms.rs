//! Discover → Rooms: public rooms only, filterable by room type. Private,
//! invite-only and unlisted rooms (including private groups) never appear.

use crate::model::{Profile, Room};
use ghost_rooms::{RoomAccess, RoomMode};
use web_sys::HtmlSelectElement;
use yew::prelude::*;

/// Room types a public room can have; private groups are never listed.
const PUBLIC_MODES: [RoomMode; 3] = [RoomMode::CommunityVoice, RoomMode::Stage, RoomMode::Radio];

pub(super) fn discoverable_rooms(profile: &Profile, filter: Option<RoomMode>) -> Vec<Room> {
    profile
        .rooms
        .iter()
        .filter(|room| room.access() == RoomAccess::Public)
        .filter(|room| filter.is_none_or(|mode| room.policies().mode() == Some(mode)))
        .cloned()
        .collect()
}

pub(super) fn render_room_directory(
    profile: &Profile,
    filter: UseStateHandle<Option<RoomMode>>,
) -> Html {
    let rooms = discoverable_rooms(profile, *filter);
    html! {
        <div class="card discover-public-list">
            <div class="public-profile-title">
                <div>
                    <h3>{"Public rooms"}</h3>
                    <small>{"Public rooms known to this client. Private groups are never listed."}</small>
                </div>
                {render_mode_filter(filter)}
                <span class="directory-count">{rooms.len()}</span>
            </div>
            {render_rooms(rooms)}
        </div>
    }
}

fn render_mode_filter(filter: UseStateHandle<Option<RoomMode>>) -> Html {
    let selected = filter.map(mode_key).unwrap_or("all");
    let onchange = Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        filter.set(
            PUBLIC_MODES
                .into_iter()
                .find(|mode| mode_key(*mode) == value),
        );
    });
    html! {
        <select class="room-type-filter" aria-label="Room type" value={selected} {onchange}>
            <option value="all" selected={selected == "all"}>{"All types"}</option>
            {for PUBLIC_MODES.into_iter().map(|mode| html! {
                <option value={mode_key(mode)} selected={selected == mode_key(mode)}>{mode.label()}</option>
            })}
        </select>
    }
}

fn mode_key(mode: RoomMode) -> &'static str {
    match mode {
        RoomMode::PrivateGroup => "private",
        RoomMode::CommunityVoice => "community",
        RoomMode::Stage => "stage",
        RoomMode::Radio => "radio",
    }
}

fn render_rooms(rooms: Vec<Room>) -> Html {
    if rooms.is_empty() {
        return html! { <p class="muted">{"No public rooms of this type are known yet."}</p> };
    }
    html! { <div class="public-user-list">{for rooms.iter().map(render_room)}</div> }
}

fn render_room(room: &Room) -> Html {
    let kind = room
        .policies()
        .mode()
        .map(RoomMode::label)
        .unwrap_or("Custom room");
    html! {
        <div class="public-user-row">
            <div>
                <b>{room.name.clone()}</b>
                <small>{format!("{kind} · {} member(s) · host {}", room.members().len(), room.owner_label)}</small>
            </div>
        </div>
    }
}
