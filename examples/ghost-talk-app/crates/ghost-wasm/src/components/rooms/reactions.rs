use super::state::{spawn_local, Room, RoomMessage, RoomsProps, RoomsUi};
use crate::model::ReactionKind;
use yew::prelude::*;

pub(crate) fn render_room_reactions(
    props: &RoomsProps,
    room: &Room,
    message: &RoomMessage,
    ui: &RoomsUi,
) -> Html {
    let own_actor = props.profile.hydra_identity_id.as_deref();
    let own_reaction = own_actor.and_then(|actor| message.reaction_for(actor));
    let summary =
        crate::components::chat::reaction_ui::render_summary(&message.reactions, own_reaction);
    let picker = own_actor
        .filter(|actor| room.can_react(actor) && !room.pending_acceptance())
        .map(|_| {
            crate::components::chat::reaction_ui::render_picker(
                own_reaction,
                reaction_callback(props, room, message, ui),
            )
        })
        .unwrap_or_default();
    html! {<>{summary}{picker}</>}
}

fn reaction_callback(
    props: &RoomsProps,
    room: &Room,
    message: &RoomMessage,
    ui: &RoomsUi,
) -> Callback<ReactionKind> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let room = room.clone();
    let message = message.clone();
    let status = ui.status.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |kind| {
        let plan = match crate::controllers::room::prepare_reaction(&profile, &room, &message, kind)
        {
            Ok(plan) => plan,
            Err(error) => {
                status.set(error);
                return;
            }
        };
        on_update.emit(plan.patch.clone());
        let status = status.clone();
        let on_update = on_update.clone();
        let password = password.clone();
        spawn_local(async move {
            let (patch, result) =
                crate::controllers::room::complete_reaction(plan, &password).await;
            on_update.emit(patch);
            status.set(match result {
                Ok(()) => "Room reaction sent.".into(),
                Err(error) => error,
            });
        });
    })
}
