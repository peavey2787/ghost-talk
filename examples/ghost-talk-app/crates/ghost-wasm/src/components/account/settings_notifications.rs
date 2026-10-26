//! The message-notification toggle. Turning it on asks the system for
//! permission first and stays off, with the reason shown, when refused.

use web_sys::HtmlInputElement;
use yew::prelude::*;

use super::settings::{SettingsProps, SettingsState};

pub(super) fn render_notification_settings(props: &SettingsProps, state: &SettingsState) -> Html {
    html! {
        <div class="card form-grid notification-card"><h3>{"Notifications"}</h3>
          <label class="toggle-row"><input type="checkbox" class="message-notifications" checked={props.profile.settings.message_notifications} onchange={toggle_callback(props, state)}/><span><b>{"Notify me about new messages"}</b><small>{"Shows who sent a message while Ghost Talk is in the background. Message text is never shown in notifications."}</small></span></label>
        </div>
    }
}

fn toggle_callback(props: &SettingsProps, state: &SettingsState) -> Callback<Event> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    let status = state.status.clone();
    Callback::from(move |event: Event| {
        let checked = event.target_unchecked_into::<HtmlInputElement>().checked();
        let (profile, on_update, status) = (profile.clone(), on_update.clone(), status.clone());
        wasm_bindgen_futures::spawn_local(async move {
            if checked {
                if let Err(error) = crate::app::request_notification_permission().await {
                    status.set(error);
                    return;
                }
            }
            if let Ok(patch) = crate::controllers::account::update_boolean_setting(
                &profile,
                "messageNotifications",
                checked,
            ) {
                on_update.emit(patch);
            }
        });
    })
}
