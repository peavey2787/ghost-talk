use crate::{Chat, ChatStore, IncomingRequest};

/// Sole mutation authority for durable chat and message records.
///
/// Callers may inspect `ChatStore` immutably, but every mutation is expressed
/// as a named domain command so arbitrary mutable references never escape.
pub struct ChatService;

#[derive(Clone, Debug)]
pub struct DirectChatSpec {
    pub id: String,
    pub label: String,
    pub contact_id: Option<String>,
    pub kaspa_address: String,
    pub hydra_handle: Option<String>,
    pub kns_name: Option<String>,
    pub dotk_name: Option<String>,
    pub verified_public: bool,
}

#[derive(Clone, Debug)]
pub struct RestoredChatSpec {
    pub id: String,
    pub label: String,
    pub contact_id: Option<String>,
    pub kaspa_address: Option<String>,
    pub hydra_handle: Option<String>,
    pub kns_name: Option<String>,
    pub dotk_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct IncomingRequestChatSpec {
    pub direct: DirectChatSpec,
    pub request: IncomingRequest,
    pub room_transport_only: bool,
}

#[derive(Clone, Debug)]
pub struct SessionChatSpec {
    pub restored: RestoredChatSpec,
    pub hydra_handle: String,
    pub verified_public: bool,
    pub sid: String,
    pub role: Option<String>,
    pub restore_pending: bool,
}

impl ChatService {
    pub fn by_id<'a>(chats: &'a ChatStore, id: &str) -> Option<&'a Chat> {
        chats.iter().find(|chat| chat.id == id)
    }

    pub fn by_index(chats: &ChatStore, index: usize) -> Option<&Chat> {
        chats.get(index)
    }

    pub fn new_direct_chat(spec: DirectChatSpec) -> Chat {
        Chat {
            id: spec.id,
            label: spec.label,
            contact_id: spec.contact_id,
            peer_kaspa_address: Some(spec.kaspa_address),
            peer_hydra_handle: spec.hydra_handle,
            peer_kns_name: spec.kns_name,
            peer_dotk_name: spec.dotk_name,
            verified_public: spec.verified_public,
            ..Chat::default()
        }
    }

    pub fn new_restored_chat(spec: RestoredChatSpec) -> Chat {
        Chat {
            id: spec.id,
            label: spec.label,
            contact_id: spec.contact_id,
            peer_kaspa_address: spec.kaspa_address,
            peer_hydra_handle: spec.hydra_handle,
            peer_kns_name: spec.kns_name,
            peer_dotk_name: spec.dotk_name,
            ..Chat::default()
        }
    }

    pub fn new_basic(id: String, label: String) -> Chat {
        Chat {
            id,
            label,
            ..Chat::default()
        }
    }

    pub fn new_incoming_request(spec: IncomingRequestChatSpec) -> Chat {
        let sid = spec.request.request_id.clone();
        let mut chat = Self::new_direct_chat(DirectChatSpec {
            verified_public: false,
            ..spec.direct
        });
        chat.begin_session(Some(sid), Some("responder".into()));
        chat.set_incoming_request(spec.request);
        chat.set_room_transport_only(spec.room_transport_only);
        chat
    }

    pub fn new_session_chat(spec: SessionChatSpec) -> Chat {
        let mut chat = Chat {
            id: spec.restored.id,
            label: spec.restored.label,
            contact_id: spec.restored.contact_id,
            peer_kaspa_address: spec.restored.kaspa_address,
            peer_kns_name: spec.restored.kns_name,
            peer_dotk_name: spec.restored.dotk_name,
            peer_hydra_handle: Some(spec.hydra_handle),
            verified_public: spec.verified_public,
            ..Chat::default()
        };
        chat.begin_session(Some(spec.sid), spec.role);
        if !spec.restore_pending {
            chat.complete_bootstrap(None);
        } else {
            chat.mark_transport_restore(
                chat.session_sid().map(str::to_owned),
                chat.session_role().map(str::to_owned),
            );
        }
        chat
    }

    pub fn insert_front(chats: &mut ChatStore, chat: Chat) {
        chats.insert_owned(0, chat);
    }
    pub fn push(chats: &mut ChatStore, chat: Chat) {
        chats.push_owned(chat);
    }
    pub fn remove_by_id(chats: &mut ChatStore, id: &str) -> bool {
        let before = chats.len();
        chats.retain_owned(|chat| chat.id != id);
        chats.len() != before
    }

    pub fn remove_if_unchanged(chats: &mut ChatStore, before: &Chat) -> bool {
        let unchanged = chats
            .iter()
            .any(|current| current.id == before.id && current == before);
        unchanged && Self::remove_by_id(chats, &before.id)
    }
}

mod helpers;
pub(crate) use helpers::chat_by_id_mut;
mod lifecycle;
mod messages;
mod reconcile;
mod requests;
