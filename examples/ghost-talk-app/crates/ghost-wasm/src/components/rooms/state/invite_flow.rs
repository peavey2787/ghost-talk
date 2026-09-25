use super::{
    resolve_recipient_target, spawn_local, Callback, MouseEvent, Profile, ProfilePatch, Room,
    RoomsUi,
};

pub(crate) fn invite_member_callback(
    profile: Profile,
    password: String,
    selected: Option<Room>,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let request = match invite_request(&profile, selected.as_ref(), &ui) {
            Ok(Some(value)) => value,
            Ok(None) => return,
            Err(error) => {
                ui.status.set(error);
                return;
            }
        };
        ui.busy.set(true);
        ui.status.set("Resolving room member…".into());
        spawn_invite(
            profile.clone(),
            password.clone(),
            request,
            ui.clone(),
            on_update.clone(),
        );
    })
}

struct InviteRequest {
    room: Room,
    raw: String,
    destination: String,
}

fn invite_request(
    profile: &Profile,
    selected: Option<&Room>,
    ui: &RoomsUi,
) -> Result<Option<InviteRequest>, String> {
    let Some(room) = selected.cloned() else {
        return Ok(None);
    };
    if !room_owner_matches(profile, &room) {
        return Err("Only the room owner can invite members.".into());
    }
    if *ui.busy {
        return Ok(None);
    }
    let raw = ui.invite.trim().to_string();
    if raw.is_empty() {
        return Ok(None);
    }
    let destination = resolve_recipient_target(profile, &raw)?;
    Ok(Some(InviteRequest {
        room,
        raw,
        destination,
    }))
}

fn spawn_invite(
    profile: Profile,
    password: String,
    request: InviteRequest,
    ui: RoomsUi,
    on_update: Callback<ProfilePatch>,
) {
    spawn_local(async move {
        let result = crate::controllers::room::invite_member(
            &profile,
            &password,
            &request.room,
            &request.raw,
            &request.destination,
        )
        .await;
        match result {
            Ok((message, patch)) => {
                on_update.emit(patch);
                ui.invite.set(String::new());
                ui.status.set(message);
            }
            Err(error) => ui.status.set(error),
        }
        ui.busy.set(false);
    });
}

pub(crate) fn room_owner_matches(profile: &Profile, room: &Room) -> bool {
    profile.hydra_identity_id.as_deref() == Some(room.owner_hydra_id.as_str())
}
