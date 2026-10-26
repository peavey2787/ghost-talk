//! Discover → Rooms: public rooms only. Rooms this client knows locally are
//! merged with signed announcements heard on the swarm room directory.
//! Private rooms are never announced, so they never appear.

use super::{kaspa_announce, DiscoverProps};
use crate::model::Profile;
use ghost_rooms::{RoomAnnouncement, SWARM_LISTING_LIFETIME_MS};
use yew::prelude::*;

/// Local public rooms first, then swarm listings for rooms not known locally.
pub(super) fn discoverable_rooms(
    profile: &Profile,
    heard: &[RoomAnnouncement],
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
    rooms
}

pub(super) fn render_room_directory(
    props: &DiscoverProps,
    heard: &[RoomAnnouncement],
    busy: kaspa_announce::Busy,
) -> Html {
    let rooms = discoverable_rooms(&props.profile, heard);
    html! {
        <div class="card discover-public-list">
            <div class="public-profile-title">
                <div>
                    <h3>{"Public rooms"}</h3>
                    <small>{"Your public rooms and rooms announced on the swarm or Kaspa. Private rooms are never listed."}</small>
                </div>
                <span class="directory-count">{rooms.len()}</span>
            </div>
            {render_rooms(props, rooms, &busy)}
        </div>
    }
}

fn render_rooms(
    props: &DiscoverProps,
    rooms: Vec<RoomAnnouncement>,
    busy: &kaspa_announce::Busy,
) -> Html {
    if rooms.is_empty() {
        return html! { <p class="muted">{"No public rooms are known yet."}</p> };
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
                <small>{format!("{} member(s) · host {}", room.member_count, room.owner_label)}</small>
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
