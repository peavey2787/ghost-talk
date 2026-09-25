use crate::model::{CallRecord, Chat, Contact};
use ghost_domain::identity::PeerBinding;

fn call_peer_binding(call: &CallRecord) -> Option<PeerBinding> {
    PeerBinding::new(call.peer_address.clone(), call.peer_hydra_id.clone()).ok()
}

pub(crate) fn call_peer_matches_chat(chat: &Chat, call: &CallRecord) -> bool {
    call_peer_binding(call).is_some_and(|peer| chat.matches_peer(&peer))
}

pub(crate) fn call_peer_matches_contact(contact: &Contact, call: &CallRecord) -> bool {
    call_peer_binding(call).is_some_and(|peer| contact.matches_peer(&peer))
}
