use crate::model::{
    Profile, ProfilePatch, Settings, WalletHistoryEntry, WalletHistoryResult, WalletProjection,
    WalletSnapshot,
};

pub(crate) fn update_setting(
    profile: &Profile,
    field: &str,
    value: String,
) -> Result<ProfilePatch, String> {
    let mut settings = profile.settings.clone();
    match field {
        "route" => settings.route = value,
        "stego" => settings.stego = value,
        _ => return Err(format!("Unknown setting: {field}")),
    }
    Ok(settings_patch(profile, settings))
}

pub(crate) fn update_boolean_setting(
    profile: &Profile,
    field: &str,
    checked: bool,
) -> Result<ProfilePatch, String> {
    let mut settings = profile.settings.clone();
    if apply_runtime_boolean(&mut settings, field, checked)
        || apply_backup_boolean(&mut settings, field, checked)
    {
        return Ok(settings_patch(profile, settings));
    }
    Err(format!("Unknown boolean setting: {field}"))
}

fn apply_runtime_boolean(settings: &mut Settings, field: &str, checked: bool) -> bool {
    match field {
        "autoIgnoreUnknownChats" => settings.auto_ignore_unknown_chats = checked,
        "requireSendPassword" => settings.require_send_password = checked,
        "debugLogging" => settings.debug_logging = checked,
        _ => return false,
    }
    true
}

fn apply_backup_boolean(settings: &mut Settings, field: &str, checked: bool) -> bool {
    match field {
        "contactsBackupKaspa" => settings.contacts_backup_kaspa = checked,
        "backupMessagesKaspa" => settings.backup_messages_kaspa = checked,
        _ => return false,
    }
    true
}

fn settings_patch(profile: &Profile, after: Settings) -> ProfilePatch {
    let mut patch = ProfilePatch::new(&profile.id);
    patch.settings(profile.settings.clone(), after);
    patch
}

pub(crate) fn auto_login_patch(profile: &Profile, enabled: bool) -> ProfilePatch {
    let mut patch = ProfilePatch::new(&profile.id);
    patch.auto_login(enabled);
    patch
}

pub(crate) fn wallet_endpoint_patch(
    profile: &Profile,
    kind: &str,
    endpoint: Option<String>,
) -> ProfilePatch {
    let mut wallet = profile.wallet.clone();
    crate::model::WalletStateService::set_endpoint(&mut wallet, kind, endpoint);
    wallet_patch(profile, wallet)
}

pub(crate) fn wallet_progress_patch(profile: &Profile, progress: WalletProjection) -> ProfilePatch {
    let mut wallet = profile.wallet.clone();
    crate::model::WalletStateService::merge_progress(&mut wallet, progress);
    wallet_patch(profile, wallet)
}

pub(crate) fn wallet_patch(
    profile: &Profile,
    after: Option<crate::model::WalletRecord>,
) -> ProfilePatch {
    let mut patch = ProfilePatch::new(&profile.id);
    patch.wallet(profile.wallet.clone(), after);
    patch
}

pub(crate) fn apply_history_result(
    profile: &Profile,
    mut snapshot: WalletSnapshot,
    result: WalletHistoryResult,
) -> (WalletSnapshot, ProfilePatch, usize) {
    let mut merged = snapshot.history.clone();
    if merged.is_empty() {
        if let Some(wallet) = profile.wallet.as_ref() {
            merged = wallet.history.clone();
        }
    }
    merge_history(&mut merged, result.history);
    snapshot.history = merged.clone();
    let mut wallet = profile.wallet.clone();
    crate::model::WalletStateService::replace_history(
        &mut wallet,
        merged,
        &result.used_addresses,
        result.recommended_receive_index,
    );
    let warnings = result.warnings.len();
    (snapshot, wallet_patch(profile, wallet), warnings)
}

fn merge_history(current: &mut Vec<WalletHistoryEntry>, incoming: Vec<WalletHistoryEntry>) {
    for entry in incoming {
        merge_history_entry(current, entry);
    }
    current.sort_by_key(|entry| {
        std::cmp::Reverse(entry.blue_score.parse::<u64>().unwrap_or_default())
    });
}

fn merge_history_entry(current: &mut Vec<WalletHistoryEntry>, mut entry: WalletHistoryEntry) {
    let Some(existing) = current
        .iter_mut()
        .find(|value| value.transaction_id == entry.transaction_id)
    else {
        current.push(entry);
        return;
    };
    if entry.blue_score.parse::<u64>().unwrap_or_default()
        > existing.blue_score.parse::<u64>().unwrap_or_default()
    {
        existing.blue_score = entry.blue_score.clone();
    }
    if entry.block_time.is_some() {
        existing.block_time = entry.block_time;
    }
    existing.ghost_payload |= entry.ghost_payload;
    for address in entry.addresses.drain(..) {
        if !existing.addresses.contains(&address) {
            existing.addresses.push(address);
        }
    }
}

pub(crate) fn apply_wallet_progress(
    profile: &Profile,
    progress: WalletProjection,
) -> Option<(Profile, ProfilePatch)> {
    let mut updated = profile.clone();
    if !crate::model::WalletStateService::merge_progress(&mut updated.wallet, progress) {
        return None;
    }
    let patch = wallet_patch(profile, updated.wallet.clone());
    Some((updated, patch))
}
