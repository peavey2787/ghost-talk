use super::send::{
    gather_wallet_history, open_secret, seal_secret, supported_network, validate_public_projection,
    validate_requested_network,
};
use crate::validation::require_min_password;
pub(crate) use ghost_api::{
    BroadcastResult, WalletCreateResponse, WalletHistoryEntry, WalletHistoryResult,
    WalletImportResponse, WalletProjection, WalletRecovery,
};
pub(crate) use ghost_history::{rest_base_for_network, RestHistory};
pub(crate) use ghost_kaspa::wallet::{CreatedWallet, WalletPublic, WalletSecret};
pub(crate) use serde::Deserialize;
pub(crate) use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
pub(crate) use tauri::State;

#[derive(Default)]
pub struct WalletRuntimeState {
    pub(crate) secrets: Mutex<HashMap<String, WalletSecret>>,
    // Serialize spend planning/broadcast per profile. Portal uses the live UTXO
    // set, so two concurrent Ghost Talk carriers must never plan against the
    // same still-unspent outpoint while the first accepted transaction is
    // waiting to become visible through the node.
    pub(crate) outbound_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub(crate) utxo_events: Mutex<HashMap<String, tokio::sync::watch::Sender<Vec<String>>>>,
}

impl WalletRuntimeState {
    pub(crate) fn insert(&self, profile_id: String, secret: WalletSecret) -> Result<(), String> {
        self.secrets
            .lock()
            .map_err(|_| "wallet runtime state is poisoned".to_string())?
            .insert(profile_id, secret);
        Ok(())
    }

    pub(crate) fn secret_or_open(
        &self,
        profile_id: &str,
        password: &str,
        sealed: &[u8],
        public: &WalletPublic,
    ) -> Result<WalletSecret, String> {
        if let Some(secret) = self
            .secrets
            .lock()
            .map_err(|_| "wallet runtime state is poisoned".to_string())?
            .get(profile_id)
            .cloned()
        {
            validate_public_projection(&secret, public)?;
            return Ok(secret);
        }
        let secret = open_secret(password, sealed)?;
        validate_public_projection(&secret, public)?;
        self.insert(profile_id.to_owned(), secret.clone())?;
        Ok(secret)
    }

    pub(crate) fn hydra_seed_for_profile(&self, profile_id: &str) -> Result<[u8; 32], String> {
        let secrets = self
            .secrets
            .lock()
            .map_err(|_| "wallet runtime state is poisoned".to_string())?;
        let secret = secrets
            .get(profile_id)
            .ok_or_else(|| "wallet must be unlocked before HYDRA can open".to_string())?;
        ghost_kaspa::wallet::hydra_identity_seed(secret)
    }

    pub(crate) fn remove(&self, profile_id: &str) -> Result<(), String> {
        self.secrets
            .lock()
            .map_err(|_| "wallet runtime state is poisoned".to_string())?
            .remove(profile_id);
        // Keep the per-profile outbound mutex allocated for the process lifetime.
        // Dropping/recreating it while an in-flight send still owns the old Arc
        // could allow a concurrent post-lock send to bypass serialization.
        Ok(())
    }

    pub(crate) fn utxo_event_receiver(
        &self,
        profile_id: &str,
    ) -> Result<tokio::sync::watch::Receiver<Vec<String>>, String> {
        let mut events = self
            .utxo_events
            .lock()
            .map_err(|_| "wallet UTXO event state is poisoned".to_string())?;
        let sender = events.entry(profile_id.to_owned()).or_insert_with(|| {
            let (sender, _receiver) = tokio::sync::watch::channel(Vec::<String>::new());
            sender
        });
        Ok(sender.subscribe())
    }

    pub(crate) fn note_utxo_transactions(
        &self,
        profile_id: &str,
        transaction_ids: Vec<String>,
    ) -> Result<(), String> {
        let mut events = self
            .utxo_events
            .lock()
            .map_err(|_| "wallet UTXO event state is poisoned".to_string())?;
        let sender = events.entry(profile_id.to_owned()).or_insert_with(|| {
            let (sender, _receiver) = tokio::sync::watch::channel(Vec::<String>::new());
            sender
        });
        sender.send_replace(transaction_ids);
        Ok(())
    }
    pub(crate) fn outbound_lock(
        &self,
        profile_id: &str,
    ) -> Result<Arc<tokio::sync::Mutex<()>>, String> {
        let mut locks = self
            .outbound_locks
            .lock()
            .map_err(|_| "wallet outbound state is poisoned".to_string())?;
        Ok(locks
            .entry(profile_id.to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone())
    }
}

mod projection;
pub use projection::derivation_presets;
pub(crate) use projection::{broadcast_projection, wallet_projection, WalletNetworkOptions};

#[tauri::command]
pub async fn wallet_create(
    password: String,
    passphrase: String,
    account_path: String,
    network: String,
) -> Result<WalletCreateResponse, String> {
    crate::run_blocking("wallet creation", move || {
        require_min_password(&password, "wallet")?;
        let network = supported_network(&network)?;
        let (secret, CreatedWallet { mnemonic, public }) =
            ghost_kaspa::wallet::generate_wallet(&passphrase, &account_path, &network)?;
        validate_requested_network(&network, &secret, &public)?;
        let sealed = seal_secret(&password, &secret)?;
        Ok(WalletCreateResponse {
            sealed,
            mnemonic,
            public: wallet_projection(&public),
        })
    })
    .await
}

#[tauri::command]
pub async fn wallet_import(
    password: String,
    mnemonic: String,
    passphrase: String,
    account_path: String,
    network: String,
) -> Result<WalletImportResponse, String> {
    crate::run_blocking("wallet restore", move || {
        require_min_password(&password, "wallet")?;
        let network = supported_network(&network)?;
        let (secret, public) =
            ghost_kaspa::wallet::import_wallet(&mnemonic, &passphrase, &account_path, &network)?;
        validate_requested_network(&network, &secret, &public)?;
        let sealed = seal_secret(&password, &secret)?;
        Ok(WalletImportResponse {
            sealed,
            public: wallet_projection(&public),
        })
    })
    .await
}

#[tauri::command]
pub async fn wallet_unlock(
    state: State<'_, WalletRuntimeState>,
    profile_id: String,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<WalletPublic, String> {
    let started = Instant::now();
    let public_for_worker = public.clone();
    let secret = crate::run_blocking("wallet unlock", move || {
        let secret = open_secret(&password, &sealed)?;
        validate_public_projection(&secret, &public_for_worker)?;
        Ok(secret)
    })
    .await?;
    state.insert(profile_id.clone(), secret)?;
    crate::debug_log::record(
        "info",
        "unlock",
        "wallet-unlock-complete",
        format!(
            "profile={} elapsed_ms={}",
            profile_id,
            started.elapsed().as_millis()
        ),
    );
    Ok(public)
}

#[tauri::command]
pub fn wallet_lock(state: State<'_, WalletRuntimeState>, profile_id: String) -> Result<(), String> {
    state.remove(&profile_id)
}

#[tauri::command]
pub async fn wallet_reveal_recovery(
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<WalletRecovery, String> {
    crate::run_blocking("wallet recovery reveal", move || {
        require_min_password(&password, "wallet")?;
        let secret = open_secret(&password, &sealed)?;
        validate_public_projection(&secret, &public)?;
        Ok(WalletRecovery {
            mnemonic: secret.mnemonic.clone(),
            passphrase: secret.passphrase.clone(),
            account_path: secret.account_path.clone(),
            network: secret.network.clone(),
        })
    })
    .await
}

#[tauri::command]
pub fn wallet_next_receive(mut public: WalletPublic) -> Result<WalletPublic, String> {
    public.advance_receive()?;
    Ok(public)
}

#[tauri::command]
pub async fn wallet_gather_history(
    public: WalletPublic,
    options: WalletNetworkOptions,
    priority_addresses: Vec<String>,
) -> Result<WalletHistoryResult, String> {
    gather_wallet_history(
        &public,
        options.rest_endpoint.as_deref(),
        &priority_addresses,
    )
    .await
}
