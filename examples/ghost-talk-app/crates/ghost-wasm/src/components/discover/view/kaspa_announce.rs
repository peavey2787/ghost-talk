//! Discover → Rooms: the owner of a public room can publish its listing on
//! Kaspa, where it reaches peers that are not on the swarm for a day. It
//! costs a network fee, so it is unavailable in No-spend mode.

use super::{spawn_local, DiscoverProps};
use ghost_domain::settings::SpendMode;
use ghost_rooms::RoomAnnouncement;
use yew::prelude::*;

/// The Discover page's shared status line and busy flag.
#[derive(Clone)]
pub(super) struct Busy {
    pub(super) status: UseStateHandle<String>,
    pub(super) busy: UseStateHandle<bool>,
}

pub(super) fn announce_button(
    props: &DiscoverProps,
    listing: &RoomAnnouncement,
    busy: &Busy,
) -> Html {
    let owned = crate::controllers::room::owned_rooms(&props.profile);
    let Some(room) = owned.into_iter().find(|room| room.id == listing.room_id) else {
        return Html::default();
    };
    let no_spend = props.profile.settings.spend_mode == SpendMode::NoSpend;
    let title = if no_spend {
        "Switch to Kaspa mode to publish a listing on Kaspa."
    } else {
        "Publish this listing on Kaspa for 24 hours (network fee applies)."
    };
    let onclick = announce_callback(props, room, busy.clone());
    html! {
        <button class="announce-on-kaspa" {title} disabled={no_spend || *busy.busy} {onclick}>{"Announce on Kaspa"}</button>
    }
}

fn announce_callback(
    props: &DiscoverProps,
    room: crate::model::Room,
    busy: Busy,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        let (profile, password, room, on_update, busy) = (
            profile.clone(),
            password.clone(),
            room.clone(),
            on_update.clone(),
            busy.clone(),
        );
        busy.busy.set(true);
        busy.status
            .set(format!("Announcing {} on Kaspa…", room.name));
        spawn_local(async move {
            match crate::controllers::room::announce_on_kaspa(&profile, &password, &room).await {
                Ok(done) => {
                    on_update.emit(done.patch);
                    busy.status.set(format!(
                        "Announced {} on Kaspa: {}",
                        room.name, done.transaction_id
                    ));
                }
                Err(error) => busy
                    .status
                    .set(format!("Kaspa announcement failed: {error}")),
            }
            busy.busy.set(false);
        });
    })
}
