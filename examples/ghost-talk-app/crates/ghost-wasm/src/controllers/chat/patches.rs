use crate::model::{Profile, ProfilePatch};

pub(crate) fn chat_store_transition(
    before: &Profile,
    after: &ghost_chat::ChatStore,
    chat_id: &str,
) -> ProfilePatch {
    let mut patch = ProfilePatch::new(&before.id);
    let old = ghost_chat::ChatService::by_id(&before.chats, chat_id).cloned();
    if let Some(new) = ghost_chat::ChatService::by_id(after, chat_id).cloned() {
        patch.chat_upsert(old, new);
    }
    patch
}

pub(crate) fn chat_profile_transition(
    before: &Profile,
    after: &Profile,
    chat_id: &str,
) -> ProfilePatch {
    chat_store_transition(before, &after.chats, chat_id)
}

pub(crate) fn chat_wallet_transition(
    before: &Profile,
    after: &Profile,
    chat_id: &str,
) -> ProfilePatch {
    let mut patch = chat_profile_transition(before, after, chat_id);
    patch.wallet(before.wallet.clone(), after.wallet.clone());
    patch
}
