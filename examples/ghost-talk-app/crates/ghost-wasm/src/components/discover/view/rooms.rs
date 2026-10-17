//! Discover → Rooms: public rooms only, filterable by room type. Rooms this
//! client knows locally are merged with signed announcements heard on the
//! swarm room directory. Private, invite-only and unlisted rooms (including
//! private groups) are never announced, so they never appear.

use super::{kaspa_announce, DiscoverProps};
use crate::model::Profile;
use ghost_rooms::{RoomAnnouncement, RoomMode, SWARM_LISTING_LIFETIME_MS};
use web_sys::HtmlSelectElement;
use yew::prelude::*;

/// Room types a public room can have; private groups are never listed.
const PUBLIC_MODES: [RoomMode; 3] = [RoomMode::CommunityVoice, RoomMode::Stage, RoomMode::Radio];

/// Local public rooms first, then swarm listings for rooms not known locally.
pub(super) fn discoverable_rooms(
    profile: &Profile,
    heard: &[RoomAnnouncement],
    filter: Option<RoomMode>,
) -> Vec<RoomAnnouncement> {
    let now = crate::now_ms().max(0.0) as u64;
    let mut rooms: Vec<RoomAnnouncement> = profile
        .rooms
        .iter()
        .filter_map(|room| RoomAnnouncement::for_room(room, now, SWARM_LISTING_LIFETIME_MS))
        .collect();
    let heard = heard
        .iter()
        .filter(|listing| !rooms.iter().any(|room| room.room_id == listing.room_id))
        .cloned()
        .collect::<Vec<_>>();
    rooms.extend(heard);
    rooms.retain(|room| filter.is_none_or(|mode| room.mode == mode));
    rooms
}

pub(super) fn render_room_directory(
    props: &DiscoverProps,
    heard: &[RoomAnnouncement],
    filter: UseStateHandle<Option<RoomMode>>,
    busy: kaspa_announce::Busy,
) -> Html {
    let rooms = discoverable_rooms(&props.profile, heard, *filter);
    html! {
        <div class="card discover-public-list">
            <div class="public-profile-title">
                <div>
                    <h3>{"Public rooms"}</h3>
                    <small>{"Your public rooms and rooms announced on the swarm or Kaspa. Private groups are never listed."}</small>
                </div>
                {render_mode_filter(filter)}
                <span class="directory-count">{rooms.len()}</span>
            </div>
            {render_rooms(props, rooms, &busy)}
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

fn render_rooms(
    props: &DiscoverProps,
    rooms: Vec<RoomAnnouncement>,
    busy: &kaspa_announce::Busy,
) -> Html {
    if rooms.is_empty() {
        return html! { <p class="muted">{"No public rooms of this type are known yet."}</p> };
    }
    html! {
        <div class="public-user-list">
            {for rooms.iter().map(|room| render_room(props, room, busy))}
        </div>
    }
}

fn render_room(
    props: &DiscoverProps,
    room: &RoomAnnouncement,
    busy: &kaspa_announce::Busy,
) -> Html {
    html! {
        <div class="public-user-row">
            <div>
                <b>{room.name.clone()}</b>
                <small>{format!("{} · {} member(s) · host {}", room.mode.label(), room.member_count, room.owner_label)}</small>
            </div>
            {message_host_button(&props.profile, room, props.on_start_chat.clone())}
            {kaspa_announce::announce_button(props, room, busy)}
        </div>
    }
}

/// Rooms this client already knows need no introduction; for a room heard
/// on the swarm, the way in is a chat with its host.
fn message_host_button(
    profile: &Profile,
    room: &RoomAnnouncement,
    on_start_chat: Callback<String>,
) -> Html {
    if profile.rooms.iter().any(|known| known.id == room.room_id) {
        return Html::default();
    }
    let host = room.owner_kaspa_address.clone();
    html! {
        <button class="primary" onclick={Callback::from(move |_| on_start_chat.emit(host.clone()))}>
            {"Message host"}
        </button>
    }
}
