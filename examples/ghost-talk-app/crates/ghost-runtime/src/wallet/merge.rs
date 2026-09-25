use super::WalletRecord;

pub(super) fn merge_collections(local: &WalletRecord, remote: &mut WalletRecord) {
    append_unique(&mut remote.mailbox_seen_txids, &local.mailbox_seen_txids);
    append_unique(&mut remote.used_addresses, &local.used_addresses);
    for entry in &local.history {
        if !remote
            .history
            .iter()
            .any(|existing| existing.transaction_id == entry.transaction_id)
        {
            remote.history.push(entry.clone());
        }
    }
}

pub(super) fn append_unique(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}
