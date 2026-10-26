//! Retry for a failed outgoing message: the same text is routed again under
//! its original wire id, so a copy that did land is deduplicated by the peer.

use yew::prelude::*;

use super::outgoing::{OutgoingText, TextSender};
use crate::model::Message;

pub(crate) fn render_retry(message: &Message, send: &TextSender) -> Html {
    if !message.is_retryable() {
        return Html::default();
    }
    let text = OutgoingText {
        body: message.body.clone(),
        retry_wire_id: Some(
            message
                .wire_id
                .clone()
                .unwrap_or_else(|| message.id.clone()),
        ),
    };
    let send = send.clone();
    let onclick = Callback::from(move |_: MouseEvent| {
        send.emit(text.clone());
    });
    html! {<button type="button" class="message-retry" onclick={onclick}>{"Retry"}</button>}
}
