use super::{decimal_cmp, live_profile_by_id, native, spawn_local, AppRuntime, AppState};
use crate::app::shell::persist_profiles;
use crate::model::{
    Chat, ChatStore, DirectoryLiveEvent, NetworkStatusEvent, Profile, PublicGhostProfile,
    WalletRecord,
};

pub(crate) fn apply_directory_event(
    wallet: &mut Option<WalletRecord>,
    directory: &mut Vec<PublicGhostProfile>,
    event: DirectoryLiveEvent,
) {
    crate::model::WalletStateService::set_directory_checkpoint(wallet, event.directory_checkpoint);
    for incoming in event.public_profiles {
        upsert_public_profile(directory, incoming);
    }
    directory.retain(|item| item.discoverable);
    directory
        .sort_by(|a, b| decimal_cmp(&b.descriptor_blue_score, &a.descriptor_blue_score).cmp(&0));
    directory.truncate(100);
}

pub(crate) fn upsert_public_profile(
    directory: &mut Vec<PublicGhostProfile>,
    incoming: PublicGhostProfile,
) {
    if let Some(existing) = directory
        .iter_mut()
        .find(|item| item.kaspa_address == incoming.kaspa_address)
    {
        if decimal_cmp(
            &incoming.descriptor_blue_score,
            &existing.descriptor_blue_score,
        ) >= 0
        {
            *existing = incoming;
        }
    } else if incoming.discoverable {
        directory.push(incoming);
    }
}

pub(crate) fn launch_wallet_monitor(state: AppState, live: AppRuntime, profile: Profile) {
    state.network_status.set("connecting".into());
    state.reconnect_attempts.set(0);
    spawn_local(async move {
        if native::is_tauri() {
            if let Err(error) = native::start_wallet_monitor(&profile).await {
                state.app_status.set(format!("Kaspa connection: {error}"));
            }
            return;
        }
        browser_wallet_monitor_loop(state, live, profile.id).await;
    });
}

async fn browser_wallet_monitor_loop(state: AppState, live: AppRuntime, profile_id: String) {
    let mut failures = 0u32;
    loop {
        if live.active_id.borrow().as_deref() != Some(profile_id.as_str()) {
            return;
        }
        let Some(profile) = live_profile_by_id(&live, &profile_id) else {
            return;
        };
        match native::start_wallet_monitor(&profile).await {
            Ok(()) => {
                browser_monitor_connected(&state, &mut failures);
                refresh_browser_wallet_snapshot(&state, &profile).await;
            }
            Err(error) => browser_monitor_failed(&state, &mut failures, error),
        }
        let delay_ms = browser_monitor_delay_ms(failures);
        gloo_timers::future::TimeoutFuture::new(delay_ms).await;
    }
}

async fn refresh_browser_wallet_snapshot(state: &AppState, profile: &Profile) {
    match native::browser_wallet_snapshot(profile).await {
        Ok(snapshot) => state.snapshot.set(Some(snapshot)),
        Err(error) => web_sys::console::warn_1(
            &format!("Ghost Talk Web wallet snapshot refresh failed: {error}").into(),
        ),
    }
}

fn browser_monitor_connected(state: &AppState, failures: &mut u32) {
    *failures = 0;
    state.network_status.set("connected".into());
    state.reconnect_attempts.set(0);
    if state.app_status.starts_with("Kaspa Web connection:") {
        state.app_status.set(String::new());
    }
}

fn browser_monitor_failed(state: &AppState, failures: &mut u32, error: String) {
    *failures = failures.saturating_add(1);
    let status = if *failures == 1 {
        "connecting"
    } else {
        "reconnecting"
    };
    state.network_status.set(status.into());
    state.reconnect_attempts.set(*failures);
    state
        .app_status
        .set(format!("Kaspa Web connection: {error}"));
}

fn browser_monitor_delay_ms(failures: u32) -> u32 {
    if failures == 0 {
        return 10_000;
    }
    let exponent = failures.saturating_sub(1).min(5);
    (1_000u32 << exponent).min(30_000)
}

pub(crate) fn register_network_listener(state: AppState, live: AppRuntime) {
    native::listen::<NetworkStatusEvent, _>("ghost://network-status", move |event| {
        handle_network_status(event, state.clone(), live.clone());
    });
}

pub(crate) fn handle_network_status(event: NetworkStatusEvent, state: AppState, live: AppRuntime) {
    if live.active_id.borrow().as_deref() == Some(event.profile_id.as_str()) {
        state.network_status.set(event.status.clone());
        state.reconnect_attempts.set(event.reconnect_attempts);
    }
    if event.status != "connected" {
        return;
    }
    let profile_id = event.profile_id.clone();
    let Some(profile) = live_profile_by_id(&live, &profile_id) else {
        return;
    };
    if !profile_needs_transport_restore(&profile)
        || !live.resume_busy.borrow_mut().insert(profile_id.clone())
    {
        return;
    }
    spawn_local(resume_after_network(profile_id, profile, state, live));
}

pub(crate) fn profile_needs_transport_restore(profile: &Profile) -> bool {
    profile.chats.iter().any(|chat| {
        !chat.left()
            && !chat.peer_left()
            && chat.transport_restore_pending()
            && chat.session_role() == Some("initiator")
    })
}

pub(crate) async fn resume_after_network(
    profile_id: String,
    mut attempt_profile: Profile,
    state: AppState,
    live: AppRuntime,
) {
    for attempt in 0..3 {
        let password = live.password.borrow().clone();
        let (resumed, had_failure) =
            native::resume_profile_sessions(attempt_profile, &password).await;
        merge_resume_result(&profile_id, &resumed, &state, &live);
        if !had_failure || attempt == 2 {
            break;
        }
        gloo_timers::future::TimeoutFuture::new(3_000).await;
        let Some(refreshed) = live_profile_by_id(&live, &profile_id) else {
            break;
        };
        attempt_profile = refreshed;
    }
    live.resume_busy.borrow_mut().remove(&profile_id);
}

pub(crate) fn merge_resume_result(
    profile_id: &str,
    resumed: &Profile,
    state: &AppState,
    live: &AppRuntime,
) {
    let mut latest = live.profiles.borrow().clone();
    if let Some(slot) = latest
        .iter_mut()
        .find(|candidate| candidate.id == profile_id)
    {
        merge_resume_wallet(&mut slot.wallet, resumed);
        merge_resume_chats(&mut slot.chats, resumed);
    }
    persist_profiles(state, live, latest);
}

pub(crate) fn merge_resume_wallet(wallet: &mut Option<WalletRecord>, resumed: &Profile) {
    let Some(src) = resumed.wallet.as_ref() else {
        return;
    };
    crate::model::WalletStateService::merge_progress(wallet, src.public.clone());
}

pub(crate) fn merge_resume_chats(chats: &mut ChatStore, resumed: &Profile) {
    for resumed_chat in &resumed.chats {
        let Some(index) = chats.position(|chat| resumable_chat_matches(chat, resumed_chat)) else {
            continue;
        };
        let chat_id = chats[index].id.clone();
        merge_resume_chat(chats, &chat_id, resumed_chat);
    }
}

pub(crate) fn resumable_chat_matches(chat: &Chat, resumed: &Chat) -> bool {
    chat.id == resumed.id
        && !chat.left()
        && !chat.peer_left()
        && chat.transport_restore_pending()
        && chat.peer_hydra_handle() == resumed.peer_hydra_handle()
}

pub(crate) fn merge_resume_chat(chats: &mut ghost_chat::ChatStore, chat_id: &str, resumed: &Chat) {
    let _ = ghost_chat::HydraSessionManager::merge_resumed(chats, chat_id, resumed);
}
