use crate::model::Room;
use yew::prelude::*;

pub(super) fn room_security_state(room: &Room) -> Html {
    let broadcast = room
        .broadcast_enabled()
        .then(|| html! { <span>{"📡 Broadcast enabled"}</span> });
    html! {
        <div class="security-state-row">
            <span>{access_label(room.access())}</span>
            <span>{audio_label(room.audio_policy())}</span>
            {broadcast.unwrap_or_default()}
        </div>
    }
}

fn access_label(access: ghost_rooms::RoomAccess) -> &'static str {
    match access {
        ghost_rooms::RoomAccess::Private => "🎙 Private Room",
        ghost_rooms::RoomAccess::InviteOnly => "🎙 Invite-only Room",
        ghost_rooms::RoomAccess::Unlisted => "🎙 Unlisted Room",
        ghost_rooms::RoomAccess::Public => "🌐 Public Room",
    }
}

fn audio_label(policy: ghost_rooms::ChannelPolicy) -> &'static str {
    match policy {
        ghost_rooms::ChannelPolicy::Off => "Audio off",
        ghost_rooms::ChannelPolicy::Interactive => "Interactive voice",
        ghost_rooms::ChannelPolicy::PresentersOnly => "Presenters only",
    }
}
