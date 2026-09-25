use super::{
    invoke, json, BackupContact, BackupMessage, Profile, ProfileBackupPublishResult,
    ProfileBackupRestoreResult,
};

pub async fn publish_profile_backup(
    profile: &Profile,
    password: &str,
    contacts: Vec<BackupContact>,
    messages: Vec<BackupMessage>,
) -> Result<ProfileBackupPublishResult, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke(
        "profile_backup_publish",
        json!({
            "profileId": profile.id,
            "password": password,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "contacts": contacts,
            "messages": messages,
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }),
    )
    .await
}

pub async fn restore_profile_backup(
    profile: &Profile,
    password: &str,
) -> Result<Option<ProfileBackupRestoreResult>, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    invoke(
        "profile_backup_restore",
        json!({
            "password": password,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "restEndpoint": wallet.rest_endpoint,
        }),
    )
    .await
}
