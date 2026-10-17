use ghost_domain::settings::{SwarmRetention, MAX_RETENTION_MEGABYTES, MAX_RETENTION_MINUTES};
use web_sys::HtmlInputElement;
use yew::prelude::*;

use super::settings::SettingsProps;

/// How much sealed no-spend mail this client holds for peers that were
/// briefly offline.
pub(super) fn render_retention_settings(props: &SettingsProps) -> Html {
    let retention = props.profile.settings.swarm_retention;
    html! {
        <div class="card form-grid retention-settings-card"><h3>{"No-spend message relay"}</h3>
          <p>{"No-spend messages travel over the swarm. While this is on, your client keeps a rolling copy of the swarm's sealed messages so peers who were offline for a while can ask for what they missed. You cannot read them: they stay end-to-end encrypted, and each peer keeps only its own."}</p>
          <label class="checkbox-row"><input type="checkbox" class="retention-enabled" checked={retention.enabled} onchange={retention_callback(props, Field::Enabled)} />{"Hold messages for offline peers"}</label>
          <label>{"Keep for (minutes)"}<input type="number" class="retention-minutes" min="1" max={MAX_RETENTION_MINUTES.to_string()} value={retention.minutes.to_string()} disabled={!retention.enabled} onchange={retention_callback(props, Field::Minutes)} /></label>
          <label>{"Storage limit (MB, oldest dropped first)"}<input type="number" class="retention-megabytes" min="1" max={MAX_RETENTION_MEGABYTES.to_string()} value={retention.megabytes.to_string()} disabled={!retention.enabled} onchange={retention_callback(props, Field::Megabytes)} /></label>
          <small>{"Turning this off clears held messages. You can still ask other peers for messages you missed."}</small>
        </div>
    }
}

#[derive(Clone, Copy)]
enum Field {
    Enabled,
    Minutes,
    Megabytes,
}

fn retention_callback(props: &SettingsProps, field: Field) -> Callback<Event> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |event: Event| {
        let Some(input) = event.target_dyn_into::<HtmlInputElement>() else {
            return;
        };
        let retention = updated(profile.settings.swarm_retention, field, &input);
        on_update.emit(crate::controllers::account::swarm_retention_patch(
            &profile, retention,
        ));
    })
}

fn updated(
    mut retention: SwarmRetention,
    field: Field,
    input: &HtmlInputElement,
) -> SwarmRetention {
    let number = || input.value().trim().parse::<u32>().unwrap_or(1);
    match field {
        Field::Enabled => retention.enabled = input.checked(),
        Field::Minutes => retention.minutes = number(),
        Field::Megabytes => retention.megabytes = number(),
    }
    retention
}
