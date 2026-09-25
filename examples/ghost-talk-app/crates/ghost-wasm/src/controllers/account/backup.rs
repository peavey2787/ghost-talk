use crate::model::{
    BackupContact, BackupMessage, Message, Profile, ProfileBackupPublishResult,
    ProfileBackupRestoreResult, ProfilePatch,
};
use ghost_contacts::{ContactService, ContactStore};
use ghost_domain::identity::PeerBinding;

pub(crate) fn backup_contacts(profile: &Profile) -> Vec<BackupContact> {
    profile
        .contacts
        .iter()
        .map(|contact| BackupContact {
            id: contact.id.clone(),
            label: contact.label.clone(),
            kaspa_address: contact.kaspa_address().to_string(),
            hydra_handle: contact.hydra_handle().map(str::to_string),
        })
        .collect()
}

pub(crate) fn backup_messages(profile: &Profile) -> Vec<BackupMessage> {
    profile
        .chats
        .iter()
        .flat_map(|chat| {
            chat.messages().iter().map(move |message| BackupMessage {
                chat_id: chat.id.clone(),
                contact_id: chat.contact_id().map(str::to_owned),
                chat_label: chat.label.clone(),
                id: message.id.clone(),
                direction: message.direction.clone(),
                body: message.body.clone(),
                created_at: message.created_at.max(0.0) as u64,
            })
        })
        .collect()
}

pub(crate) fn published_backup_patch(
    profile: &Profile,
    result: &ProfileBackupPublishResult,
) -> ProfilePatch {
    let mut wallet = profile.wallet.clone();
    crate::model::WalletStateService::apply_backup_publish(
        &mut wallet,
        result.content_hash.clone(),
        result.public.clone(),
    );
    super::state::wallet_patch(profile, wallet)
}

pub(crate) fn restored_backup_patch(
    profile: &Profile,
    result: ProfileBackupRestoreResult,
) -> ProfilePatch {
    let mut contacts = profile.contacts.clone();
    let mut chats = profile.chats.clone();
    let mut patch = ProfilePatch::new(&profile.id);
    merge_contacts(&mut contacts, result.archive.contacts, &mut patch);
    merge_messages(&mut chats, &contacts, result.archive.messages, &mut patch);
    sort_messages(&mut chats, &mut patch);
    let mut wallet = profile.wallet.clone();
    crate::model::WalletStateService::set_profile_backup_hash(&mut wallet, result.content_hash);
    patch.wallet(profile.wallet.clone(), wallet);
    patch
}

fn merge_contacts(
    store: &mut ContactStore,
    contacts: Vec<BackupContact>,
    patch: &mut ProfilePatch,
) {
    for saved in contacts {
        if let Some(index) = backup_contact_index(store, &saved) {
            let before = store.get(index).cloned();
            merge_saved_contact(store, index, saved);
            if let Some(after) = store.get(index).cloned() {
                patch.contact_upsert(before, after);
            }
        } else {
            let label = ContactService::unique_label(store, &saved.label, None);
            let contact = ContactService::new_contact(
                saved.id,
                label,
                saved.kaspa_address,
                saved.hydra_handle,
            );
            ContactService::push(store, contact.clone());
            patch.contact_upsert(None, contact);
        }
    }
}

fn backup_contact_index(store: &ContactStore, saved: &BackupContact) -> Option<usize> {
    if let Some(index) = store.iter().position(|contact| contact.id == saved.id) {
        return Some(index);
    }
    let peer = PeerBinding::from_optional(
        (!saved.kaspa_address.trim().is_empty()).then_some(saved.kaspa_address.as_str()),
        saved.hydra_handle.as_deref(),
    )?;
    ContactService::resolve_peer(store, &peer)
        .and_then(|matched| store.iter().position(|contact| contact.id == matched.id))
}

fn merge_saved_contact(store: &mut ContactStore, index: usize, saved: BackupContact) {
    let Some(contact_id) = store.get(index).map(|contact| contact.id.clone()) else {
        return;
    };
    let label = ContactService::unique_label(store, &saved.label, Some(&contact_id));
    let _ = ContactService::restore_missing_identity_by_id(
        store,
        &contact_id,
        label,
        saved.hydra_handle,
    );
}

fn merge_messages(
    chats: &mut ghost_chat::ChatStore,
    contacts: &ContactStore,
    messages: Vec<BackupMessage>,
    patch: &mut ProfilePatch,
) {
    for saved in messages {
        let (chat_index, created) = ensure_backup_chat(chats, contacts, &saved);
        if created {
            if let Some(chat) = chats.get(chat_index).cloned() {
                patch.chat_upsert(None, chat);
            }
        }
        let duplicate = chats
            .get(chat_index)
            .is_some_and(|chat| chat.messages().iter().any(|message| message.id == saved.id));
        if duplicate {
            continue;
        }
        let before = chats.get(chat_index).cloned();
        let _ = ghost_chat::ChatService::record_message_at_index(
            chats,
            chat_index,
            Message {
                id: saved.id,
                direction: saved.direction,
                body: saved.body,
                created_at: saved.created_at as f64,
                send_state: Some("delivered".into()),
                ..Default::default()
            },
        );
        if let Some(after) = chats.get(chat_index).cloned() {
            patch.chat_upsert(before, after);
        }
    }
}

fn ensure_backup_chat(
    chats: &mut ghost_chat::ChatStore,
    contacts: &ContactStore,
    saved: &BackupMessage,
) -> (usize, bool) {
    if let Some(index) = chats.iter().position(|chat| chat.id == saved.chat_id) {
        return (index, false);
    }
    let contact = saved
        .contact_id
        .as_ref()
        .and_then(|id| contacts.iter().find(|contact| &contact.id == id));
    let chat = ghost_chat::ChatService::new_restored_chat(ghost_chat::RestoredChatSpec {
        id: saved.chat_id.clone(),
        label: saved.chat_label.clone(),
        contact_id: saved.contact_id.clone(),
        kaspa_address: contact.map(|value| value.kaspa_address().to_string()),
        hydra_handle: contact.and_then(|value| value.hydra_handle().map(str::to_string)),
        kns_name: contact.and_then(|value| value.kns_name.clone()),
        dotk_name: contact.and_then(|value| value.dotk_name.clone()),
    });
    ghost_chat::ChatService::push(chats, chat);
    (chats.len() - 1, true)
}

fn sort_messages(chats: &mut ghost_chat::ChatStore, patch: &mut ProfilePatch) {
    let before = chats.iter().cloned().collect::<Vec<_>>();
    ghost_chat::ChatService::sort_messages(chats);
    for old in before {
        if let Some(after) = ghost_chat::ChatService::by_id(chats, &old.id).cloned() {
            patch.chat_upsert(Some(old), after);
        }
    }
}
