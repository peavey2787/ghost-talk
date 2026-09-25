use ghost_api::{BackupContact, BackupMessage};

pub(crate) fn validate_archive_inputs(
    contacts: &[BackupContact],
    messages: &[BackupMessage],
) -> Result<(), String> {
    ghost_api::validate_profile_backup_inputs(contacts, messages)
}
