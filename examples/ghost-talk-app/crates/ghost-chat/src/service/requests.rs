use super::{helpers::chat_by_id_mut, ChatService};
use crate::{Chat, ChatStore, IncomingRequest};
use ghost_domain::identity::PeerBinding;

impl ChatService {
    pub fn set_incoming_request(
        chats: &mut ChatStore,
        id: &str,
        request: IncomingRequest,
        room_transport_only: bool,
    ) -> bool {
        let Some(chat) = chat_by_id_mut(chats, id) else {
            return false;
        };
        chat.set_incoming_request(request);
        chat.set_room_transport_only(room_transport_only);
        true
    }

    pub fn clear_incoming_request(chats: &mut ChatStore, id: &str) -> bool {
        let Some(chat) = chat_by_id_mut(chats, id) else {
            return false;
        };
        chat.clear_incoming_request();
        true
    }

    pub fn clear_incoming_request_at_index(chats: &mut ChatStore, index: usize) -> bool {
        let Some(chat) = chats.as_mut_slice().get_mut(index) else {
            return false;
        };
        chat.clear_incoming_request();
        true
    }

    pub fn set_incoming_request_state(chats: &mut ChatStore, id: &str, state: &str) -> bool {
        let Some(chat) = chat_by_id_mut(chats, id) else {
            return false;
        };
        chat.set_incoming_request_state(state);
        true
    }

    pub fn accept_incoming_request(
        chats: &mut ChatStore,
        id: &str,
        contact_id: Option<String>,
        peer: &PeerBinding,
        request_id: String,
        room_transport_only: bool,
    ) -> bool {
        let Some(chat) = chat_by_id_mut(chats, id) else {
            return false;
        };
        chat.bind_accepted_identity(contact_id, peer, room_transport_only);
        crate::HydraSessionManager::begin_chat(chat, Some(request_id), Some("responder".into()));
        chat.set_incoming_request_state("accepted");
        true
    }

    pub fn hidden_room_transport_from(chat: &Chat, replacement_id: String) -> Chat {
        let mut transport = chat.clone();
        transport.convert_to_hidden_room_transport(replacement_id);
        transport
    }

    pub fn convert_to_hidden_room_transport(
        chats: &mut ChatStore,
        id: &str,
        replacement_id: String,
    ) -> Option<Chat> {
        let index = chats.position(|chat| chat.id == id)?;
        let mut chat = chats.as_slice()[index].clone();
        chat.convert_to_hidden_room_transport(replacement_id);
        chats.retain_owned(|existing| existing.id != id);
        Some(chat)
    }

    pub fn reopen_room_transport_at(
        chats: &mut ChatStore,
        index: usize,
        label: String,
        hydra_id: Option<String>,
    ) -> Option<String> {
        let chat = chats.as_mut_slice().get_mut(index)?;
        chat.reopen_room_transport(label, hydra_id);
        Some(chat.id.clone())
    }
}
