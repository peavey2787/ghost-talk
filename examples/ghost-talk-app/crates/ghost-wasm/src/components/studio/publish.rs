use crate::{
    components::form::{prevent_submit, select_value, text_input},
    model::{Profile, ProfilePatch},
};
use ghost_media::MediaReference;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Clone)]
pub(super) struct PublishUi {
    pub(super) show_id: UseStateHandle<String>,
    pub(super) title: UseStateHandle<String>,
    pub(super) description: UseStateHandle<String>,
    pub(super) tags: UseStateHandle<String>,
    pub(super) recording: UseStateHandle<Option<MediaReference>>,
}

pub(super) fn render(
    profile: &Profile,
    password: &str,
    duration_ms: UseStateHandle<u64>,
    ui: &PublishUi,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
) -> Html {
    let show_id = ui.show_id.clone();
    let on_value = {
        let show_id = show_id.clone();
        Callback::from(move |value| show_id.set(value))
    };
    let submit = submit_callback(profile, password, duration_ms, ui, status, on_update);
    html! {
        <form class="card form-grid" onsubmit={prevent_submit()}>
            <h3>{"Publish recording"}</h3>
            <label>{"Show"}<select value={(*show_id).clone()} onchange={select_value(on_value)}>
                <option value="">{"Select show"}</option>
                {for profile.broadcast_catalog().shows().iter().map(|show| html! {
                    <option value={show.id.clone()}>{show.title.clone()}</option>
                })}
            </select></label>
            <label>{"Episode title"}<input value={(*ui.title).clone()} oninput={text_input(ui.title.clone())} /></label>
            <label>{"Description"}<textarea value={(*ui.description).clone()} oninput={crate::components::form::textarea_input(ui.description.clone())} /></label>
            <label>{"Tags"}<input value={(*ui.tags).clone()} oninput={text_input(ui.tags.clone())} placeholder="kaspa, radio, technology" /></label>
            <button type="submit" class="primary" disabled={ui.recording.is_none() || ui.show_id.is_empty()} onclick={submit}>{"Publish as podcast"}</button>
        </form>
    }
}

fn submit_callback(
    profile: &Profile,
    password: &str,
    duration_ms: UseStateHandle<u64>,
    ui: &PublishUi,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
) -> Callback<MouseEvent> {
    let profile = profile.clone();
    let password = password.to_owned();
    let ui = ui.clone();
    Callback::from(move |_| {
        let Some(media) = ui.recording.as_ref().cloned() else {
            status.set("Record a broadcast before publishing an episode.".into());
            return;
        };
        let profile = profile.clone();
        let password = password.clone();
        let ui = ui.clone();
        let status = status.clone();
        let on_update = on_update.clone();
        let duration = *duration_ms;
        spawn_local(async move {
            let request = crate::controllers::studio::EpisodePublishRequest {
                show_id: (*ui.show_id).clone(),
                title: (*ui.title).clone(),
                description: (*ui.description).clone(),
                tags: (*ui.tags).clone(),
                duration_ms: duration,
                media,
            };
            match crate::controllers::studio::publish_episode(&profile, &password, request).await {
                Ok(patch) => finish_publish(&ui, &status, &on_update, patch),
                Err(error) => status.set(error),
            }
        });
    })
}

fn finish_publish(
    ui: &PublishUi,
    status: &UseStateHandle<String>,
    on_update: &Callback<ProfilePatch>,
    patch: ProfilePatch,
) {
    on_update.emit(patch);
    status.set("Podcast episode published with a signed media manifest.".into());
    ui.title.set(String::new());
    ui.description.set(String::new());
    ui.tags.set(String::new());
}
