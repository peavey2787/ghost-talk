use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::model::{Profile, ProfilePatch};

use super::super::state::RoomsUi;

pub(crate) fn accept_bootstrap_callback(
    profile: Profile,
    password: String,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) -> Callback<String> {
    Callback::from(move |chat_id: String| {
        if *ui.busy {
            return;
        }
        let room_name = crate::controllers::room::bootstrap_room_name(&profile, &chat_id);
        ui.busy.set(true);
        ui.status
            .set(format!("Accepting invitation to {room_name}…"));
        let profile = profile.clone();
        let password = password.clone();
        let ui = ui.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            match crate::controllers::room::accept_bootstrap(&profile, &password, &chat_id).await {
                Ok((accepted_name, patch)) => {
                    on_update.emit(patch);
                    ui.accepted_bootstrap_room.set(None);
                    ui.status.set(format!(
                        "Invitation to {accepted_name} accepted. Establishing the encrypted HYDRA room session…"
                    ));
                }
                Err(error) => {
                    ui.accepted_bootstrap_room.set(None);
                    ui.status.set(error);
                }
            }
            ui.busy.set(false);
        });
    })
}
