//! System notifications for new messages. Only the sender (or Room) name is
//! shown: message text never leaves the app. The same Web Notification API
//! serves the browser build and Tauri, whose notification plugin backs it on
//! desktop and mobile.

use wasm_bindgen_futures::JsFuture;
use web_sys::{Notification, NotificationOptions, NotificationPermission};

use crate::model::Profile;

const DENIED: &str = "Notifications are blocked for Ghost Talk. Allow them in your system or browser settings, then turn this on again.";

/// The newest incoming chat or Room message: its time and where it came from.
pub(crate) fn newest_incoming(profile: &Profile) -> Option<(f64, String)> {
    let own = profile.hydra_identity_id.as_deref();
    let chats = profile.chats.iter().flat_map(|chat| {
        chat.messages()
            .iter()
            .filter(|message| message.direction == "in")
            .map(move |message| (message.created_at, chat.label.clone()))
    });
    let rooms = profile.rooms.iter().flat_map(|room| {
        room.messages()
            .iter()
            .filter(move |message| Some(message.sender_hydra_id.as_str()) != own)
            .map(move |message| (message.created_at, room.name.clone()))
    });
    chats.chain(rooms).max_by(|a, b| a.0.total_cmp(&b.0))
}

/// Ask once for permission; `Err` explains a refusal to the user.
pub(crate) async fn request_permission() -> Result<(), String> {
    let promise = Notification::request_permission().map_err(crate::native::js_error)?;
    let answer = JsFuture::from(promise)
        .await
        .map_err(crate::native::js_error)?;
    (answer.as_string().as_deref() == Some("granted"))
        .then_some(())
        .ok_or_else(|| DENIED.to_string())
}

/// The user is looking at Ghost Talk right now.
fn in_foreground() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| !document.hidden() && document.has_focus().unwrap_or(false))
}

/// Notify about a new message from `source` unless the app is in front.
pub(crate) fn notify_new_message(source: &str) {
    if in_foreground() || Notification::permission() != NotificationPermission::Granted {
        return;
    }
    let options = NotificationOptions::new();
    options.set_body(&format!("New message from {source}"));
    options.set_tag("ghost-talk-message");
    let _ = Notification::new_with_options("Ghost Talk", &options);
}
