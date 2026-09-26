use super::view::{format_message_timestamp, voice, Message};
use crate::model::ReactionKind;
use yew::prelude::*;
pub(crate) fn render_message(message: &Message, masked: bool) -> Html {
    let timestamp = format_message_timestamp(message.created_at);
    if masked {
        return html! {<p class={classes!(message_side(message),"masked-message")}>{"••••••••"}<small class="message-time">{timestamp}</small></p>};
    }
    if let Some((mime, data)) = voice::decode_voice_message(&message.body) {
        return render_voice_message(message, mime, data, timestamp);
    }
    render_text_message(message, timestamp)
}

pub(crate) fn message_side(message: &Message) -> &'static str {
    if message.direction == "out" {
        "me"
    } else {
        "them"
    }
}

pub(crate) fn render_voice_message(
    message: &Message,
    mime: String,
    data: String,
    timestamp: String,
) -> Html {
    let src = format!("data:{mime};base64,{data}");
    html! {<div class={classes!("voice-message",message_side(message))}><audio controls=true src={src}></audio><small class="message-time">{timestamp}</small>{render_carrier(message)}<small class={classes!("message-state",message.send_state.clone().unwrap_or_default())}>{message.send_state.clone().unwrap_or_default()}</small></div>}
}

pub(crate) fn render_text_message(message: &Message, timestamp: String) -> Html {
    html! {<p class={classes!(message_side(message),(message.send_state.as_deref()==Some("failed")).then_some("failed-message"),(!message.stored_on_kaspa()).then_some("ephemeral-message"))}>{message.body.clone()}<small class="message-time">{timestamp}</small>{render_carrier(message)}{render_message_state(message)}</p>}
}

/// Ephemeral p2p-net messages are visibly marked: they were never stored on
/// Kaspa and cannot be recovered from chain history.
pub(crate) fn render_carrier(message: &Message) -> Html {
    if message.stored_on_kaspa() {
        return Html::default();
    }
    html! {<small class="message-carrier" title="Delivered directly over p2p-net. Not stored on Kaspa; it cannot be recovered from chain history.">{"p2p · not stored on Kaspa"}</small>}
}

pub(crate) fn render_message_state(message: &Message) -> Html {
    let Some(state) = message.send_state.clone() else {
        return Html::default();
    };
    let text = message
        .send_error
        .clone()
        .map(|error| format!("{state} · {error}"))
        .unwrap_or_else(|| state.clone());
    html! {<small class={classes!("message-state",state)}>{text}</small>}
}

pub(crate) fn preview(body: &str) -> String {
    if voice::decode_voice_message(body).is_some() {
        return "Voice message".into();
    }
    let mut text = body.chars().take(44).collect::<String>();
    if body.chars().count() > 44 {
        text.push('…');
    }
    text
}

pub(crate) fn toggle_bool(state: UseStateHandle<bool>) -> Callback<MouseEvent> {
    Callback::from(move |_| state.set(!*state))
}
pub(crate) fn clear_selection(on_select: Callback<String>) -> Callback<MouseEvent> {
    Callback::from(move |_| on_select.emit(String::new()))
}

pub(crate) fn render_reaction_summary(message: &Message, own_actor: Option<&str>) -> Html {
    let own_reaction = own_actor.and_then(|actor| message.reaction_for(actor));
    crate::components::chat::reaction_ui::render_summary(&message.reactions, own_reaction)
}

pub(crate) fn render_reaction_picker(
    message: &Message,
    own_actor: Option<&str>,
    on_react: Callback<ReactionKind>,
) -> Html {
    let own_reaction = own_actor.and_then(|actor| message.reaction_for(actor));
    crate::components::chat::reaction_ui::render_picker(own_reaction, on_react)
}
