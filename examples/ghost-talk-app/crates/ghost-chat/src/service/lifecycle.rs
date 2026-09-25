use super::{helpers::chat_by_id_mut, ChatService};
use crate::{ChatStore, Message};
use ghost_domain::identity::PeerBinding;

impl ChatService {
    pub fn reopen_at_index(chats: &mut ChatStore, index: usize) -> Option<(String, bool)> {
        let chat = chats.as_mut_slice().get_mut(index)?;
        let was_archived = chat.archived;
        if was_archived {
            chat.set_archived(false);
        }
        Some((chat.id.clone(), was_archived))
    }

    pub fn mark_read(chats: &mut ChatStore, id: &str) -> Option<bool> {
        let chat = chat_by_id_mut(chats, id)?;
        let was_unread = chat.unread_count != 0;
        if was_unread {
            chat.mark_read();
        }
        Some(was_unread)
    }

    pub fn toggle_archive(chats: &mut ChatStore, id: &str) -> Option<bool> {
        let chat = chat_by_id_mut(chats, id)?;
        chat.toggle_archived();
        Some(chat.archived)
    }

    pub fn set_contact_id(chats: &mut ChatStore, id: &str, contact_id: Option<String>) -> bool {
        let Some(chat) = chat_by_id_mut(chats, id) else {
            return false;
        };
        chat.set_contact_id(contact_id);
        true
    }

    pub fn set_peer_name_aliases_at_index(
        chats: &mut ChatStore,
        index: usize,
        kns_name: Option<String>,
        dotk_name: Option<String>,
    ) -> bool {
        let Some(chat) = chats.as_mut_slice().get_mut(index) else {
            return false;
        };
        let changed = chat.peer_kns_name != kns_name || chat.peer_dotk_name != dotk_name;
        if changed {
            chat.set_peer_name_aliases(kns_name, dotk_name);
        }
        changed
    }

    pub fn leave(chats: &mut ChatStore, id: &str) -> bool {
        let Some(chat) = chat_by_id_mut(chats, id) else {
            return false;
        };
        chat.leave_local();
        crate::HydraSessionManager::reset_chat(chat);
        true
    }

    pub fn mark_peer_left(chats: &mut ChatStore, id: &str) -> bool {
        let Some(chat) = chat_by_id_mut(chats, id) else {
            return false;
        };
        chat.mark_peer_left();
        crate::HydraSessionManager::clear_after_peer_leave(chat);
        true
    }

    pub fn record_incoming_message(chats: &mut ChatStore, chat_id: &str, message: Message) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.push_message(message);
        chat.increment_unread();
        true
    }

    pub fn mark_peer_left_by_hydra(chats: &mut ChatStore, peer_hydra_id: &str) -> usize {
        let mut changed = 0;
        for chat in chats.as_mut_slice() {
            if chat.peer_hydra_handle.as_deref() == Some(peer_hydra_id) && !chat.peer_left {
                chat.mark_peer_left();
                crate::HydraSessionManager::clear_after_peer_leave(chat);
                changed += 1;
            }
        }
        changed
    }

    pub fn retire_room_transports_for_peer(chats: &mut ChatStore, peer_hydra_id: &str) -> bool {
        for chat in chats.as_mut_slice() {
            if chat.room_transport_only
                && chat.peer_hydra_handle.as_deref() == Some(peer_hydra_id)
                && !chat.left
            {
                chat.leave_local();
                crate::HydraSessionManager::reset_chat(chat);
            }
        }
        !chats.iter().any(|chat| {
            !chat.left
                && !chat.peer_left
                && chat.peer_hydra_handle.as_deref() == Some(peer_hydra_id)
        })
    }

    pub fn bind_peer(chats: &mut ChatStore, id: &str, peer: &PeerBinding) -> bool {
        let Some(chat) = chat_by_id_mut(chats, id) else {
            return false;
        };
        chat.set_peer_binding(peer);
        true
    }

    pub fn bind_peer_at_index(chats: &mut ChatStore, index: usize, peer: &PeerBinding) -> bool {
        let Some(chat) = chats.as_mut_slice().get_mut(index) else {
            return false;
        };
        chat.set_peer_binding(peer);
        true
    }
}
