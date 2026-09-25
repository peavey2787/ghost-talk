use crate::components::{
    call::LiveCallManager, chat::ChatView, contacts::ContactsView, debug::DebugLogWindow,
    discover::DiscoverView, rooms::RoomsView, settings::SettingsView, sidebar::Sidebar,
    simple::GamesView, studio::StudioView, wallet::KaspaWallet,
};
use crate::model::{Profile, ProfilePatch};
use yew::prelude::*;

use super::super::profile_runtime::call_profile_update_callback;
use super::super::state::{AppRuntime, AppState};
use super::callbacks::{
    app_error_callback, call_chat_select_callback, call_event_handled_callback,
    chat_select_callback, clear_contact_prefill_callback, close_debug_callback,
    open_debug_callback, realtime_handled_callback, room_add_contact_callback, snapshot_callback,
    tab_callback, wallet_public_callback,
};

pub(crate) fn app_shell(
    state: &AppState,
    live: &AppRuntime,
    active: Profile,
    update_profile: Callback<ProfilePatch>,
    switch_user: Callback<()>,
    start_chat: Callback<String>,
) -> Html {
    let content = app_tab_content(state, live, &active, update_profile.clone(), start_chat);
    let on_select = call_chat_select_callback(state.clone(), live.clone());
    let on_control_handled = realtime_handled_callback(state.clone(), live.clone());
    let on_call_event_handled = call_event_handled_callback(state.clone(), live.clone());
    let call_update = call_profile_update_callback(state.clone(), live.clone());
    let on_error = app_error_callback(state.app_status.clone());
    html! {
      <LiveCallManager
        profile={active.clone()}
        password={(*state.session_password).clone()}
        realtime_controls={(*state.realtime_controls).clone()}
        {on_control_handled}
        call_events={(*state.call_events).clone()}
        {on_call_event_handled}
        on_select={on_select}
        on_update={call_update}
        {on_error}
      >
        <main class="app">
          <Sidebar profile={active.clone()} tab={(*state.tab).clone()} selected_chat_id={(*state.selected_chat_id).clone()} network_status={(*state.network_status).clone()} reconnect_attempts={*state.reconnect_attempts} on_tab={tab_callback(state.clone(), live.clone())} on_switch_user={switch_user}/>
          <section class="workspace">{app_status_view(state.app_status.clone())}{content}</section>
          <DebugLogWindow profile_id={active.id} open={*state.debug_open} on_close={close_debug_callback(state.debug_open.clone())}/>
        </main>
      </LiveCallManager>
    }
}

pub(crate) fn app_tab_content(
    state: &AppState,
    live: &AppRuntime,
    active: &Profile,
    update_profile: Callback<ProfilePatch>,
    start_chat: Callback<String>,
) -> Html {
    match state.tab.as_str() {
        "Chats" => {
            html! { <ChatView profile={active.clone()} password={(*state.session_password).clone()} selected_chat_id={(*state.selected_chat_id).clone()} on_select={chat_select_callback(state.clone(), live.clone())} on_update={update_profile} on_wallet_public={wallet_public_callback(state.clone(), live.clone())} on_start_chat={start_chat}/> }
        }
        "Contacts" => {
            html! { <ContactsView profile={active.clone()} password={(*state.session_password).clone()} prefill_target={(*state.contact_prefill).clone()} on_prefill_consumed={clear_contact_prefill_callback(state.contact_prefill.clone())} on_update={update_profile} on_open_chat={start_chat}/> }
        }
        "Discover" => {
            html! { <DiscoverView profile={active.clone()} password={(*state.session_password).clone()} on_update={update_profile} on_start_chat={start_chat}/> }
        }
        "Rooms" => {
            html! { <RoomsView profile={active.clone()} password={(*state.session_password).clone()} on_update={update_profile} on_add_contact={room_add_contact_callback(state.clone(), live.clone())}/> }
        }
        tab => secondary_tab_content(tab, state, active, update_profile),
    }
}

fn secondary_tab_content(
    tab: &str,
    state: &AppState,
    active: &Profile,
    update_profile: Callback<ProfilePatch>,
) -> Html {
    match tab {
        "Studio" => {
            html! { <StudioView profile={active.clone()} password={(*state.session_password).clone()} on_update={update_profile}/> }
        }
        "Games" => html! { <GamesView/> },
        "Kaspa" => {
            html! { <KaspaWallet profile={active.clone()} password={(*state.session_password).clone()} snapshot={(*state.snapshot).clone()} on_update={update_profile} on_snapshot={snapshot_callback(state.snapshot.clone())}/> }
        }
        "Settings" => {
            html! { <SettingsView profile={active.clone()} password={(*state.session_password).clone()} on_update={update_profile} on_open_debug={open_debug_callback(state.debug_open.clone())}/> }
        }
        _ => Html::default(),
    }
}

pub(crate) fn app_status_view(status: UseStateHandle<String>) -> Html {
    if status.is_empty() {
        return Html::default();
    }
    let clear = status.clone();
    html! { <div class="incoming-notice"><span>{(*status).clone()}</span><button onclick={Callback::from(move |_| clear.set(String::new()))}>{"×"}</button></div> }
}
