use crate::{
    components::{
        avatar::Avatar,
        form::{prevent_submit, select_value, text_input},
        media_upload::MediaUpload,
    },
    model::{Profile, ProfilePatch},
};
use ghost_media::MediaReference;
use yew::prelude::*;

#[derive(Clone)]
pub(super) struct CatalogUi {
    pub(super) creator_name: UseStateHandle<String>,
    pub(super) creator_description: UseStateHandle<String>,
    pub(super) creator_id: UseStateHandle<String>,
    pub(super) creator_avatar: UseStateHandle<Option<MediaReference>>,
    pub(super) station_name: UseStateHandle<String>,
    pub(super) station_description: UseStateHandle<String>,
    pub(super) station_artwork: UseStateHandle<Option<MediaReference>>,
    pub(super) station_room_id: UseStateHandle<String>,
    pub(super) show_title: UseStateHandle<String>,
    pub(super) show_description: UseStateHandle<String>,
    pub(super) show_artwork: UseStateHandle<Option<MediaReference>>,
    pub(super) show_station_id: UseStateHandle<String>,
}

pub(super) fn render(
    profile: &Profile,
    ui: &CatalogUi,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
) -> Html {
    html! {
        <div class="studio-grid">
            {creator_form(profile, ui, status.clone(), on_update.clone())}
            {station_form(profile, ui, status.clone(), on_update.clone())}
            {show_form(profile, ui, status, on_update)}
        </div>
    }
}

fn creator_form(
    profile: &Profile,
    ui: &CatalogUi,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
) -> Html {
    let profile = profile.clone();
    let name = ui.creator_name.clone();
    let description = ui.creator_description.clone();
    let avatar = ui.creator_avatar.clone();
    let submit_name = name.clone();
    let submit_description = description.clone();
    let submit_avatar = avatar.clone();
    let submit_status = status.clone();
    let submit = Callback::from(move |_| {
        match crate::controllers::studio::upsert_creator(
            &profile,
            &submit_name,
            &submit_description,
            submit_avatar.as_ref().cloned(),
        ) {
            Ok(patch) => {
                on_update.emit(patch);
                submit_status.set("Creator profile saved.".into());
            }
            Err(error) => submit_status.set(error),
        }
    });
    html! {
        <form class="card form-grid" onsubmit={prevent_submit()}>
            <h3>{"Creator profile"}</h3>
            <Avatar label={(*name).clone()} reference={(*avatar).clone()} size={72}/>
            {image_upload("Creator avatar", avatar, status.clone())}
            <label>{"Display name"}<input value={(*name).clone()} oninput={text_input(name)} /></label>
            <label>{"Description"}<textarea value={(*description).clone()} oninput={crate::components::form::textarea_input(description)} /></label>
            <button type="submit" class="primary" onclick={submit}>{"Save creator"}</button>
        </form>
    }
}

fn station_form(
    profile: &Profile,
    ui: &CatalogUi,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
) -> Html {
    let profile_owned = profile.clone();
    let selected = ui.creator_id.clone();
    let name = ui.station_name.clone();
    let description = ui.station_description.clone();
    let artwork = ui.station_artwork.clone();
    let room_id = ui.station_room_id.clone();
    let submit = station_submit(profile_owned, ui, status.clone(), on_update);
    html! {
        <form class="card form-grid" onsubmit={prevent_submit()}>
            <h3>{"Station"}</h3>
            <Avatar label={(*name).clone()} reference={(*artwork).clone()} size={72}/>
            {image_upload("Station artwork", artwork, status)}
            {creator_select(profile, selected)}
            {broadcast_room_select(profile, room_id)}
            <label>{"Name"}<input value={(*name).clone()} oninput={text_input(name)} /></label>
            <label>{"Description"}<textarea value={(*description).clone()} oninput={crate::components::form::textarea_input(description)} /></label>
            <button type="submit" disabled={profile.broadcast_catalog().creators().is_empty()} onclick={submit}>{"Save station"}</button>
        </form>
    }
}

fn station_submit(
    profile: Profile,
    ui: &CatalogUi,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
) -> Callback<MouseEvent> {
    let creator_id = ui.creator_id.clone();
    let name = ui.station_name.clone();
    let description = ui.station_description.clone();
    let artwork = ui.station_artwork.clone();
    let room_id = ui.station_room_id.clone();
    Callback::from(move |_| {
        match crate::controllers::studio::upsert_station(
            &profile,
            &creator_id,
            &name,
            &description,
            artwork.as_ref().cloned(),
            Some(&room_id),
        ) {
            Ok(patch) => {
                on_update.emit(patch);
                status.set("Station profile saved.".into());
            }
            Err(error) => status.set(error),
        }
    })
}

fn show_form(
    profile: &Profile,
    ui: &CatalogUi,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
) -> Html {
    let profile_owned = profile.clone();
    let creator_id = ui.creator_id.clone();
    let station_id = ui.show_station_id.clone();
    let title = ui.show_title.clone();
    let description = ui.show_description.clone();
    let artwork = ui.show_artwork.clone();
    let submit_status = status.clone();
    let submit = Callback::from(move |_| {
        match crate::controllers::studio::upsert_show(
            &profile_owned,
            &creator_id,
            &station_id,
            &title,
            &description,
            artwork.as_ref().cloned(),
        ) {
            Ok(patch) => {
                on_update.emit(patch);
                submit_status.set("Podcast show saved.".into());
            }
            Err(error) => submit_status.set(error),
        }
    });
    html! {
        <form class="card form-grid" onsubmit={prevent_submit()}>
            <h3>{"Podcast show"}</h3>
            <Avatar label={(*ui.show_title).clone()} reference={(*ui.show_artwork).clone()} size={72}/>
            {image_upload("Show artwork", ui.show_artwork.clone(), status.clone())}
            {creator_select(profile, ui.creator_id.clone())}
            {station_select(profile, ui.show_station_id.clone())}
            <label>{"Title"}<input value={(*ui.show_title).clone()} oninput={text_input(ui.show_title.clone())} /></label>
            <label>{"Description"}<textarea value={(*ui.show_description).clone()} oninput={crate::components::form::textarea_input(ui.show_description.clone())} /></label>
            <button type="submit" disabled={ui.show_station_id.is_empty()} onclick={submit}>{"Save show"}</button>
        </form>
    }
}

fn image_upload(
    label: &str,
    target: UseStateHandle<Option<MediaReference>>,
    status: UseStateHandle<String>,
) -> Html {
    let on_media = Callback::from(move |reference| target.set(Some(reference)));
    let on_error = Callback::from(move |error| status.set(error));
    html! { <MediaUpload label={label.to_owned()} accept={"image/*".to_string()} content_type_prefix={"image/".to_string()} {on_media} {on_error}/> }
}

fn creator_select(profile: &Profile, selected: UseStateHandle<String>) -> Html {
    let on_value = {
        let selected = selected.clone();
        Callback::from(move |value| selected.set(value))
    };
    html! { <label>{"Creator"}<select value={(*selected).clone()} onchange={select_value(on_value)}>
        <option value="">{"Select creator"}</option>
        {for profile.broadcast_catalog().creators().iter().map(|creator| html! { <option value={creator.id.clone()}>{creator.display_name.clone()}</option> })}
    </select></label> }
}

fn station_select(profile: &Profile, selected: UseStateHandle<String>) -> Html {
    let on_value = {
        let selected = selected.clone();
        Callback::from(move |value| selected.set(value))
    };
    html! { <label>{"Station"}<select value={(*selected).clone()} onchange={select_value(on_value)}>
        <option value="">{"Select station"}</option>
        {for profile.broadcast_catalog().stations().iter().map(|station| html! { <option value={station.id.clone()}>{station.name.clone()}</option> })}
    </select></label> }
}

fn broadcast_room_select(profile: &Profile, selected: UseStateHandle<String>) -> Html {
    let on_value = {
        let selected = selected.clone();
        Callback::from(move |value| selected.set(value))
    };
    html! { <label>{"Live radio Room"}<select value={(*selected).clone()} onchange={select_value(on_value)}>
        <option value="">{"Not live"}</option>
        {for profile.rooms.iter().filter(|room| room.broadcast_enabled()).map(|room| html! { <option value={room.id.clone()}>{room.name.clone()}</option> })}
    </select></label> }
}
