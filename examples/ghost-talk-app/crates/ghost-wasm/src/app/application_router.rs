use crate::model::{
    CallEvent, CallManager, ChatStore, ContactStore, Message, WalletProjection, WalletRecord,
};
use ghost_rooms::{Room, RoomStore};

/// Typed application-level handoffs between orchestration and domain owners.
/// Events describe intent; the router delegates each mutation to the sole
/// owning service instead of exposing another domain's mutable record.
#[derive(Clone, Debug)]
pub(crate) enum ApplicationEvent {
    ContactVerified {
        contact_id: String,
        verified: bool,
    },
    IncomingMessage {
        chat_id: String,
        message: Message,
    },
    ChatSessionBound {
        chat_id: String,
        sid: String,
        role: String,
    },
    ChatSessionEnded {
        peer_hydra_id: String,
        sid: String,
    },
    RoomMemberRemoved {
        room_id: String,
        peer_hydra_id: String,
    },
    CallEnded {
        call_id: String,
        event: CallEvent,
    },
    WalletUpdated {
        projection: WalletProjection,
    },
    MailboxEnvelopeConsumed {
        packet_id: String,
    },
}

pub(crate) struct ApplicationRouter;

impl ApplicationRouter {
    pub(crate) fn route_contact(contacts: &mut ContactStore, event: ApplicationEvent) -> bool {
        match event {
            ApplicationEvent::ContactVerified {
                contact_id,
                verified,
            } => ghost_contacts::ContactService::mark_verified(contacts, &contact_id, verified),
            _ => false,
        }
    }

    pub(crate) fn route_chat(chats: &mut ChatStore, event: ApplicationEvent) -> bool {
        match event {
            ApplicationEvent::IncomingMessage { chat_id, message } => {
                ghost_chat::ChatService::record_incoming_message(chats, &chat_id, message)
            }
            ApplicationEvent::ChatSessionBound { chat_id, sid, role } => {
                ghost_chat::HydraSessionManager::establish(chats, &chat_id, sid, role)
            }
            ApplicationEvent::ChatSessionEnded { peer_hydra_id, sid } => {
                let targets: Vec<String> = chats
                    .iter()
                    .filter(|chat| {
                        chat.peer_hydra_handle() == Some(peer_hydra_id.as_str())
                            && chat.session_sid() == Some(sid.as_str())
                    })
                    .map(|chat| chat.id.clone())
                    .collect();
                let changed = !targets.is_empty();
                for chat_id in targets {
                    let _ = ghost_chat::ChatService::mark_peer_left(chats, &chat_id);
                }
                changed
            }
            _ => false,
        }
    }

    pub(crate) fn route_room(rooms: &mut RoomStore, event: ApplicationEvent) -> Option<Room> {
        match event {
            ApplicationEvent::RoomMemberRemoved {
                room_id,
                peer_hydra_id,
            } => ghost_rooms::RoomService::remove_member_by_hydra(rooms, &room_id, &peer_hydra_id),
            _ => None,
        }
    }

    pub(crate) fn route_call(calls: &mut CallManager, event: ApplicationEvent) -> bool {
        let ApplicationEvent::CallEnded { call_id, event } = event else {
            return false;
        };
        if calls.get(&call_id).is_none() {
            return false;
        }
        if calls.event(&call_id, event).is_err() {
            return false;
        }
        calls.event(&call_id, CallEvent::CompleteEnd).is_ok()
    }

    pub(crate) fn route_wallet(wallet: &mut Option<WalletRecord>, event: ApplicationEvent) -> bool {
        match event {
            ApplicationEvent::WalletUpdated { projection } => {
                crate::model::WalletStateService::merge_progress(wallet, projection)
            }
            ApplicationEvent::MailboxEnvelopeConsumed { packet_id } => {
                let existed = wallet
                    .as_ref()
                    .is_some_and(|record| record.mailbox_pending.contains_key(&packet_id));
                crate::model::WalletStateService::remove_mailbox_envelope(wallet, &packet_id);
                existed
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ApplicationEvent, ApplicationRouter};
    use crate::model::{ChatStore, Message};

    #[test]
    fn incoming_message_routes_through_chat_owner() {
        let mut chats = ChatStore::from(vec![ghost_chat::ChatService::new_basic(
            "chat".into(),
            String::new(),
        )]);
        assert!(ApplicationRouter::route_chat(
            &mut chats,
            ApplicationEvent::IncomingMessage {
                chat_id: "chat".into(),
                message: Message {
                    id: "message".into(),
                    body: "hello".into(),
                    ..Default::default()
                },
            },
        ));
        assert_eq!(chats[0].messages().len(), 1);
    }
}
