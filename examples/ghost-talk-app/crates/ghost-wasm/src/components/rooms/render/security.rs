use crate::model::Room;
use yew::prelude::*;

pub(super) fn room_security_state(room: &Room) -> Html {
    html! {
        <div class="security-state-row">
            <span>{access_label(room.access())}</span>
        </div>
    }
}

fn access_label(access: ghost_rooms::RoomAccess) -> &'static str {
    match access {
        ghost_rooms::RoomAccess::Private => "🔒 Private Room",
        ghost_rooms::RoomAccess::Public => "🌐 Public Room",
    }
}
