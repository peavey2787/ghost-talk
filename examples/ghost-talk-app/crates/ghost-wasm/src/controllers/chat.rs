mod lifecycle;
mod patches;
pub(crate) mod reactions;
mod requests;
mod send;
pub(crate) use lifecycle::{delete_patch, notify_leave, prepare_leave, rejoin, LeaveChatPlan};
pub(crate) use patches::{chat_profile_transition, chat_store_transition, chat_wallet_transition};
pub(crate) use requests::{
    accept_request, add_contact_patch, incoming_request_state_patch, toggle_archive_patch,
};
mod direct;
pub(crate) use direct::record_direct_text;
pub(crate) use send::{execute_send, prepare_send};

pub(crate) fn begin_session(
    chats: &mut crate::model::ChatStore,
    chat_id: &str,
    sid: Option<String>,
    role: Option<String>,
) -> bool {
    ghost_chat::HydraSessionManager::begin(chats, chat_id, sid, role)
}

pub(crate) fn begin_session_at_index(
    chats: &mut crate::model::ChatStore,
    index: usize,
    sid: Option<String>,
    role: Option<String>,
) -> bool {
    ghost_chat::HydraSessionManager::begin_at_index(chats, index, sid, role)
}

pub(crate) fn establish_session(
    chats: &mut crate::model::ChatStore,
    chat_id: &str,
    sid: String,
    role: String,
) -> bool {
    crate::app::ApplicationRouter::route_chat(
        chats,
        crate::app::ApplicationEvent::ChatSessionBound {
            chat_id: chat_id.to_owned(),
            sid,
            role,
        },
    )
}

pub(crate) fn bind_peer_at_index(
    chats: &mut crate::model::ChatStore,
    index: usize,
    peer: &ghost_domain::identity::PeerBinding,
) -> bool {
    ghost_chat::ChatService::bind_peer_at_index(chats, index, peer)
}
