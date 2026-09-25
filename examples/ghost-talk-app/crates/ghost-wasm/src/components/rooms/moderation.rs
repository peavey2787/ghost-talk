use super::{
    roster::{format_ban_expiry, parse_ban_expiry},
    state::{room_owner_matches, spawn_local, Profile, ProfilePatch, Room, RoomsUi},
};
use yew::prelude::*;

pub(crate) fn disband_room_callback(
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
        if !room_owner_matches(&profile, &room) || *ui.busy {
            return;
        }
        ui.busy.set(true);
        ui.status.set("Disbanding room…".into());
        let profile = profile.clone();
        let password = password.clone();
        let ui = ui.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            match crate::controllers::room::disband(&profile, &password, &room).await {
                Ok((patch, failures)) => {
                    on_update.emit(patch);
                    ui.selected_id.set(String::new());
                    ui.status.set(disband_status(&failures));
                }
                Err(error) => ui.status.set(error),
            }
            ui.busy.set(false);
        });
    })
}

pub(crate) fn disband_status(failures: &[String]) -> String {
    if failures.is_empty() {
        "Room disbanded.".into()
    } else {
        format!(
            "Room disbanded locally; notification failed for {}.",
            failures.join(", ")
        )
    }
}

pub(crate) fn kick_member_callback(
    profile: Profile,
    password: String,
    selected: Option<Room>,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) -> Callback<String> {
    Callback::from(move |address: String| {
        let Some(room) = selected.clone() else {
            return;
        };
        if !room_owner_matches(&profile, &room) || *ui.busy {
            return;
        }
        ui.busy.set(true);
        ui.status.set("Removing room member…".into());
        let profile = profile.clone();
        let password = password.clone();
        let ui = ui.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            match crate::controllers::room::remove_member(&profile, &password, &room, &address)
                .await
            {
                Ok((label, patch)) => {
                    on_update.emit(patch);
                    ui.status.set(format!("{label} was removed from the room."));
                }
                Err(error) => ui.status.set(error),
            }
            ui.busy.set(false);
        });
    })
}

pub(crate) fn confirm_ban_callback(
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
        if !room_owner_matches(&profile, &room) || *ui.busy {
            return;
        }
        let address = ui.ban_target.trim().to_string();
        if address.is_empty() {
            ui.status.set("Select a current member to ban.".into());
            return;
        }
        let expires_at = match parse_ban_expiry(ui.ban_amount.as_str(), ui.ban_unit.as_str()) {
            Ok(value) => value,
            Err(error) => {
                ui.status.set(error);
                return;
            }
        };
        ui.busy.set(true);
        ui.status.set("Banning room member…".into());
        let profile = profile.clone();
        let password = password.clone();
        let ui = ui.clone();
        let on_update = on_update.clone();
        spawn_local(ban_member_async(BanMemberTask {
            profile,
            password,
            room,
            address,
            expires_at,
            ui,
            on_update,
        }));
    })
}

struct BanMemberTask {
    profile: Profile,
    password: String,
    room: Room,
    address: String,
    expires_at: Option<f64>,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
}

async fn ban_member_async(task: BanMemberTask) {
    match crate::controllers::room::ban_member_command(
        &task.profile,
        &task.password,
        &task.room,
        &task.address,
        task.expires_at,
    )
    .await
    {
        Ok((label, patch)) => {
            task.on_update.emit(patch);
            task.ui.ban_target.set(String::new());
            task.ui.status.set(format!(
                "{label} was banned{}.",
                format_ban_expiry(task.expires_at)
            ));
        }
        Err(error) => task.ui.status.set(error),
    }
    task.ui.busy.set(false);
}

pub(crate) fn unban_member_callback(
    profile: Profile,
    password: String,
    selected: Option<Room>,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) -> Callback<String> {
    Callback::from(move |address: String| {
        let Some(room) = selected.clone() else {
            return;
        };
        if !room_owner_matches(&profile, &room) || *ui.busy {
            return;
        }
        ui.busy.set(true);
        let profile = profile.clone();
        let password = password.clone();
        let ui = ui.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            match crate::controllers::room::unban_member(&profile, &password, &room, &address).await
            {
                Ok(patch) => {
                    on_update.emit(patch);
                    ui.status.set("Room ban removed.".into());
                }
                Err(error) => ui.status.set(error),
            }
            ui.busy.set(false);
        });
    })
}

mod bootstrap;
pub(crate) use bootstrap::accept_bootstrap_callback;
