use crate::model::{
    BackupContact, BackupMessage, Profile, ProfileBackupPublishResult, ProfileBackupRestoreResult,
    WalletHistoryResult, WalletRecovery,
};
mod backup;
mod identity;
mod state;
mod transactions;
pub(crate) use backup::{
    backup_contacts, backup_messages, published_backup_patch, restored_backup_patch,
};
pub(crate) use identity::{
    confirm_recovery_backup, create_or_restore_profile, set_passwordless_and_maybe_unlock,
    IdentitySetup,
};
pub(crate) use state::{
    apply_history_result, apply_wallet_progress, auto_login_patch, update_boolean_setting,
    update_setting, wallet_endpoint_patch,
};
pub(crate) use transactions::{
    broadcast_signer_send_and_patch, consolidate_and_patch, prepare_signer_send,
    send_kaspa_and_patch,
};

pub(crate) async fn unlock_profile_runtime(
    profile: Profile,
    password: &str,
) -> Result<Profile, String> {
    crate::native::unlock_profile_runtime(profile, password).await
}

pub(crate) async fn reveal_recovery(
    profile: &Profile,
    password: &str,
) -> Result<WalletRecovery, String> {
    crate::native::reveal_recovery(profile, password).await
}

pub(crate) async fn gather_wallet_history(
    profile: &Profile,
    priority_addresses: &[String],
) -> Result<WalletHistoryResult, String> {
    crate::native::gather_wallet_history(profile, priority_addresses).await
}

pub(crate) async fn set_remembered_unlock(
    profile: &Profile,
    enabled: bool,
    password: &str,
) -> Result<(), String> {
    crate::native::set_remembered_unlock(profile, enabled, password).await
}

pub(crate) async fn set_debug_logging(enabled: bool) -> Result<(), String> {
    crate::native::set_debug_logging(enabled).await
}

pub(crate) async fn publish_profile_backup(
    profile: &Profile,
    password: &str,
    contacts: Vec<BackupContact>,
    messages: Vec<BackupMessage>,
) -> Result<ProfileBackupPublishResult, String> {
    crate::native::publish_profile_backup(profile, password, contacts, messages).await
}

pub(crate) async fn restore_profile_backup(
    profile: &Profile,
    password: &str,
) -> Result<Option<ProfileBackupRestoreResult>, String> {
    crate::native::restore_profile_backup(profile, password).await
}
