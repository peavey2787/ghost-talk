mod broadcast;
mod backup;
mod discover;
mod hydra;
mod interop;
mod kaspa;
mod media;
mod metadata;
mod runtime;
mod signer;
mod support;
mod wallet;

use ghost_hydra::HydraFacade;
use ghost_kaspa::wallet::WalletSecret;
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap};

thread_local! {
    static WALLET_SECRETS: RefCell<HashMap<String, WalletSecret>> = RefCell::new(HashMap::new());
    static HYDRA_RUNTIMES: RefCell<HashMap<String, HydraFacade>> = RefCell::new(HashMap::new());
}

pub(crate) async fn invoke(command: &str, args: Value) -> Result<Value, String> {
    if command.starts_with("profile_state_") {
        return support::storage::invoke(command, &args);
    }
    invoke_runtime(command, &args).await
}

async fn invoke_runtime(command: &str, args: &Value) -> Result<Value, String> {
    if command.starts_with("debug_log_") {
        return support::debug::invoke(command, args);
    }
    if command.starts_with("profile_backup_") {
        return backup::invoke(command, args).await;
    }
    if command.starts_with("broadcast_") {
        return broadcast::invoke(command, args);
    }
    if command == "publish_ghost_descriptor" {
        return discover::invoke(command, args).await;
    }
    invoke_social(command, args).await
}

async fn invoke_social(command: &str, args: &Value) -> Result<Value, String> {
    if matches!(command, "resolve_ghost_peer" | "lookup_ghost_profile") {
        return runtime::peer::invoke(command, args);
    }
    if command == "hydra_receive_mailbox" {
        return runtime::mailbox::invoke(command, args).await;
    }
    if command.starts_with("mailbox_send_") {
        return runtime::mailbox_send::invoke(command, args).await;
    }
    if command.starts_with("media_") || command.starts_with("kaspa_archive_") {
        return media::invoke(command, args).await;
    }
    if command.starts_with("kasia_") {
        return interop::kasia::invoke(command, args).await;
    }
    invoke_subsystem(command, args).await
}

async fn invoke_subsystem(command: &str, args: &Value) -> Result<Value, String> {
    if let Some(result) = invoke_local_subsystem(command, args) {
        return result;
    }
    invoke_wallet_subsystem(command, args).await
}


fn invoke_local_subsystem(command: &str, args: &Value) -> Option<Result<Value, String>> {
    if command.starts_with("kaskold_") {
        return Some(interop::kaskold::invoke(command, args));
    }
    if matches!(command, "app_info" | "derivation_presets") {
        return Some(metadata::invoke(command));
    }
    if command.starts_with("remembered_unlock_") {
        return Some(support::credentials::invoke(command, args));
    }
    None
}

async fn invoke_wallet_subsystem(command: &str, args: &Value) -> Result<Value, String> {
    if command.starts_with("wallet_monitor_") {
        return kaspa::invoke_monitor(command, args).await;
    }
    if matches!(command, "wallet_prepare_signer_send" | "wallet_broadcast_signer_send") {
        return signer::invoke(command, args).await;
    }
    if command.starts_with("wallet_") {
        return wallet::invoke(command, args).await;
    }
    if command.starts_with("hydra_") {
        return hydra::invoke(command, args).await;
    }
    Err(format!("unknown standalone Web command: {command}"))
}

#[cfg(test)]
mod tests;
