use super::super::form::{prevent_submit, status_view};
use super::discover_render_helpers::{render_lookup_result, render_public_directory};
pub(crate) use crate::{
    components::recipient_input::{resolve_recipient_target, RecipientInput},
    model::{Profile, ProfilePatch, PublicGhostProfile},
};
pub(crate) use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DiscoverProps {
    pub profile: Profile,
    pub password: String,
    pub on_update: Callback<ProfilePatch>,
    pub on_start_chat: Callback<String>,
}

#[component(DiscoverView)]
pub fn discover_view(props: &DiscoverProps) -> Html {
    let query = use_state(String::new);
    let result = use_state(|| None::<PublicGhostProfile>);
    let status = use_state(String::new);
    let busy = use_state(|| false);
    let section = use_state(|| DiscoverSection::People);
    let heard_rooms = use_context::<crate::components::call::RoomListings>()
        .map(|listings| listings.fresh())
        .unwrap_or_default();

    let ghost_address = registered_address(&props.profile);
    let public_users = observed_public_users(&props.profile, &ghost_address);
    let public_count = public_users.len().min(100);

    html! {
        <section class="page">
            <div class="page-head">
                <div>
                    <h2>{"Discover"}</h2>
                    <p>{"Public Ghost Talk profiles are optional. Private/unlisted users can still chat over Kaspa."}</p>
                </div>
            </div>
            {render_section_tabs(section.clone())}
            {if *section == DiscoverSection::Rooms {
                html! {<>
                    {rooms::render_room_directory(props, &heard_rooms, kaspa_announce::Busy { status: status.clone(), busy: busy.clone() })}
                    {status_view(&status)}
                </>}
            } else {
                html! {<>
                    {render_profile_editor(props, &ghost_address, status.clone(), busy.clone())}
                    {render_lookup_form(props, query.clone(), result.clone(), status.clone(), busy.clone())}
                    {status_view(&status)}
                    {render_lookup_result((*result).clone(), props)}
                    {render_public_directory(public_users, public_count, props.on_start_chat.clone())}
                </>}
            }}
        </section>
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DiscoverSection {
    People,
    Rooms,
}

fn render_section_tabs(section: UseStateHandle<DiscoverSection>) -> Html {
    let tab = |value: DiscoverSection, label: &'static str| {
        let section = section.clone();
        html! {
            <button class={(*section == value).then_some("active")} onclick={Callback::from(move |_| section.set(value))}>{label}</button>
        }
    };
    html! {
        <div class="segmented discover-tabs">
            {tab(DiscoverSection::People, "Public users")}
            {tab(DiscoverSection::Rooms, "Rooms")}
        </div>
    }
}

pub(crate) fn registered_address(profile: &Profile) -> String {
    profile
        .wallet
        .as_ref()
        .and_then(|wallet| wallet.registered_address.clone())
        .unwrap_or_default()
}

pub(crate) fn observed_public_users(
    profile: &Profile,
    ghost_address: &str,
) -> Vec<PublicGhostProfile> {
    profile
        .public_directory
        .iter()
        .filter(|user| user.kaspa_address != ghost_address)
        .cloned()
        .collect()
}

mod avatar_editor;
mod editor;
mod kaspa_announce;
mod lookup;
mod rooms;
use editor::render_profile_editor;
use lookup::render_lookup_form;
