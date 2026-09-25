use crate::model::Profile;
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
}

#[component(Sidebar)]
pub fn sidebar(props: &SidebarProps) -> Html {
    let incoming = pending_direct_count(&props.profile, &props.tab, &props.selected_chat_id);
    let room_invites = pending_room_count(&props.profile);
    let network_title = network_title(&props.network_status, props.reconnect_attempts);
    html! {
      <aside class="sidebar">
        <div class="brand-row">
          <img class="brand-mark small" src={crate::ui_assets::ghost_talk_logo_src()} alt="" />
          <h1>{"Ghost Talk"}</h1>
          <span class={classes!("network-dot", props.network_status.clone())} title={network_title}></span>
        </div>
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

fn network_title(status: &str, reconnect_attempts: u32) -> String {
    if status == "reconnecting" {
        format!("Kaspa reconnecting · attempt {reconnect_attempts}")
    } else {
        format!("Kaspa {status}")
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
          <span class="avatar">{props.profile.label.chars().next().unwrap_or('G').to_ascii_uppercase()}</span>
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
