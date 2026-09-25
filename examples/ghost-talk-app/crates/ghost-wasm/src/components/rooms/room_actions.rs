use super::state::{spawn_local, Profile, ProfilePatch, Room, RoomsUi};
use yew::prelude::*;

pub(crate) fn send_message_callback(
    profile: Profile,
    password: String,
    selected: Option<Room>,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let Some(room) = selected.clone() else {
            return;
        };
        let body = ui.draft.trim().to_string();
        if body.is_empty() || *ui.busy {
            return;
        }
        let plan = match crate::controllers::room::prepare_send(&profile, &room, body) {
            Ok(plan) => plan,
            Err(error) => {
                ui.status.set(error);
                return;
            }
        };
        on_update.emit(plan.patch.clone());
        ui.busy.set(true);
        ui.status.set("Sending room message…".into());
        let password = password.clone();
        let ui = ui.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            let (patch, result) = crate::controllers::room::complete_send(plan, &password).await;
            on_update.emit(patch);
            match result {
                Ok(()) => {
                    ui.draft.set(String::new());
                    ui.status.set("Room message sent.".into());
                }
                Err(error) => ui.status.set(error),
            }
            ui.busy.set(false);
        });
    })
}

pub(crate) fn leave_room_callback(
    profile: Profile,
    password: String,
    selected: Option<Room>,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let Some(room) = selected.clone() else {
            return;
        };
        if *ui.busy {
            return;
        }
        let plan = match crate::controllers::room::prepare_leave(&profile, &room) {
            Ok(plan) => plan,
            Err(error) => {
                ui.status.set(error);
                return;
            }
        };
        on_update.emit(plan.patch.clone());
        ui.selected_id.set(String::new());
        ui.busy.set(true);
        ui.status.set("Leaving room…".into());
        let password = password.clone();
        let ui = ui.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            let (patch, status) = crate::controllers::room::complete_leave(plan, &password).await;
            on_update.emit(patch);
            ui.status.set(status);
            ui.busy.set(false);
        });
    })
}
