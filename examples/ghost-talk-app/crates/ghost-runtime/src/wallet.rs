use std::collections::BTreeMap;

use ghost_api::{MailboxEvent, WalletHistoryEntry, WalletSnapshot};
use ghost_domain::wallet::WalletProjection;
use serde::{Deserialize, Serialize};

use crate::MailboxService;
mod merge;
use merge::{append_unique, merge_collections};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PendingFrameBucket {
    pub count: usize,
    #[serde(default)]
    pub parts: BTreeMap<String, String>,
    #[serde(default)]
    pub txids: BTreeMap<String, String>,
    #[serde(default, rename = "blockTime")]
    pub block_time: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletRecord {
    #[serde(default)]
    pub sealed: Vec<u8>,
    #[serde(default)]
    pub kaskold_inventory: Vec<u8>,
    pub public: WalletProjection,
    #[serde(default = "zero_string")]
    pub mailbox_checkpoint: String,
    #[serde(default = "zero_string")]
    pub directory_checkpoint: String,
    #[serde(default)]
    pub mailbox_seen_txids: Vec<String>,
    #[serde(default)]
    pub mailbox_pending: BTreeMap<String, PendingFrameBucket>,
    #[serde(default)]
    pub rest_endpoint: Option<String>,
    #[serde(default)]
    pub wrpc_endpoint: Option<String>,
    #[serde(default)]
    pub profile_backup_hash: Option<String>,
    #[serde(default)]
    pub history: Vec<WalletHistoryEntry>,
    #[serde(default)]
    pub used_addresses: Vec<String>,
    #[serde(default)]
    pub registered_address: Option<String>,
}

pub fn zero_string() -> String {
    "0".to_string()
}

/// Sole mutation authority for durable wallet state.
pub struct WalletStateService;

impl WalletStateService {
    fn wallet_mut(wallet_state: &mut Option<WalletRecord>) -> Option<&mut WalletRecord> {
        wallet_state.as_mut()
    }

    pub fn install(wallet_state: &mut Option<WalletRecord>, wallet: WalletRecord) {
        *wallet_state = Some(wallet);
    }

    pub fn merge_progress(
        wallet_state: &mut Option<WalletRecord>,
        incoming: WalletProjection,
    ) -> bool {
        let Some(wallet) = Self::wallet_mut(wallet_state) else {
            return false;
        };
        let before = wallet.public.clone();
        wallet.public.merge_progress(incoming);
        wallet.public != before
    }

    pub fn set_directory_checkpoint(wallet_state: &mut Option<WalletRecord>, checkpoint: String) {
        if let Some(wallet) = Self::wallet_mut(wallet_state) {
            wallet.directory_checkpoint = checkpoint;
        }
    }

    pub fn set_profile_backup_hash(wallet_state: &mut Option<WalletRecord>, hash: String) {
        if let Some(wallet) = Self::wallet_mut(wallet_state) {
            wallet.profile_backup_hash = Some(hash);
        }
    }

    pub fn set_endpoint(
        wallet_state: &mut Option<WalletRecord>,
        kind: &str,
        endpoint: Option<String>,
    ) {
        if let Some(wallet) = Self::wallet_mut(wallet_state) {
            if kind == "rest" {
                wallet.rest_endpoint = endpoint;
            } else {
                wallet.wrpc_endpoint = endpoint;
            }
        }
    }

    pub fn set_registered_address(
        wallet_state: &mut Option<WalletRecord>,
        address: String,
        progress: WalletProjection,
    ) {
        if let Some(wallet) = Self::wallet_mut(wallet_state) {
            wallet.public.merge_progress(progress);
            wallet.registered_address = Some(address);
        }
    }

    pub fn apply_backup_publish(
        wallet_state: &mut Option<WalletRecord>,
        hash: String,
        progress: WalletProjection,
    ) {
        if let Some(wallet) = Self::wallet_mut(wallet_state) {
            wallet.public.merge_progress(progress);
            wallet.profile_backup_hash = Some(hash);
        }
    }

    pub fn replace_history(
        wallet_state: &mut Option<WalletRecord>,
        history: Vec<WalletHistoryEntry>,
        used_addresses: &[String],
        recommended_receive_index: usize,
    ) {
        let Some(wallet) = Self::wallet_mut(wallet_state) else {
            return;
        };
        wallet.history = history;
        append_unique(&mut wallet.used_addresses, used_addresses);
        wallet.public.next_receive_index = wallet.public.next_receive_index.max(
            recommended_receive_index.min(wallet.public.receive_addresses.len().saturating_sub(1)),
        );
    }

    pub fn replace_restored_history(
        wallet_state: &mut Option<WalletRecord>,
        history: Vec<WalletHistoryEntry>,
        used_addresses: Vec<String>,
        recommended_receive_index: usize,
    ) {
        let Some(wallet) = Self::wallet_mut(wallet_state) else {
            return;
        };
        wallet.history = history;
        wallet.used_addresses = used_addresses;
        wallet.public.next_receive_index = wallet.public.next_receive_index.max(
            recommended_receive_index.min(wallet.public.receive_addresses.len().saturating_sub(1)),
        );
    }

    pub fn apply_live_update(
        wallet_state: &mut Option<WalletRecord>,
        checkpoint: String,
        events: &[MailboxEvent],
        snapshot: Option<&WalletSnapshot>,
    ) {
        let Some(wallet) = Self::wallet_mut(wallet_state) else {
            return;
        };
        wallet.mailbox_checkpoint = checkpoint;
        MailboxService::absorb(wallet, events);
        let Some(snapshot) = snapshot else {
            return;
        };
        wallet.history = snapshot.history.clone();
        wallet.public.next_receive_index = wallet.public.next_receive_index.max(
            snapshot
                .recommended_receive_index
                .min(wallet.public.receive_addresses.len().saturating_sub(1)),
        );
        append_unique(&mut wallet.used_addresses, &snapshot.active_addresses);
        for entry in &snapshot.history {
            append_unique(&mut wallet.used_addresses, &entry.addresses);
        }
    }

    pub fn remove_mailbox_envelope(wallet_state: &mut Option<WalletRecord>, packet_id: &str) {
        if let Some(wallet) = Self::wallet_mut(wallet_state) {
            MailboxService::remove(wallet, packet_id);
        }
    }

    pub fn reconcile(
        wallet_state: &mut Option<WalletRecord>,
        baseline: Option<&WalletRecord>,
        updated: Option<WalletRecord>,
    ) {
        let latest = wallet_state.clone();
        let Some(mut remote) = updated.or_else(|| latest.clone()) else {
            *wallet_state = None;
            return;
        };
        if let Some(local) = latest.as_ref() {
            if let Some(baseline) = baseline {
                preserve_user_changes(baseline, local, &mut remote);
            }
            merge_collections(local, &mut remote);
            merge_checkpoints(local, &mut remote);
            remote.public.next_receive_index = remote
                .public
                .next_receive_index
                .max(local.public.next_receive_index);
            remote.public.next_change_index = remote
                .public
                .next_change_index
                .max(local.public.next_change_index);
        }
        *wallet_state = Some(remote);
    }
}

fn preserve_user_changes(baseline: &WalletRecord, local: &WalletRecord, remote: &mut WalletRecord) {
    preserve_pending_packets(baseline, local, remote);
    preserve_user_endpoints(baseline, local, remote);
    if local.profile_backup_hash != baseline.profile_backup_hash {
        remote.profile_backup_hash = local.profile_backup_hash.clone();
    }
    if local.registered_address != baseline.registered_address {
        remote.registered_address = local.registered_address.clone();
    }
    if local.kaskold_inventory != baseline.kaskold_inventory {
        remote.kaskold_inventory = local.kaskold_inventory.clone();
    }
}

fn preserve_pending_packets(
    baseline: &WalletRecord,
    local: &WalletRecord,
    remote: &mut WalletRecord,
) {
    for (packet_id, bucket) in &local.mailbox_pending {
        if baseline.mailbox_pending.get(packet_id) != Some(bucket) {
            remote
                .mailbox_pending
                .entry(packet_id.clone())
                .or_insert_with(|| bucket.clone());
        }
    }
}

fn preserve_user_endpoints(
    baseline: &WalletRecord,
    local: &WalletRecord,
    remote: &mut WalletRecord,
) {
    if local.rest_endpoint != baseline.rest_endpoint {
        remote.rest_endpoint = local.rest_endpoint.clone();
    }
    if local.wrpc_endpoint != baseline.wrpc_endpoint {
        remote.wrpc_endpoint = local.wrpc_endpoint.clone();
    }
}

fn merge_checkpoints(local: &WalletRecord, remote: &mut WalletRecord) {
    if decimal_cmp(&local.mailbox_checkpoint, &remote.mailbox_checkpoint).is_gt() {
        remote.mailbox_checkpoint = local.mailbox_checkpoint.clone();
    }
    if decimal_cmp(&local.directory_checkpoint, &remote.directory_checkpoint).is_gt() {
        remote.directory_checkpoint = local.directory_checkpoint.clone();
    }
}

fn decimal_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let left = normalized_decimal(left);
    let right = normalized_decimal(right);
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn normalized_decimal(value: &str) -> &str {
    let trimmed = value.trim_start_matches('0');
    if trimmed.is_empty() {
        "0"
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests;
