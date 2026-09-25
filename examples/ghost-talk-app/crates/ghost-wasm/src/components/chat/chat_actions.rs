use super::view::{spawn_local, Chat, ChatProps, ChatUiState};
use yew::prelude::*;

pub(crate) fn accept_request_callback(
    props: &ChatProps,
    state: &ChatUiState,
    chat: Chat,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        let Some(request) = chat.incoming_request().cloned() else {
            return;
        };
        if *busy {
            return;
        }
        busy.set(true);
        status.set("Accepting secure chat request…".into());
        let profile = profile.clone();
        let password = password.clone();
        let chat = chat.clone();
        let status = status.clone();
        let busy = busy.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            match crate::controllers::chat::accept_request(&profile, &password, &chat, &request)
                .await
            {
                Ok(patch) => {
                    on_update.emit(patch);
                    status.set("Accepted. Establishing the encrypted HYDRA session…".into());
                }
                Err(error) => status.set(error),
            }
            busy.set(false);
        });
    })
}

pub(crate) fn ignore_request_callback(props: &ChatProps, chat: Chat) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        on_update.emit(crate::controllers::chat::incoming_request_state_patch(
            &profile, &chat.id, "ignored",
        ));
    })
}

pub(crate) fn archive_callback(props: &ChatProps, chat: Chat) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        on_update.emit(crate::controllers::chat::toggle_archive_patch(
            &profile, &chat.id,
        ));
    })
}

pub(crate) fn add_contact_callback(
    props: &ChatProps,
    state: &ChatUiState,
    chat: Chat,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    let status = state.status.clone();
    Callback::from(
        move |_| match crate::controllers::chat::add_contact_patch(&profile, &chat) {
            Ok(patch) => {
                on_update.emit(patch);
                status.set("Added to contacts.".into());
            }
            Err(error) => status.set(error),
        },
    )
}

pub(crate) fn chat_setting_callback(props: &ChatProps, field: &'static str) -> Callback<Event> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    crate::components::form::select_value(Callback::from(move |value: String| {
        if let Ok(patch) = crate::controllers::account::update_setting(&profile, field, value) {
            on_update.emit(patch);
        }
    }))
}
