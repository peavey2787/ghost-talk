use web_sys::HtmlTextAreaElement;
use yew::prelude::*;

use super::settings::SettingsProps;

/// Operator p2p-net infrastructure for the Auto realtime route.
pub(super) fn render_p2p_settings(props: &SettingsProps) -> Html {
    let settings = &props.profile.settings;
    html! {
        <div class="card form-grid p2p-settings-card"><h3>{"Direct transport (p2p-net)"}</h3>
          <p>{"With Auto, live voice moves to p2p-net after both sides exchange signed transport announcements over Kaspa; Kaspa stays the fallback. Browsers reach each other through p2p-net relays."}</p>
          <label>{"Relay peers (one multiaddr per line)"}<textarea class="p2p-relay-peers" value={settings.p2p_relay_peers.join("\n")} onchange={list_callback(props, "p2pRelayPeers")} placeholder="/dns4/relay.example/tcp/443/wss/p2p/12D3KooW…" /></label>
          <label>{"Bootstrap peers (one multiaddr per line)"}<textarea class="p2p-bootstrap-peers" value={settings.p2p_bootstrap_peers.join("\n")} onchange={list_callback(props, "p2pBootstrapPeers")} placeholder="p2p-net public bootstrap" /></label>
          <small>{"Leave both empty to use p2p-net's public bootstrap policy."}</small>
        </div>
    }
}

fn list_callback(props: &SettingsProps, field: &'static str) -> Callback<Event> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |event: Event| {
        let Some(area) = event.target_dyn_into::<HtmlTextAreaElement>() else {
            return;
        };
        if let Ok(patch) =
            crate::controllers::account::update_setting(&profile, field, area.value())
        {
            on_update.emit(patch);
        }
    })
}
