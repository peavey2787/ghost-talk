use crate::model::{Profile, ProfilePatch};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::settings::{SettingsProps, SettingsState};

pub(super) fn backup_callback(
    props: &SettingsProps,
    state: &SettingsState,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        if *busy {
            return;
        }
        if let Err(error) = validate_backup_request(&profile) {
            status.set(error);
            return;
        }
        busy.set(true);
        status.set("Encrypting and publishing profile backup…".into());
        publish_backup_async(
            profile.clone(),
            password.clone(),
            status.clone(),
            busy.clone(),
            on_update.clone(),
        );
    })
}

fn validate_backup_request(profile: &Profile) -> Result<(), String> {
    if !profile.recovery_backup_confirmed() {
        return Err("Confirm the 24-word recovery backup before publishing an encrypted Kaspa profile backup.".into());
    }
    if !profile.settings.contacts_backup_kaspa && !profile.settings.backup_messages_kaspa {
        return Err("Enable contact backup or message backup first.".into());
    }
    Ok(())
}

fn publish_backup_async(
    profile: Profile,
    password: String,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
    on_update: Callback<ProfilePatch>,
) {
    spawn_local(async move {
        let contacts = if profile.settings.contacts_backup_kaspa {
            crate::controllers::account::backup_contacts(&profile)
        } else {
            Default::default()
        };
        let messages = if profile.settings.backup_messages_kaspa {
            crate::controllers::account::backup_messages(&profile)
        } else {
            Default::default()
        };
        match crate::controllers::account::publish_profile_backup(
            &profile, &password, contacts, messages,
        )
        .await
        {
            Ok(result) => apply_published_backup(profile, result, status, on_update),
            Err(error) => status.set(error),
        }
        busy.set(false);
    });
}

fn apply_published_backup(
    profile: Profile,
    result: crate::model::ProfileBackupPublishResult,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
) {
    let count = result.transaction_ids.len();
    on_update.emit(crate::controllers::account::published_backup_patch(
        &profile, &result,
    ));
    status.set(format!(
        "Encrypted Kaspa backup published ({count} transaction{}).",
        if count == 1 { "" } else { "s" }
    ));
}

pub(super) fn restore_backup_callback(
    props: &SettingsProps,
    state: &SettingsState,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        if *busy {
            return;
        }
        busy.set(true);
        status.set("Looking for the latest archival Kaspa profile backup…".into());
        restore_backup_async(
            profile.clone(),
            password.clone(),
            status.clone(),
            busy.clone(),
            on_update.clone(),
        );
    })
}

fn restore_backup_async(
    profile: Profile,
    password: String,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
    on_update: Callback<ProfilePatch>,
) {
    spawn_local(async move {
        match crate::controllers::account::restore_profile_backup(&profile, &password).await {
            Ok(Some(result)) => {
                on_update.emit(crate::controllers::account::restored_backup_patch(&profile, result));
                status.set("Encrypted Kaspa profile backup restored and merged.".into());
            }
            Ok(None) => status.set("No archival profile backup was found yet. Backups younger than 48 hours stay on the live path and are not queried through REST.".into()),
            Err(error) => status.set(error),
        }
        busy.set(false);
    });
}

pub(super) fn backup_fingerprint(profile: &Profile) -> String {
    profile
        .wallet
        .as_ref()
        .and_then(|wallet| wallet.profile_backup_hash.as_deref())
        .map(|hash| hash.chars().take(16).collect())
        .unwrap_or_else(|| "none".into())
}
