use super::{
    helpers::{chat_by_id_mut, message_by_id_mut, message_by_index_mut},
    ChatService,
};
use crate::{ChatStore, Message};

impl ChatService {
    pub fn record_message(chats: &mut ChatStore, chat_id: &str, message: Message) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.push_message(message);
        true
    }

    pub fn record_message_at_index(chats: &mut ChatStore, index: usize, message: Message) -> bool {
        let Some(chat) = chats.as_mut_slice().get_mut(index) else {
            return false;
        };
        chat.push_message(message);
        true
    }

    pub fn increment_unread(chats: &mut ChatStore, chat_id: &str) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.increment_unread();
        true
    }

    pub fn sort_messages(chats: &mut ChatStore) {
        for chat in chats.as_mut_slice() {
            chat.sort_messages_by_created_at();
        }
    }

    pub fn remove_messages_by_id(
        chats: &mut ChatStore,
        chat_id: &str,
        message_ids: &[String],
    ) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.retain_messages(|message| !message_ids.iter().any(|id| id == &message.id));
        true
    }

    pub fn replace_message_body_at(
        chats: &mut ChatStore,
        chat_index: usize,
        message_index: usize,
        body: String,
    ) -> bool {
        let Some(message) = message_by_index_mut(chats, chat_index, message_index) else {
            return false;
        };
        message.replace_body(body);
        true
    }

    pub fn apply_message_send_result_at(
        chats: &mut ChatStore,
        chat_index: usize,
        message_index: usize,
        transaction_id: Option<String>,
        pending: bool,
        pending_id: Option<String>,
        pending_stage: Option<String>,
    ) -> bool {
        let Some(message) = message_by_index_mut(chats, chat_index, message_index) else {
            return false;
        };
        message.apply_send_result(transaction_id, pending, pending_id, pending_stage);
        true
    }

    pub fn apply_message_send_result(
        chats: &mut ChatStore,
        chat_id: &str,
        message_id: &str,
        transaction_id: Option<String>,
        pending: bool,
        pending_id: Option<String>,
        pending_stage: Option<String>,
    ) -> bool {
        let Some(message) = message_by_id_mut(chats, chat_id, message_id) else {
            return false;
        };
        message.apply_send_result(transaction_id, pending, pending_id, pending_stage);
        true
    }

    pub fn mark_message_finish_sent(
        chats: &mut ChatStore,
        chat_id: &str,
        message_id: &str,
        transaction_id: String,
    ) -> bool {
        let Some(message) = message_by_id_mut(chats, chat_id, message_id) else {
            return false;
        };
        message.mark_finish_sent(transaction_id);
        true
    }

    pub fn mark_message_sent_by_contact_request(
        chats: &mut ChatStore,
        chat_id: &str,
        request_id: &str,
        transaction_id: String,
    ) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        let Some(message) = chat
            .messages
            .iter_mut()
            .find(|message| message.contact_request_id.as_deref() == Some(request_id))
        else {
            return false;
        };
        message.mark_sent(transaction_id);
        true
    }

    pub fn set_message_handshake_ready_at(
        chats: &mut ChatStore,
        chat_index: usize,
        message_index: usize,
    ) -> bool {
        let Some(message) = message_by_index_mut(chats, chat_index, message_index) else {
            return false;
        };
        message.set_pending_stage(Some("handshake".into()));
        message.clear_contact_request_id();
        true
    }

    pub fn set_message_reaction(
        chats: &mut ChatStore,
        chat_id: &str,
        message_id: &str,
        actor_id: &str,
        kind: Option<ghost_domain::reaction::ReactionKind>,
    ) -> bool {
        let Some(message) = message_by_id_mut(chats, chat_id, message_id) else {
            return false;
        };
        message.set_reaction(actor_id, kind)
    }

    pub fn mark_message_delivered(chats: &mut ChatStore, chat_id: &str, message_id: &str) -> bool {
        let Some(message) = message_by_id_mut(chats, chat_id, message_id) else {
            return false;
        };
        message.mark_delivered();
        true
    }

    pub fn mark_message_failed(
        chats: &mut ChatStore,
        chat_id: &str,
        message_id: &str,
        error: String,
    ) -> bool {
        let Some(message) = message_by_id_mut(chats, chat_id, message_id) else {
            return false;
        };
        message.mark_failed(error);
        true
    }

    pub fn mark_message_sending(chats: &mut ChatStore, chat_id: &str, message_id: &str) -> bool {
        let Some(message) = message_by_id_mut(chats, chat_id, message_id) else {
            return false;
        };
        message.mark_sending();
        true
    }

    pub fn mark_message_queued_handshake(
        chats: &mut ChatStore,
        chat_id: &str,
        message_id: &str,
    ) -> bool {
        let Some(message) = message_by_id_mut(chats, chat_id, message_id) else {
            return false;
        };
        message.mark_queued_handshake();
        true
    }

    pub fn clear_message_pending_id_for_restore(
        chats: &mut ChatStore,
        chat_id: &str,
        message_id: &str,
    ) -> bool {
        let Some(message) = message_by_id_mut(chats, chat_id, message_id) else {
            return false;
        };
        message.clear_pending_id_for_restore();
        true
    }
}
