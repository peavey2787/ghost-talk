#![forbid(unsafe_code)]

mod backup_commands;
mod debug_log;
mod hydra_commands;
mod kaspa_gateway;
mod mailbox_commands;
mod kassigner_commands;
mod peer_commands;
mod profile_state_commands;
mod remembered_unlock_commands;
mod wallet_commands;

use backup_commands::{profile_backup_publish, profile_backup_restore};
use debug_log::{debug_log_clear, debug_log_record, debug_log_set_enabled, debug_log_snapshot};
use hydra_commands::{
    hydra_debug_state, hydra_ensure, hydra_initialize_from_wallet, hydra_leave_peer, hydra_lock_profile,
    hydra_open_direct, hydra_preview_contact_request, hydra_receive_mailbox, hydra_register_peer_routes,
    hydra_seal_direct,
    hydra_rejoin_peer, HydraRuntimeState,
};
use kaspa_gateway::KaspaGatewayState;
use kassigner_commands::{
    kassigner_begin_identity_proof, kassigner_cancel, kassigner_cancel_identity_proof,
    kassigner_complete, kassigner_complete_identity_proof, kassigner_import_kpub,
    kassigner_prepare_consolidation, kassigner_prepare_send, kassigner_scan_account_qr,
    kassigner_scan_identity_proof_qr, kassigner_scan_response_frame, KasSignerRuntimeState,
};
use mailbox_commands::{
    mailbox_retry_handshake_finish, mailbox_send_contact_accept, mailbox_send_contact_request,
    mailbox_send_control, mailbox_send_delivery_ack, mailbox_send_message,
    mailbox_send_recovery_offer, mailbox_send_session_end, mailbox_sync,
    wallet_monitor_start, wallet_monitor_stop, wallet_monitor_update_public, MonitorState,
};
use peer_commands::{lookup_ghost_profile, publish_ghost_descriptor, resolve_ghost_peer, PublicDirectoryState};
use profile_state_commands::{profile_state_load, profile_state_save};
use remembered_unlock_commands::{remembered_unlock_load, remembered_unlock_set};
use serde::Serialize;

pub(crate) async fn run_blocking<T, F>(label: &'static str, task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| format!("{label} worker failed: {error}"))?
}

#[derive(Serialize)]
struct QrSvg {
    svg: String,
}

pub(crate) fn render_qr_svg_bytes(data: &[u8]) -> Result<String, String> {
    use std::fmt::Write;
    let code = qrcode::QrCode::new(data)
        .map_err(|error| format!("QR failed: {error:?}"))?;
    let modules = code.to_colors();
    let size = code.width();
    let border = 4usize;
    let total = size + border * 2;
    let mut svg = String::with_capacity(total.saturating_mul(total).saturating_mul(48));
    write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {total} {total}\" shape-rendering=\"crispEdges\"><rect width=\"{total}\" height=\"{total}\" fill=\"white\"/>"
    )
    .map_err(|_| "QR SVG formatting failed".to_string())?;
    for (index, color) in modules.iter().enumerate() {
        if *color == qrcode::types::Color::Dark {
            let x = index % size + border;
            let y = index / size + border;
            write!(svg, "<rect x=\"{x}\" y=\"{y}\" width=\"1\" height=\"1\" fill=\"black\"/>")
                .map_err(|_| "QR SVG formatting failed".to_string())?;
        }
    }
    svg.push_str("</svg>");
    Ok(svg)
}

#[tauri::command]
fn qr_svg(text: String) -> Result<QrSvg, String> {
    Ok(QrSvg { svg: render_qr_svg_bytes(text.as_bytes())? })
}

#[tauri::command]
fn qr_svg_hex(payload_hex: String) -> Result<QrSvg, String> {
    let bytes = hex::decode(payload_hex.trim())
        .map_err(|_| "QR payload must be valid hexadecimal bytes".to_string())?;
    Ok(QrSvg { svg: render_qr_svg_bytes(&bytes)? })
}

use wallet_commands::{
    derivation_presets, wallet_consolidate, wallet_create, wallet_import, wallet_lock,
    wallet_next_receive, wallet_refresh, wallet_reveal_recovery, wallet_send, wallet_unlock,
    WalletRuntimeState,
};

#[derive(Serialize)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    max_kaspa_payload: usize,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: ghost_core::APP_NAME,
        version: ghost_core::APP_VERSION,
        max_kaspa_payload: ghost_core::MAX_GHOST_TX_PAYLOAD,
    }
}


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(MonitorState::default())
        .manage(KaspaGatewayState::default())
        .manage(PublicDirectoryState::default())
        .manage(HydraRuntimeState::default())
        .manage(WalletRuntimeState::default())
        .manage(KasSignerRuntimeState::default())
        .invoke_handler(tauri::generate_handler![
            app_info,
            debug_log_set_enabled,
            debug_log_record,
            debug_log_snapshot,
            debug_log_clear,
            qr_svg,
            qr_svg_hex,
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
            hydra_seal_direct,
            hydra_open_direct,
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
            wallet_next_receive,
            wallet_refresh,
            wallet_send,
            wallet_consolidate,
            kassigner_import_kpub,
            kassigner_scan_account_qr,
            kassigner_begin_identity_proof,
            kassigner_scan_identity_proof_qr,
            kassigner_complete_identity_proof,
            kassigner_cancel_identity_proof,
            kassigner_prepare_send,
            kassigner_prepare_consolidation,
            kassigner_scan_response_frame,
            kassigner_complete,
            kassigner_cancel,
            mailbox_send_message,
            mailbox_send_contact_request,
            mailbox_send_contact_accept,
            mailbox_send_control,
            mailbox_retry_handshake_finish,
            mailbox_send_recovery_offer,
            mailbox_send_session_end,
            mailbox_send_delivery_ack,
            mailbox_sync,
            wallet_monitor_start,
            wallet_monitor_update_public,
            wallet_monitor_stop
        ])
        .run(tauri::generate_context!())
        .expect("Ghost Talk runtime failed")
}
