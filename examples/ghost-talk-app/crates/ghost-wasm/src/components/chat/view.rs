use super::prevent_submit;
use super::{
    composer::{render_composer, send_body_callback},
    message_rendering::{preview, toggle_bool},
};
pub(crate) use crate::{
    components::recipient_input::RecipientInput,
    format_message_timestamp,
    model::{Chat, Message, Profile, ProfilePatch, WalletProjection},
    voice,
};
pub(crate) use wasm_bindgen_futures::spawn_local;
pub(crate) use web_sys::Element;
use yew::prelude::*;
mod details;
use details::{
    render_advanced, render_chat_empty, render_chat_header, render_composer_status,
    render_messages, render_request_banner,
};

#[derive(Properties, PartialEq)]
pub struct ChatProps {
    pub profile: Profile,
    pub password: String,
    pub selected_chat_id: String,
    pub on_select: Callback<String>,
    pub on_update: Callback<ProfilePatch>,
    pub on_wallet_public: Callback<WalletProjection>,
    pub on_start_chat: Callback<String>,
}

#[derive(Clone)]
pub(crate) struct ChatUiState {
    pub(crate) target: UseStateHandle<String>,
    pub(crate) draft: UseStateHandle<String>,
    pub(crate) status: UseStateHandle<String>,
    pub(crate) busy: UseStateHandle<bool>,
    pub(crate) recording: UseStateHandle<bool>,
    pub(crate) show_archived: UseStateHandle<bool>,
    pub(crate) mask_messages: UseStateHandle<bool>,
    pub(crate) show_advanced: UseStateHandle<bool>,
    pub(crate) messages_ref: NodeRef,
}

#[component(ChatView)]
pub fn chat_view(props: &ChatProps) -> Html {
    let state = ChatUiState {
        target: use_state(String::new),
        draft: use_state(String::new),
        status: use_state(String::new),
        busy: use_state(|| false),
        recording: use_state(|| false),
        show_archived: use_state(|| false),
        mask_messages: use_state(|| false),
        show_advanced: use_state(|| false),
        messages_ref: use_node_ref(),
    };
    let selected = selected_chat(props);
    use_selected_chat_status_reset(selected.as_ref(), state.status.clone());
    use_established_status(selected.as_ref(), state.status.clone());
    use_message_autoscroll(selected.as_ref(), state.messages_ref.clone());
    let visible = visible_chats(&props.profile, *state.show_archived);
    html! {
        <section class={classes!("chat-layout", selected.is_some().then_some("has-chat"))}>
            {render_thread_list(props, &state, &visible)}
            {render_selected_chat(props, &state, selected)}
        </section>
    }
}

pub(crate) fn selected_chat(props: &ChatProps) -> Option<Chat> {
    props
        .profile
        .chats
        .iter()
        .find(|chat| chat.id == props.selected_chat_id && !chat.room_transport_only())
        .cloned()
}

#[hook]
pub(crate) fn use_selected_chat_status_reset(
    selected: Option<&Chat>,
    status: UseStateHandle<String>,
) {
    let selected_id = selected.map(|chat| chat.id.clone());
    use_effect_with(selected_id, move |_| {
        if !status.is_empty() {
            status.set(String::new());
        }
        || ()
    });
}

#[hook]
pub(crate) fn use_established_status(selected: Option<&Chat>, status: UseStateHandle<String>) {
    let session_state = selected.map(|chat| (chat.id.clone(), chat.bootstrap_complete()));
    use_effect_with(session_state, move |state| {
        if state.as_ref().is_some_and(|(_, active)| *active)
            && (*status).starts_with("Accepted. Establishing")
        {
            status.set("Encrypted HYDRA session established.".into());
        }
        || ()
    });
}

#[hook]
pub(crate) fn use_message_autoscroll(selected: Option<&Chat>, messages_ref: NodeRef) {
    let message_state = selected.map(|chat| {
        (
            chat.id.clone(),
            chat.messages().len(),
            chat.messages().last().map(|message| message.id.clone()),
        )
    });
    use_effect_with(message_state, move |_| {
        if let Some(element) = messages_ref.cast::<Element>() {
            element.set_scroll_top(element.scroll_height());
        }
        || ()
    });
}

pub(crate) fn visible_chats(profile: &Profile, show_archived: bool) -> Vec<Chat> {
    profile
        .chats
        .iter()
        .filter(|chat| chat_is_visible(chat, show_archived))
        .cloned()
        .collect()
}

pub(crate) fn chat_is_visible(chat: &Chat, show_archived: bool) -> bool {
    if chat.room_transport_only() {
        return false;
    }
    let ignored = chat
        .incoming_request()
        .is_some_and(|request| request.call_id.is_none() && request.state == "ignored");
    if show_archived {
        chat.archived() || ignored
    } else {
        !chat.archived() && !ignored
    }
}

pub(crate) fn render_thread_list(props: &ChatProps, state: &ChatUiState, visible: &[Chat]) -> Html {
    html! {
        <aside class="thread-list">
            <div class="thread-list-head"><h3>{"Chats"}</h3><button onclick={toggle_bool(state.show_archived.clone())}>{if *state.show_archived {"Active"} else {"Archived"}}</button></div>
            <form class="inline-form compact-start" onsubmit={prevent_submit()}>
                <RecipientInput profile={props.profile.clone()} value={(*state.target).clone()} on_change={{let target=state.target.clone(); Callback::from(move|value:String|target.set(value))}} placeholder="Contact, KNS, dot.k, or Kaspa address" />
                <button type="submit" class="primary" onclick={start_chat_callback(props, state)} disabled={state.target.trim().is_empty() || props.password.is_empty()}>{"+"}</button>
            </form>
            {render_thread_rows(props, visible)}
        </aside>
    }
}

pub(crate) fn render_thread_rows(props: &ChatProps, visible: &[Chat]) -> Html {
    if visible.is_empty() {
        return html! {<p class="muted thread-empty">{"No chats yet."}</p>};
    }
    html! {<>{for visible.iter().map(|chat| render_thread_row(props, chat))}</>}
}

pub(crate) fn render_thread_row(props: &ChatProps, chat: &Chat) -> Html {
    let view = crate::view_models::ThreadViewModel::from(chat);
    let id = view.id.clone();
    let on_select = props.on_select.clone();
    let subtitle = view
        .last_body
        .as_deref()
        .map(preview)
        .unwrap_or_else(|| thread_empty_preview(&view));
    html! {
        <button class={classes!("thread-row", (props.selected_chat_id == view.id).then_some("active"))} onclick={Callback::from(move |_| on_select.emit(id.clone()))}>
            <span class="avatar">{view.label.chars().next().unwrap_or('G').to_ascii_uppercase()}</span>
            <span class="thread-row-text"><b>{view.label.clone()}</b><small>{subtitle}</small></span>
            {render_thread_attention(&view, props.selected_chat_id == view.id)}
        </button>
    }
}

fn thread_empty_preview(view: &crate::view_models::ThreadViewModel) -> String {
    if view.incoming_call {
        "Incoming voice call".into()
    } else if view.incoming_pending {
        "Incoming chat request".into()
    } else {
        "New conversation".into()
    }
}

pub(crate) fn render_thread_attention(
    view: &crate::view_models::ThreadViewModel,
    selected: bool,
) -> Html {
    if !selected && view.unread_count() > 0 {
        return html! { <span class="nav-badge">{view.unread_count()}</span> };
    }
    if view.incoming_pending {
        html! { <span class="request-dot"></span> }
    } else {
        Html::default()
    }
}

pub(crate) fn start_chat_callback(props: &ChatProps, state: &ChatUiState) -> Callback<MouseEvent> {
    let target = state.target.clone();
    let on_start = props.on_start_chat.clone();
    Callback::from(move |_| {
        let value = target.trim().to_string();
        if value.is_empty() {
            return;
        }
        on_start.emit(value);
        target.set(String::new());
    })
}

pub(crate) fn render_selected_chat(
    props: &ChatProps,
    state: &ChatUiState,
    selected: Option<Chat>,
) -> Html {
    let Some(chat) = selected else {
        return render_chat_empty();
    };
    let send_body = send_body_callback(props, state, chat.clone());
    html! {
        <div class="chat">
            {render_chat_header(props, state, &chat)}
            {render_request_banner(props, state, &chat)}
            {render_advanced(props, state, &chat)}
            {render_messages(props, state, &chat)}
            {render_composer_status(&state.status)}
            {render_composer(state, &chat, send_body)}
        </div>
    }
}
