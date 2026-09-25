use ghost_domain::wallet::WalletProjection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupContact {
    pub id: String,
    pub label: String,
    pub kaspa_address: String,
    #[serde(default)]
    pub hydra_handle: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupMessage {
    pub chat_id: String,
    #[serde(default)]
    pub contact_id: Option<String>,
    pub chat_label: String,
    pub id: String,
    pub direction: String,
    pub body: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileBackupArchive {
    #[serde(default)]
    pub version: u8,
    #[serde(default)]
    pub saved_at_ms: u64,
    #[serde(default)]
    pub contacts: Vec<BackupContact>,
    #[serde(default)]
    pub messages: Vec<BackupMessage>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProfileBackupPublishResult {
    #[serde(default)]
    pub transaction_ids: Vec<String>,
    pub content_hash: String,
    pub public: WalletProjection,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProfileBackupRestoreResult {
    pub content_hash: String,
    pub archive: ProfileBackupArchive,
}


const MAX_BACKUP_CONTACTS: usize = 4096;
const MAX_BACKUP_MESSAGES: usize = 10_000;

/// Validate the portable profile-backup payload shared by native and Web hosts.
pub fn validate_profile_backup_inputs(
    contacts: &[BackupContact],
    messages: &[BackupMessage],
) -> Result<(), String> {
    if contacts.len() > MAX_BACKUP_CONTACTS {
        return Err("profile backup contains too many contacts".into());
    }
    if messages.len() > MAX_BACKUP_MESSAGES {
        return Err("profile backup contains too many messages".into());
    }
    contacts.iter().try_for_each(validate_backup_contact)?;
    messages.iter().try_for_each(validate_backup_message)
}

fn validate_backup_contact(contact: &BackupContact) -> Result<(), String> {
    bounded(&contact.id, 1, 128, "contact id")?;
    bounded(&contact.label, 1, 256, "contact label")?;
    bounded(&contact.kaspa_address, 1, 192, "contact Kaspa address")?;
    ghost_core::KaspaAddress::parse(&contact.kaspa_address)?;
    if let Some(handle) = contact.hydra_handle.as_deref() {
        bounded(handle, 1, 256, "HYDRA contact id")?;
    }
    Ok(())
}

fn validate_backup_message(message: &BackupMessage) -> Result<(), String> {
    bounded(&message.chat_id, 1, 160, "backup chat id")?;
    bounded(&message.chat_label, 1, 256, "backup chat label")?;
    bounded(&message.id, 1, 160, "backup message id")?;
    if let Some(contact_id) = message.contact_id.as_deref() {
        bounded(contact_id, 1, 128, "backup contact id")?;
    }
    if !matches!(message.direction.as_str(), "in" | "out") {
        return Err("backup message direction must be in or out".into());
    }
    if message.body.is_empty() || message.body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("backup message body is empty or exceeds the event limit".into());
    }
    Ok(())
}

fn bounded(value: &str, min: usize, max: usize, label: &str) -> Result<(), String> {
    if value.len() < min || value.len() > max {
        Err(format!("{label} length is invalid"))
    } else {
        Ok(())
    }
}
