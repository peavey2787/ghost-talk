use super::{
    roster::room_member_is_local,
    state::{RoomActions, RoomMember, RoomsProps, RoomsUi},
};
use crate::model::Profile;
use ghost_domain::identity::PeerBinding;
use yew::prelude::*;

pub(crate) fn member_detail(profile: &Profile, member: &RoomMember, is_owner: bool) -> String {
    if room_member_is_local(profile, member) {
        return format!("{} · You", member.kaspa_address);
    }
    if is_owner && crate::controllers::room::room_member_session_restoring(profile, member) {
        return format!("{} · secure session restoring", member.kaspa_address);
    }
    if is_owner && !crate::controllers::room::room_member_session_established(profile, member) {
        return format!("{} · secure invite pending", member.kaspa_address);
    }
    member.kaspa_address.clone()
}

pub(crate) fn render_member_actions(
    props: &RoomsProps,
    ui: &RoomsUi,
    actions: &RoomActions,
    member: &RoomMember,
    is_owner: bool,
) -> Html {
    let local_member = room_member_is_local(&props.profile, member);
    if !is_owner && (local_member || member.kaspa_address.trim().is_empty()) {
        return Html::default();
    }
    let add_contact = render_add_contact_button(props, member, local_member);
    if !is_owner {
        return html! { <div class="button-row room-member-actions">{add_contact}</div> };
    }
    let kick_address = member.kaspa_address.clone();
    let ban_address = member.kaspa_address.clone();
    let kick = actions.kick_member.clone();
    let ban_target = ui.ban_target.clone();
    html! {
        <div class="button-row room-member-actions">
            {add_contact}
            <button type="button" disabled={*ui.busy} onclick={Callback::from(move |_| kick.emit(kick_address.clone()))}>{"Kick"}</button>
            <button type="button" class="danger-link" disabled={*ui.busy} onclick={Callback::from(move |_| ban_target.set(ban_address.clone()))}>{"Ban"}</button>
        </div>
    }
}

pub(crate) fn render_add_contact_button(
    props: &RoomsProps,
    member: &RoomMember,
    local_member: bool,
) -> Html {
    if local_member || member.kaspa_address.trim().is_empty() {
        return Html::default();
    }
    if profile_has_member_contact(&props.profile, member) {
        return html! { <button type="button" disabled=true>{"In contacts"}</button> };
    }
    let address = member.kaspa_address.clone();
    let on_add_contact = props.on_add_contact.clone();
    html! {
        <button type="button" onclick={Callback::from(move |_| on_add_contact.emit(address.clone()))}>{"Add contact"}</button>
    }
}

pub(crate) fn profile_has_member_contact(profile: &Profile, member: &RoomMember) -> bool {
    let Some(hydra_id) = member.hydra_handle.as_deref() else {
        return false;
    };
    let Ok(peer) = PeerBinding::new(member.kaspa_address.clone(), hydra_id.to_owned()) else {
        return false;
    };
    ghost_contacts::ContactService::resolve_peer(&profile.contacts, &peer).is_some()
}
