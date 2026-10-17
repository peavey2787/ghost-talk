use crate::components::call::SwarmStatus;
use crate::model::{Profile, ProfilePatch};
use ghost_domain::settings::SpendMode;
use yew::prelude::*;

pub const TABS: [&str; 8] = [
    "Chats", "Contacts", "Discover", "Rooms", "Studio", "Games", "Kaspa", "Settings",
];

#[derive(Properties, PartialEq)]
pub struct SidebarProps {
    pub profile: Profile,
    pub tab: String,
    pub selected_chat_id: String,
    pub network_status: String,
    pub reconnect_attempts: u32,
    pub on_tab: Callback<String>,
    pub on_switch_user: Callback<()>,
    pub on_update: Callback<ProfilePatch>,
}

#[component(Sidebar)]
pub fn sidebar(props: &SidebarProps) -> Html {
    let incoming = pending_direct_count(&props.profile, &props.tab, &props.selected_chat_id);
    let room_invites = pending_room_count(&props.profile);
    let network_title = network_title(&props.network_status, props.reconnect_attempts);
    let swarm = use_context::<SwarmStatus>().unwrap_or_default();
    html! {
      <aside class="sidebar">
        <div class="brand-row">
          <img class="brand-mark small" src={crate::ui_assets::ghost_talk_logo_src()} alt="" />
          <h1>{"Ghost Talk"}</h1>
          <span class="status-icons">
            {kaspa_status_icon(&props.network_status, network_title)}
            {swarm_status_icon(swarm)}
          </span>
        </div>
        {spend_mode_switch(props)}
        <input class="search" placeholder="Search name, KNS, dot.k or kaspa:…" />
        <nav>{for TABS.iter().map(|name| nav_button(props, name, incoming, room_invites))}</nav>
        {profile_switch_button(props)}
      </aside>
    }
}

fn pending_direct_count(profile: &Profile, tab: &str, selected_chat_id: &str) -> usize {
    profile
        .chats
        .iter()
        .filter(|chat| !chat.room_transport_only() && !chat.left() && !chat.archived())
        .map(|chat| {
            let visible_here = tab == "Chats" && chat.id == selected_chat_id;
            let unread = if visible_here {
                0
            } else {
                chat.unread_count() as usize
            };
            let pending_request = if chat
                .incoming_request()
                .is_some_and(|request| request.state == "pending")
            {
                1
            } else {
                0
            };
            unread + pending_request
        })
        .sum()
}

fn pending_room_count(profile: &Profile) -> usize {
    let chat_invites = profile
        .chats
        .iter()
        .filter(|chat| {
            chat.room_transport_only()
                && chat.incoming_request().is_some_and(|request| {
                    request.state == "pending" && request.room_invite.is_some()
                })
        })
        .count();
    chat_invites
        + profile
            .rooms
            .iter()
            .filter(|room| room.pending_acceptance())
            .count()
}

/// Kaspa (fee-paying transactions, stored on chain) or No-spend (the swarm
/// only, free, nothing stored on chain): always visible, one click to switch.
fn spend_mode_switch(props: &SidebarProps) -> Html {
    let current = props.profile.settings.spend_mode;
    html! {
      <div class="spend-mode-switch" role="radiogroup" aria-label="Messaging mode">
        {for SpendMode::ALL.into_iter().map(|mode| spend_mode_option(props, mode, current))}
      </div>
    }
}

fn spend_mode_option(props: &SidebarProps, mode: SpendMode, current: SpendMode) -> Html {
    let selected = mode == current;
    let (profile, on_update) = (props.profile.clone(), props.on_update.clone());
    let onclick = Callback::from(move |_| {
        if !selected {
            on_update.emit(crate::controllers::account::spend_mode_patch(
                &profile, mode,
            ));
        }
    });
    html! {
      <button class={classes!("spend-mode-option", selected.then_some("selected"))} role="radio" aria-checked={selected.to_string()} title={spend_mode_hint(mode)} {onclick}>
        {mode.label()}
      </button>
    }
}

fn spend_mode_hint(mode: SpendMode) -> &'static str {
    match mode {
        SpendMode::Kaspa => "Kaspa mode: messages are Kaspa transactions — small fee, stored on chain, reach peers even when they are offline.",
        SpendMode::NoSpend => "No-spend mode: messages travel over the swarm only — no funds needed, nothing stored on Kaspa, peers must be online.",
    }
}

fn network_title(status: &str, reconnect_attempts: u32) -> String {
    if status == "reconnecting" {
        format!("Kaspa reconnecting · attempt {reconnect_attempts}")
    } else {
        format!("Kaspa {status}")
    }
}

/// Kaspa mark (mirrored K in a disc): teal connected, grey reconnecting, black offline.
fn kaspa_status_icon(status: &str, title: String) -> Html {
    html! {
      <svg class={classes!("status-icon", "kaspa-status", status.to_owned())} viewBox="0 0 24 24" role="img" aria-label={title.clone()}>
        <title>{title}</title>
        <circle class="status-disc" cx="12" cy="12" r="11"/>
        <path class="status-glyph" d="M15.5 5.5v13M15.5 12L8 6M15.5 12L8 18"/>
      </svg>
    }
}

/// p2p-net swarm glyph (three linked nodes) with the same colour states.
fn swarm_status_icon(status: SwarmStatus) -> Html {
    let title = status.title();
    html! {
      <svg class={classes!("status-icon", "swarm-status", status.indicator_class())} viewBox="0 0 24 24" role="img" aria-label={title.clone()}>
        <title>{title}</title>
        <circle class="status-disc" cx="12" cy="12" r="11"/>
        <path class="status-glyph" d="M12 7L7.5 15.5H16.5Z"/>
        <circle class="status-node" cx="12" cy="7" r="2"/>
        <circle class="status-node" cx="7.5" cy="15.5" r="2"/>
        <circle class="status-node" cx="16.5" cy="15.5" r="2"/>
      </svg>
    }
}

fn nav_button(props: &SidebarProps, name: &&str, incoming: usize, room_invites: usize) -> Html {
    let tab = (*name).to_string();
    let on_tab = props.on_tab.clone();
    html! {
      <button class={(*name == props.tab).then_some("active")} onclick={Callback::from(move |_| on_tab.emit(tab.clone()))}>
        <span>{tab_icon(name)}</span><span class="nav-label">{*name}</span>
        {nav_badge(name, incoming, room_invites)}
      </button>
    }
}

fn nav_badge(name: &str, incoming: usize, room_invites: usize) -> Html {
    if name == "Chats" && incoming > 0 {
        html! { <span class="nav-badge">{incoming}</span> }
    } else if name == "Rooms" && room_invites > 0 {
        html! { <span class="nav-badge">{room_invites}</span> }
    } else {
        Html::default()
    }
}

fn profile_switch_button(props: &SidebarProps) -> Html {
    let callback = props.on_switch_user.clone();
    let network = props
        .profile
        .wallet
        .as_ref()
        .map(|wallet| wallet.public.network.clone())
        .unwrap_or_else(|| "No Kaspa wallet".into());
    html! {
        <button class="profile-chip" onclick={Callback::from(move |_| callback.emit(()))}>
          <crate::components::avatar::Avatar label={props.profile.label.clone()} reference={props.profile.public_avatar().cloned()} size={36} />
          <span><b>{props.profile.label.clone()}</b><small>{network}</small></span>
          <span>{"⇄"}</span>
        </button>
    }
}

fn tab_icon(tab: &str) -> &'static str {
    const ICONS: &[(&str, &str)] = &[
        ("Chats", "◉"),
        ("Contacts", "◎"),
        ("Discover", "✦"),
        ("Rooms", "▦"),
        ("Studio", "◍"),
        ("Games", "◇"),
        ("Kaspa", "K"),
        ("Settings", "⚙"),
    ];
    ICONS
        .iter()
        .find_map(|(name, icon)| (*name == tab).then_some(*icon))
        .unwrap_or("•")
}
