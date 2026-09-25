use crate::components::call::identity::{call_peer_matches_chat, call_peer_matches_contact};
use crate::model::{CallRecord, Profile, ProfilePatch};

pub(crate) struct CallChatBinding {
    pub(crate) chat_id: String,
    pub(crate) profile: Option<Profile>,
    pub(crate) patch: Option<ProfilePatch>,
}

pub(crate) fn ensure_chat(profile: &Profile, call: &CallRecord) -> Result<CallChatBinding, String> {
    if let Some(chat_id) = call.chat_id.clone() {
        return Ok(CallChatBinding {
            chat_id,
            profile: None,
            patch: None,
        });
    }
    if let Some(chat_id) = existing_call_chat_id(profile, call) {
        return Ok(CallChatBinding {
            chat_id,
            profile: None,
            patch: None,
        });
    }
    let id = crate::random_id()?;
    let contact = profile
        .contacts
        .iter()
        .find(|contact| call_peer_matches_contact(contact, call));
    let label = contact
        .map(|contact| contact.label.clone())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| call.peer_address.clone());
    let new_chat = ghost_chat::ChatService::new_direct_chat(ghost_chat::DirectChatSpec {
        id: id.clone(),
        label,
        contact_id: contact.map(|contact| contact.id.clone()),
        kaspa_address: call.peer_address.clone(),
        hydra_handle: Some(call.peer_hydra_id.clone()),
        kns_name: contact.and_then(|contact| contact.kns_name.clone()),
        dotk_name: contact.and_then(|contact| contact.dotk_name.clone()),
        verified_public: false,
    });
    let mut updated = profile.clone();
    ghost_chat::ChatService::insert_front(&mut updated.chats, new_chat.clone());
    let mut patch = ProfilePatch::new(&profile.id);
    patch.chat_upsert(None, new_chat);
    Ok(CallChatBinding {
        chat_id: id,
        profile: Some(updated),
        patch: Some(patch),
    })
}

fn existing_call_chat_id(profile: &Profile, call: &CallRecord) -> Option<String> {
    profile
        .chats
        .iter()
        .find(|chat| {
            !chat.room_transport_only()
                && !chat.archived()
                && !chat.left()
                && !chat.peer_left()
                && call_peer_matches_chat(chat, call)
        })
        .map(|chat| chat.id.clone())
}
