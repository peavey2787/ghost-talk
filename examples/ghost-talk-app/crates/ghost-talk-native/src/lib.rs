#![forbid(unsafe_code)]

mod broadcast_commands;
mod diagnostics;
mod media_commands;
mod persistence;
pub(crate) use diagnostics::debug_log;
pub(crate) use persistence::{
    backup_commands, backup_validation, profile_merge, profile_state_commands,
    remembered_unlock_commands, storage_root,
};
mod hydra_commands;
mod interop;
mod kaskold_compat;
mod kaspa_gateway;
mod kaspa_wallet_events;
mod mailbox_commands;
mod peer_commands;
mod time;
mod validation;
mod wallet_commands;

use backup_commands::{profile_backup_publish, profile_backup_restore};
use broadcast_commands::{broadcast_push, broadcast_start, broadcast_stop, BroadcastRuntimeState};
use debug_log::{debug_log_clear, debug_log_record, debug_log_set_enabled, debug_log_snapshot};
use hydra_commands::{
    hydra_debug_state, hydra_ensure, hydra_initialize_from_wallet, hydra_leave_peer,
    hydra_lock_profile, hydra_open_realtime, hydra_peer_session_binding,
    hydra_preview_contact_request, hydra_receive_mailbox, hydra_register_peer_routes,
    hydra_rejoin_peer, hydra_seal_realtime, HydraRuntimeState,
};
use interop::{
    kasia_history, kasia_identity, kasia_received_handshakes, kasia_send_handshake,
    kasia_send_message,
};
use kaskold_compat::{
    kaskold_backup, kaskold_import_bytes, kaskold_import_text, kaskold_review_pskt,
    kaskold_sign_pskt,
};
use kaspa_gateway::KaspaGatewayState;
use mailbox_commands::{
    mailbox_retry_handshake_finish, mailbox_send_call_signal, mailbox_send_contact_accept,
    mailbox_send_contact_request, mailbox_send_control, mailbox_send_delivery_ack,
    mailbox_send_message, mailbox_send_realtime_carrier, mailbox_send_recovery_offer, mailbox_send_session_end,
    wallet_monitor_start, wallet_monitor_stop, wallet_monitor_update_public, MonitorState,
};
use media_commands::{
    kaspa_archive_plan, kaspa_archive_publish, media_fetch_verified, media_import_local,
    media_manifest_sign,
};
use peer_commands::{
    lookup_ghost_profile, publish_ghost_descriptor, resolve_ghost_peer, NameResolverState,
    PublicDirectoryState,
};
use profile_state_commands::{profile_state_load, profile_state_save};
use remembered_unlock_commands::{remembered_unlock_load, remembered_unlock_set};
use tauri::Manager;

#[derive(Clone)]
pub(crate) struct NativeAppState {
    handle: tauri::AppHandle,
}

impl NativeAppState {
    pub(crate) fn handle(&self) -> tauri::AppHandle {
        self.handle.clone()
    }
}

pub(crate) async fn run_blocking<T, F>(label: &'static str, task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| format!("{label} worker failed: {error}"))?
}

use wallet_commands::{
    derivation_presets, wallet_consolidate, wallet_create, wallet_gather_history, wallet_import,
    wallet_broadcast_signer_send, wallet_lock, wallet_next_receive, wallet_prepare_signer_send,
    wallet_reveal_recovery, wallet_send, wallet_unlock,
    WalletRuntimeState,
};

#[tauri::command]
fn app_info() -> ghost_api::AppInfo {
    ghost_api::app_info()
}

macro_rules! native_handler {
    () => {
        tauri::generate_handler![
            app_info,
            kaspa_archive_plan,
            kaspa_archive_publish,
            media_manifest_sign,
            media_import_local,
            media_fetch_verified,
            kasia_identity,
            kasia_send_handshake,
            kasia_send_message,
            kasia_history,
            kasia_received_handshakes,
            broadcast_start,
            broadcast_push,
            broadcast_stop,
            debug_log_set_enabled,
            debug_log_record,
            debug_log_snapshot,
            debug_log_clear,
            profile_backup_publish,
            profile_backup_restore,
            profile_state_load,
            profile_state_save,
            remembered_unlock_load,
            remembered_unlock_set,
            hydra_initialize_from_wallet,
            hydra_debug_state,
            hydra_ensure,
            hydra_preview_contact_request,
            hydra_receive_mailbox,
            hydra_seal_realtime,
            hydra_open_realtime,
            hydra_peer_session_binding,
            hydra_register_peer_routes,
            hydra_leave_peer,
            hydra_rejoin_peer,
            hydra_lock_profile,
            resolve_ghost_peer,
            lookup_ghost_profile,
            publish_ghost_descriptor,
            derivation_presets,
            wallet_create,
            wallet_import,
            wallet_unlock,
            wallet_lock,
            wallet_reveal_recovery,
            wallet_gather_history,
            wallet_next_receive,
            wallet_send,
            wallet_prepare_signer_send,
            wallet_broadcast_signer_send,
            wallet_consolidate,
            kaskold_import_text,
            kaskold_import_bytes,
            kaskold_backup,
            kaskold_review_pskt,
            kaskold_sign_pskt,
            mailbox_send_message,
            mailbox_send_realtime_carrier,
            mailbox_send_contact_request,
            mailbox_send_call_signal,
            mailbox_send_contact_accept,
            mailbox_send_control,
            mailbox_retry_handshake_finish,
            mailbox_send_recovery_offer,
            mailbox_send_session_end,
            mailbox_send_delivery_ack,
            wallet_monitor_start,
            wallet_monitor_update_public,
            wallet_monitor_stop
        ]
    };
}

// The checked-in fallback page keeps Tauri context validation deterministic before
// the production WASM bundle is generated; release packaging replaces that page.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            app.manage(NativeAppState {
                handle: app.handle().clone(),
            });
            Ok(())
        })
        .manage(BroadcastRuntimeState::default())
        .manage(MonitorState::default())
        .manage(KaspaGatewayState::default())
        .manage(PublicDirectoryState::default())
        .manage(NameResolverState::default())
        .manage(HydraRuntimeState::default())
        .manage(WalletRuntimeState::default())
        .invoke_handler(native_handler!())
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("Ghost Talk runtime failed: {error}");
        std::process::exit(1);
    }
}
