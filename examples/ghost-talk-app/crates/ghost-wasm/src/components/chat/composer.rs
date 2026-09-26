use super::view::{ChatProps, ChatUiState};
use super::{input, prevent_submit};
use crate::{model::Chat, voice};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

pub(crate) fn render_composer(
    state: &ChatUiState,
    chat: &Chat,
    send_body: Callback<String>,
) -> Html {
    let closed = chat_composer_closed(chat);
    let masked_draft = "*".repeat(state.draft.chars().count());
    html! {
        <form class="composer" onsubmit={prevent_submit()}>
            <button type="button" title="Record voice message" class={(*state.recording).then_some("voice-recording")} onclick={recording_callback(state, send_body.clone())} disabled={closed}>{if *state.recording {"■"} else {"◉"}}</button>
            <div class="composer-input-wrap">
                <input
                    class={(*state.mask_messages).then_some("masked-composer-input")}
                    type="text"
                    value={(*state.draft).clone()}
                    oninput={input(state.draft.clone())}
                    placeholder="Message…"
                    disabled={closed}
                    aria-label="Message"
                />
                {if *state.mask_messages && !state.draft.is_empty() {
                    html! { <span class="composer-mask-display" aria-hidden="true">{masked_draft}</span> }
                } else {
                    Html::default()
                }}
            </div>
            <button type="submit" class="primary" onclick={send_text_callback(state, send_body)} disabled={state.draft.trim().is_empty() || *state.busy || closed}>{"Send"}</button>
        </form>
    }
}

pub(crate) fn chat_composer_closed(chat: &Chat) -> bool {
    chat.archived()
        || chat.left()
        || chat.peer_left()
        || chat
            .incoming_request()
            .is_some_and(|request| request.call_id.is_none() && request.state != "accepted")
}

pub(crate) fn send_text_callback(
    state: &ChatUiState,
    sender: Callback<String>,
) -> Callback<MouseEvent> {
    let draft = state.draft.clone();
    Callback::from(move |_| {
        let body = draft.trim().to_string();
        if body.is_empty() {
            return;
        }
        sender.emit(body);
        draft.set(String::new());
    })
}

pub(crate) fn recording_callback(
    state: &ChatUiState,
    sender: Callback<String>,
) -> Callback<MouseEvent> {
    let recording = state.recording.clone();
    let status = state.status.clone();
    Callback::from(move |_| {
        if *recording {
            stop_recording(recording.clone(), status.clone());
            return;
        }
        start_recording(recording.clone(), status.clone(), sender.clone());
    })
}

pub(crate) fn stop_recording(recording: UseStateHandle<bool>, status: UseStateHandle<String>) {
    match voice::stop_clip_recording() {
        Ok(()) => {
            recording.set(false);
            status.set("Finishing voice message…".into());
        }
        Err(error) => status.set(error),
    }
}

pub(crate) fn start_recording(
    recording: UseStateHandle<bool>,
    status: UseStateHandle<String>,
    sender: Callback<String>,
) {
    spawn_local(async move {
        match voice::start_clip_recording(move |body| sender.emit(body)).await {
            Ok(()) => {
                recording.set(true);
                status.set("Recording voice message… click again to send.".into());
            }
            Err(error) => status.set(error),
        }
    });
}

pub(crate) fn send_body_callback(
    props: &ChatProps,
    state: &ChatUiState,
    chat: Chat,
) -> Callback<String> {
    let durable = durable_send_callback(props, state, chat.clone());
    super::direct_send::route_text(props, state, chat, durable)
}

/// Kaspa-anchored KKTP send (the durable default).
fn durable_send_callback(props: &ChatProps, state: &ChatUiState, chat: Chat) -> Callback<String> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |body: String| {
        if password.is_empty() || *busy {
            return;
        }
        let prepared = match crate::controllers::chat::prepare_send(&profile, &chat, body) {
            Ok(value) => value,
            Err(error) => {
                status.set(error);
                return;
            }
        };
        on_update.emit(prepared.patch);
        if prepared.establishing {
            status.set(
                "Message queued. The existing encrypted HYDRA session is still being established."
                    .into(),
            );
            return;
        }
        busy.set(true);
        status.set(if prepared.needs_request {
            "Sending private chat request…".into()
        } else {
            "Sending encrypted message…".into()
        });
        let password = password.clone();
        let status = status.clone();
        let busy = busy.clone();
        let on_update = on_update.clone();
        spawn_local(async move {
            let outcome = crate::controllers::chat::execute_send(prepared.plan, &password).await;
            on_update.emit(outcome.patch);
            status.set(outcome.status);
            busy.set(false);
        });
    })
}
