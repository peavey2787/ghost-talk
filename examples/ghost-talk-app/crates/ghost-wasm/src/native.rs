#![cfg(target_arch = "wasm32")]

mod browser_host;
mod invoke;
pub(crate) use invoke::js_error;
pub(crate) use invoke::{
    create_wallet, import_wallet, initialize_hydra, is_tauri, load_profile_state,
    save_profile_state, SESSION_RESTORE_BODY,
};
mod transport_restore;
pub(crate) use transport_restore::unlock_profile_runtime;
mod commands;
pub(crate) use commands::{
    broadcast_signer_send, consolidate_kaspa, fetch_verified_media, gather_wallet_history, lock_profile,
    lookup_public_profile, publish_descriptor, resolve_peer, reveal_recovery,
    send_call_contact_request, send_call_control_message, send_call_signal, send_contact_accept,
    prepare_signer_send, send_contact_request, send_kaspa, send_mailbox_message, send_mailbox_message_existing_session,
    send_mailbox_reaction, send_room_contact_request,
};
mod restore;
pub(crate) use restore::{
    leave_peer, leave_peer_session, open_realtime, peer_session_binding, publish_profile_backup,
    receive_mailbox, rejoin_peer, restore_profile_backup, resume_profile_sessions, seal_realtime,
    send_delivery_ack, send_mailbox_control, send_realtime_carrier, send_recovery_offer,
    send_session_end,
};
mod broadcast;
mod events;
mod kasia;
mod media;
pub(crate) use events::{
    browser_wallet_snapshot, clear_debug_log, debug_log_snapshot, hydra_debug_state, listen,
    load_remembered_unlock, set_debug_logging, set_remembered_unlock, start_wallet_monitor,
    update_wallet_monitor_public,
};

pub(crate) use broadcast::{push_broadcast, start_broadcast, stop_broadcast, BroadcastStopResult};
pub(crate) use kasia::{
    kasia_history, received_kasia_handshakes, send_kasia_handshake, send_kasia_message,
};

pub(crate) use media::{archive_plan, archive_publish, import_local_media, sign_manifest};

