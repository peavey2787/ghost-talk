use super::{Chat, ChatProps, ChatUiState};
use crate::components::{
    call::CallButton,
    chat::{
        chat_actions::{
            accept_request_callback, add_contact_callback, archive_callback, chat_setting_callback,
            ignore_request_callback,
        },
        lifecycle::{delete_request_callback, leave_chat_callback, rejoin_chat_callback},
        message_rendering::{
            clear_selection, render_message, render_reaction_picker, render_reaction_summary,
            toggle_bool,
        },
    },
};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

pub(super) fn render_chat_empty() -> Html {
    html! {<div class="empty-state"><div class="empty-icon">{"◉"}</div><h2>{"Chats"}</h2><p>{"Start a private conversation with a Kaspa address, KNS name, or dot.k name."}</p></div>}
}

pub(super) fn render_chat_header(props: &ChatProps, state: &ChatUiState, chat: &Chat) -> Html {
    html! {<header><button type="button" class="mobile-chat-back" onclick={clear_selection(props.on_select.clone())}>{"← Chats"}</button><div><strong>{chat.label.clone()}{render_verified(chat.verified_public())}</strong><small>{chat.peer_kns_name().map(str::to_owned).or(chat.peer_dotk_name().map(str::to_owned)).or(chat.peer_kaspa_address().map(str::to_owned)).unwrap_or_else(||"Secure Ghost Talk chat".into())}</small></div><div class="thread-actions"><CallButton chat={chat.clone()} /><button onclick={toggle_bool(state.mask_messages.clone())}>{if *state.mask_messages {"Show"} else {"Mask"}}</button><button onclick={toggle_bool(state.show_advanced.clone())}>{"Advanced"}</button>{render_add_contact_action(props, state, chat)}{render_leave_action(props, state, chat)}<button onclick={archive_callback(props, chat.clone())}>{if chat.archived() {"Unarchive"} else {"Archive"}}</button></div></header>}
}

fn render_verified(verified: bool) -> Html {
    if verified {
        html! {<span class="verified-mark">{"✓"}</span>}
    } else {
        Html::default()
    }
}

fn render_add_contact_action(props: &ChatProps, state: &ChatUiState, chat: &Chat) -> Html {
    if chat.contact_id().is_some() {
        return Html::default();
    }
    html! {<button onclick={add_contact_callback(props, state, chat.clone())}>{"Add contact"}</button>}
}

fn render_leave_action(props: &ChatProps, state: &ChatUiState, chat: &Chat) -> Html {
    if chat.left() || chat.peer_left() {
        html! {<button onclick={rejoin_chat_callback(props, state, chat.clone())}>{"Rejoin"}</button>}
    } else {
        html! {<button class="danger-link" onclick={leave_chat_callback(props, state, chat.clone())}>{"Leave"}</button>}
    }
}

pub(super) fn render_request_banner(props: &ChatProps, state: &ChatUiState, chat: &Chat) -> Html {
    let Some(request) = chat.incoming_request() else {
        return Html::default();
    };
    if request.call_id.is_some() || (request.state != "pending" && request.state != "ignored") {
        return Html::default();
    }
    let ignored = request.state == "ignored";
    html! {<div class={classes!("chat-request-banner", ignored.then_some("ignored"))}><div><b>{if ignored {"Ignored incoming request"} else {"Incoming secure chat request"}}</b><small>{request.peer_address.clone()}</small></div><div class="button-row"><button class="primary" onclick={accept_request_callback(props, state, chat.clone())} disabled={props.password.is_empty() || *state.busy}>{"Accept"}</button><button onclick={ignore_request_callback(props, chat.clone())}>{"Ignore"}</button>{render_delete_request(props, chat, ignored)}</div></div>}
}

fn render_delete_request(props: &ChatProps, chat: &Chat, ignored: bool) -> Html {
    if ignored {
        html! {<button class="danger-link" onclick={delete_request_callback(props, chat.clone())}>{"Delete"}</button>}
    } else {
        Html::default()
    }
}

pub(super) fn render_advanced(props: &ChatProps, state: &ChatUiState, chat: &Chat) -> Html {
    if !*state.show_advanced {
        return Html::default();
    }
    html! {
        <div class="advanced-drawer">
            <label>{"Realtime route"}<select value={props.profile.settings.route.clone()} onchange={chat_setting_callback(props, "route")}><option value="Auto">{"Auto"}</option><option value="Kaspa only">{"Kaspa only"}</option></select></label>
            <label>{"Text messages"}<select value={props.profile.settings.text_route.clone()} onchange={chat_setting_callback(props, "textRoute")}><option value="Kaspa">{"Kaspa (stored)"}</option><option value="P2P preferred">{"p2p-net preferred"}</option><option value="P2P only">{"p2p-net only"}</option></select></label>
            <label>{"Steganography"}<select value={props.profile.settings.stego.clone()} onchange={chat_setting_callback(props, "stego")}><option>{"Off"}</option><option>{"Deterministic"}</option><option>{"Fast Unicode"}</option><option>{"Fast Hybrid"}</option><option>{"Arithmetic"}</option></select></label>
            <small>{if chat.bootstrap_complete() {"🛡 Ghost PQ secure session active."} else {"🛡 Ghost PQ session establishing or inactive."}}</small>
            {render_kasia_compatibility(props, chat)}
        </div>
    }
}

fn render_kasia_compatibility(props: &ChatProps, chat: &Chat) -> Html {
    let Some(contact_id) = chat.contact_id() else {
        return html! { <small>{"Add this peer as a contact to enable explicit Kasia interoperability."}</small> };
    };
    html! {
        <crate::components::kasia::KasiaConversation
            profile={props.profile.clone()}
            password={props.password.clone()}
            contact_id={contact_id.to_owned()}
            on_update={props.on_update.clone()}
        />
    }
}

pub(super) fn render_messages(props: &ChatProps, state: &ChatUiState, chat: &Chat) -> Html {
    if chat.messages().is_empty() {
        return html! {<div class="messages" ref={state.messages_ref.clone()}><div class="inline-empty">{"Send the first message to establish or use the secure session."}</div></div>};
    }
    let own_actor = props.profile.hydra_identity_id.as_deref();
    html! {
        <div class="messages" ref={state.messages_ref.clone()}>
            {for chat.messages().iter().map(|message| {
                let picker = reaction_callback(props, state, chat, message);
                html! {
                    <div class={classes!("message-with-reactions", crate::components::chat::message_rendering::message_side(message))}>
                        <div class="message-bubble-wrap">
                            {render_message(message, *state.mask_messages)}
                            {render_reaction_picker(message, own_actor, picker)}
                        </div>
                        {render_reaction_summary(message, own_actor)}
                    </div>
                }
            })}
        </div>
    }
}

fn reaction_callback(
    props: &ChatProps,
    state: &ChatUiState,
    chat: &Chat,
    message: &crate::model::Message,
) -> Callback<crate::model::ReactionKind> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let chat = chat.clone();
    let message = message.clone();
    let on_update = props.on_update.clone();
    let status = state.status.clone();
    Callback::from(move |kind| {
        let (optimistic, plan) = match crate::controllers::chat::reactions::prepare_reaction(
            &profile, &chat, &message, kind,
        ) {
            Ok(value) => value,
            Err(error) => {
                status.set(error);
                return;
            }
        };
        on_update.emit(optimistic);
        let on_update = on_update.clone();
        let status = status.clone();
        let password = password.clone();
        spawn_local(async move {
            match crate::controllers::chat::reactions::send_reaction(plan, &password).await {
                Ok(wallet_patch) => {
                    on_update.emit(wallet_patch);
                    status.set("Reaction submitted through Ghost PQ.".into());
                }
                Err((rollback, error)) => {
                    on_update.emit(rollback);
                    status.set(error);
                }
            }
        });
    })
}

pub(super) fn render_composer_status(status: &UseStateHandle<String>) -> Html {
    if status.is_empty() {
        Html::default()
    } else {
        html! {<div class="status composer-status">{(**status).clone()}</div>}
    }
}
