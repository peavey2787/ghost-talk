use crate::{service::chat_by_id_mut, Chat, ChatStore};

/// Authoritative lifecycle manager for the durable projection of HYDRA sessions.
///
/// Public commands identify a chat through its owning store. The application
/// never receives a mutable `Chat`, and the private transition helpers below
/// are the only code that invokes the session-field mutators on `Chat`.
pub struct HydraSessionManager;

impl HydraSessionManager {
    pub fn set_role(chats: &mut ChatStore, chat_id: &str, role: String) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        Self::set_role_chat(chat, role);
        true
    }

    pub fn set_role_at_index(chats: &mut ChatStore, index: usize, role: String) -> bool {
        let Some(chat) = chats.as_mut_slice().get_mut(index) else {
            return false;
        };
        Self::set_role_chat(chat, role);
        true
    }

    pub fn begin(
        chats: &mut ChatStore,
        chat_id: &str,
        sid: Option<String>,
        role: Option<String>,
    ) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        Self::begin_chat(chat, sid, role);
        true
    }

    pub fn begin_at_index(
        chats: &mut ChatStore,
        index: usize,
        sid: Option<String>,
        role: Option<String>,
    ) -> bool {
        let Some(chat) = chats.as_mut_slice().get_mut(index) else {
            return false;
        };
        Self::begin_chat(chat, sid, role);
        true
    }

    pub fn establish(chats: &mut ChatStore, chat_id: &str, sid: String, role: String) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        Self::establish_chat(chat, sid, role);
        true
    }

    pub fn mark_restore(
        chats: &mut ChatStore,
        chat_id: &str,
        sid: Option<String>,
        role: Option<String>,
    ) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.mark_transport_restore(sid, role);
        true
    }

    pub fn set_restore_message_id(
        chats: &mut ChatStore,
        chat_id: &str,
        id: Option<String>,
    ) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.set_transport_restore_message_id(id);
        true
    }

    pub fn merge_resumed(chats: &mut ChatStore, chat_id: &str, resumed: &Chat) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.merge_resumed_transport(resumed);
        true
    }

    pub fn clear_restore(chats: &mut ChatStore, chat_id: &str) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.clear_transport_restore();
        true
    }

    pub fn complete_bootstrap(chats: &mut ChatStore, chat_id: &str, sid: Option<String>) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.complete_bootstrap(sid);
        true
    }

    pub fn reset(chats: &mut ChatStore, chat_id: &str) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        Self::reset_chat(chat);
        true
    }

    pub fn rejoin(
        chats: &mut ChatStore,
        chat_id: &str,
        sid: Option<String>,
        role: Option<String>,
        restore_pending: bool,
        restore_message_id: Option<String>,
    ) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.rejoin_with_session(sid, role, restore_pending, restore_message_id);
        true
    }

    pub fn rejoin_without_session(chats: &mut ChatStore, chat_id: &str) -> bool {
        let Some(chat) = chat_by_id_mut(chats, chat_id) else {
            return false;
        };
        chat.rejoin_without_session();
        true
    }

    pub(crate) fn set_role_chat(chat: &mut Chat, role: String) {
        chat.set_session_role(role);
    }
    pub(crate) fn begin_chat(chat: &mut Chat, sid: Option<String>, role: Option<String>) {
        chat.begin_session(sid, role);
    }
    pub(crate) fn establish_chat(chat: &mut Chat, sid: String, role: String) {
        chat.establish_session(sid, role);
    }
    pub(crate) fn reset_chat(chat: &mut Chat) {
        chat.reset_transport();
    }
    pub(crate) fn clear_after_peer_leave(chat: &mut Chat) {
        chat.mark_transport_restore(None, chat.session_role.clone());
        chat.clear_transport_restore();
    }
}
