use super::{prevent_submit, spawn_local, DiscoverProps, Profile, ProfilePatch};
use super::avatar_editor::AvatarCropEditor;
use web_sys::{HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;

fn render_profile_fields(props: &DiscoverProps) -> Html {
    html! {
        <>
            <label>{"Public username"}<input value={props.profile.settings.public_username.clone()} maxlength="48" oninput={update_text_callback(props, "username")} placeholder="Optional username" /></label>
            <label>{"Description"}<textarea rows="3" value={props.profile.settings.public_description.clone()} maxlength="320" oninput={update_text_callback(props, "description")} placeholder="Optional short description" /></label>
            <label>{"Interests"}<input value={props.profile.settings.public_interests.clone()} oninput={update_text_callback(props, "interests")} placeholder="music, kaspa, games" /></label>
        </>
    }
}

fn render_publish_actions(
    props: &DiscoverProps,
    busy: &UseStateHandle<bool>,
    publish_public: Callback<MouseEvent>,
    unlist: Callback<MouseEvent>,
) -> Html {
    let disabled = props.password.is_empty() || **busy || props.profile.wallet.is_none();
    html! {
        <div class="button-row">
            <button type="submit" class="primary" {disabled} onclick={publish_public}>{"Publish / update"}</button>
            <button type="button" {disabled} onclick={unlist}>{"Make private / unlist"}</button>
        </div>
    }
}

pub(crate) fn render_profile_editor(
    props: &DiscoverProps,
    ghost_address: &str,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) -> Html {
    let publish_public = publish_callback(props, true, status.clone(), busy.clone());
    let unlist = publish_callback(props, false, status.clone(), busy.clone());
    html! {
        <form class="card form-grid discover-profile-card" onsubmit={prevent_submit()}>
            <h3>{"Your public profile"}</h3>
            {render_registered_address(ghost_address)}
            {render_profile_avatar(props, status.clone())}
            {render_profile_fields(props)}
            {render_publish_actions(props, &busy, publish_public, unlist)}
        </form>
    }
}

fn render_profile_avatar(props: &DiscoverProps, status: UseStateHandle<String>) -> Html {
    let on_update = props.on_update.clone();
    let profile = props.profile.clone();
    let upload_status = status.clone();
    let on_media = Callback::from(move |reference| {
        let patch = crate::controllers::discover::update_public_avatar(&profile, reference);
        on_update.emit(patch);
        upload_status.set(
            "Avatar imported. Publish / update to commit it to your signed public profile.".into(),
        );
    });
    let on_error = Callback::from(move |error| status.set(error));
    html! {
        <AvatarCropEditor
            label={props.profile.label.clone()}
            current={props.profile.public_avatar().cloned()}
            {on_media}
            {on_error}
        />
    }
}

pub(crate) fn render_registered_address(address: &str) -> Html {
    if address.is_empty() {
        return Html::default();
    }
    html! {
        <>
            <small>{"Stable Ghost Talk address"}</small>
            <code class="address-code">{address.to_string()}</code>
        </>
    }
}

pub(crate) fn update_text_callback(
    props: &DiscoverProps,
    field: &'static str,
) -> Callback<InputEvent> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |event: InputEvent| {
        let value = discover_input_value(field, event);
        if let Ok(patch) = crate::controllers::discover::update_public_text(&profile, field, value)
        {
            on_update.emit(patch);
        }
    })
}

pub(crate) fn discover_input_value(field: &str, event: InputEvent) -> String {
    if field == "description" {
        event.target_unchecked_into::<HtmlTextAreaElement>().value()
    } else {
        event.target_unchecked_into::<HtmlInputElement>().value()
    }
}

pub(crate) fn publish_callback(
    props: &DiscoverProps,
    discoverable: bool,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        if password.is_empty() || *busy {
            return;
        }
        busy.set(true);
        status.set(publish_pending_status(discoverable).into());
        publish_profile_async(
            profile.clone(),
            password.clone(),
            discoverable,
            status.clone(),
            busy.clone(),
            on_update.clone(),
        );
    })
}

pub(crate) fn publish_profile_async(
    profile: Profile,
    password: String,
    discoverable: bool,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
    on_update: Callback<ProfilePatch>,
) {
    spawn_local(async move {
        match crate::controllers::discover::publish_and_patch(&profile, &password, discoverable)
            .await
        {
            Ok((published, patch)) => {
                let message = publish_complete_status(discoverable, published.already_current);
                on_update.emit(patch);
                status.set(message.into());
            }
            Err(error) => status.set(error),
        }
        busy.set(false);
    });
}

pub(crate) fn publish_pending_status(discoverable: bool) -> &'static str {
    if discoverable {
        "Connecting to Kaspa and publishing public Ghost Talk profile…"
    } else {
        "Connecting to Kaspa and publishing private/unlisted update…"
    }
}

pub(crate) fn publish_complete_status(discoverable: bool, already_current: bool) -> &'static str {
    if already_current {
        "Public profile was already current."
    } else if discoverable {
        "Public profile published."
    } else {
        "Profile is now unlisted."
    }
}
