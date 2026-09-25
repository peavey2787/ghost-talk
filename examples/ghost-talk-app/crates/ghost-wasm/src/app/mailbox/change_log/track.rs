use ghost_domain::identity::PeerBinding;

use super::track_helpers::{
    contact_acceptance_chat_ids, delivery_chat_matches, direct_received_chat_matches,
    peer_chat_ids, reusable_request_chat, room_contains_accepted_peer, room_transport_chat_ids,
    room_wire_id, same_request,
};

use super::MailboxChangeLog;
use crate::model::{
    HydraContactAcceptedProjection, HydraIncomingRequestProjection, HydraMailboxResult, Profile,
    ReceivedProjection, RoomWire,
};

mod acceptance;

impl MailboxChangeLog {
    /// Declare every pre-existing record one decoded mailbox result may mutate.
    pub(crate) fn track_result(&mut self, profile: &Profile, result: &HydraMailboxResult) {
        self.track_wallet(profile);
        self.track_call_signal(profile, result);
        self.track_incoming_request(profile, result.incoming_request());
        self.track_contact_acceptance_result(profile, result);
        self.track_recovery_and_control(profile, result);
        self.track_received_and_delivery(profile, result);
        self.track_session_transitions(profile, result);
    }

    /// Record creations that are a direct consequence of this decoded envelope.
    /// This is intentionally event-scoped: persistence never scans the whole
    /// aggregate to infer which records appeared.
    pub(crate) fn track_created_records(&mut self, profile: &Profile, result: &HydraMailboxResult) {
        self.track_created_request_chat(profile, result);
        self.track_created_direct_chat(profile, result);
    }

    fn track_created_request_chat(&mut self, profile: &Profile, result: &HydraMailboxResult) {
        let Some(incoming) = result.incoming_request() else {
            return;
        };
        let Some(chat) = profile.chats.iter().find(|chat| {
            chat.incoming_request().is_some_and(|request| {
                request.request_id == incoming.request_id
                    && request.peer_hydra_id == incoming.peer_hydra_id
                    && request
                        .peer_address
                        .eq_ignore_ascii_case(&incoming.peer_address)
            })
        }) else {
            return;
        };
        if !self.chats.contains_key(&chat.id) {
            self.track_new_chat(&chat.id);
        }
    }

    fn track_created_direct_chat(&mut self, profile: &Profile, result: &HydraMailboxResult) {
        let Some(received) = result.received.as_ref() else {
            return;
        };
        if crate::controllers::room::decode_room_wire(&received.plaintext).is_some() {
            return;
        }
        let Some(sid) = received.session_sid() else {
            return;
        };
        let Some(chat) = profile.chats.iter().find(|chat| {
            !chat.room_transport_only()
                && chat.peer_hydra_handle() == Some(received.from.as_str())
                && chat.session_sid() == Some(sid)
        }) else {
            return;
        };
        if !self.chats.contains_key(&chat.id) {
            self.track_new_chat(&chat.id);
        }
    }

    fn track_call_signal(&mut self, profile: &Profile, result: &HydraMailboxResult) {
        let Some(signal) = result.call_signal.as_ref() else {
            return;
        };
        if !profile.has_seen_call_signal(&signal.signal_id) {
            self.record_seen_call_signal(&signal.signal_id);
        }
    }

    fn track_incoming_request(
        &mut self,
        profile: &Profile,
        incoming: Option<&HydraIncomingRequestProjection>,
    ) {
        let Some(incoming) = incoming else {
            return;
        };
        let binding = PeerBinding::new(
            incoming.peer_address.clone(),
            incoming.peer_hydra_id.clone(),
        )
        .ok();
        let ids = profile
            .chats
            .iter()
            .filter(|chat| {
                same_request(chat, incoming) || reusable_request_chat(chat, binding.as_ref())
            })
            .map(|chat| chat.id.clone())
            .collect::<Vec<_>>();
        self.track_chats(profile, ids);
    }

    fn track_contact_acceptance_result(&mut self, profile: &Profile, result: &HydraMailboxResult) {
        if let Some(accepted) = result.contact_accepted.as_ref() {
            self.track_contact_acceptance(profile, accepted);
        }
    }

    fn track_recovery_and_control(&mut self, profile: &Profile, result: &HydraMailboxResult) {
        if let Some(recovery) = result.recovery.as_ref() {
            self.track_chats(profile, peer_chat_ids(profile, &recovery.peer_hydra_id));
        }
        let Some(pending_id) = result
            .control
            .as_ref()
            .and_then(|control| control.completes_pending_id.as_deref())
        else {
            return;
        };
        let ids = profile
            .chats
            .iter()
            .filter(|chat| {
                chat.messages()
                    .iter()
                    .any(|message| message.pending_id.as_deref() == Some(pending_id))
            })
            .map(|chat| chat.id.clone())
            .collect::<Vec<_>>();
        self.track_chats(profile, ids);
    }

    fn track_received_and_delivery(&mut self, profile: &Profile, result: &HydraMailboxResult) {
        if let Some(received) = result.received.as_ref() {
            self.track_received(profile, result, received);
        }
        let (Some(message_id), Some(peer)) = (
            result.delivery_ack.as_deref(),
            result.delivery_ack_peer.as_deref(),
        ) else {
            return;
        };
        let ids = profile
            .chats
            .iter()
            .filter(|chat| delivery_chat_matches(chat, peer, message_id))
            .map(|chat| chat.id.clone())
            .collect::<Vec<_>>();
        self.track_chats(profile, ids);
    }

    fn track_session_transitions(&mut self, profile: &Profile, result: &HydraMailboxResult) {
        if let Some(peer) = result.session_established_peer.as_deref() {
            self.track_chats(profile, peer_chat_ids(profile, peer));
        }
        let Some(ended) = result.session_ended.as_ref() else {
            return;
        };
        let ids = profile
            .chats
            .iter()
            .filter(|chat| {
                chat.peer_hydra_handle() == Some(ended.peer_hydra_id.as_str())
                    && chat.session_sid() == Some(ended.sid.as_str())
            })
            .map(|chat| chat.id.clone())
            .collect::<Vec<_>>();
        self.track_chats(profile, ids);
    }

    fn track_received(
        &mut self,
        profile: &Profile,
        result: &HydraMailboxResult,
        received: &ReceivedProjection,
    ) {
        if let Some(room_wire) = crate::controllers::room::decode_room_wire(&received.plaintext) {
            self.track_room_wire(profile, &received.from, &room_wire);
        } else {
            self.track_direct_received(profile, result, received);
        }
    }

    fn track_direct_received(
        &mut self,
        profile: &Profile,
        result: &HydraMailboxResult,
        received: &ReceivedProjection,
    ) {
        self.track_direct_received_chat(profile, received);
        self.track_direct_received_contact(profile, result, received);
    }

    fn track_direct_received_chat(&mut self, profile: &Profile, received: &ReceivedProjection) {
        let Some(sid) = received.session_sid() else {
            return;
        };
        let Some(chat) = profile
            .chats
            .iter()
            .find(|chat| direct_received_chat_matches(chat, received, sid))
        else {
            return;
        };
        self.track_chat(profile, &chat.id);
        if let Some(contact_id) = chat.contact_id() {
            self.track_contact(profile, contact_id);
        }
    }

    fn track_direct_received_contact(
        &mut self,
        profile: &Profile,
        result: &HydraMailboxResult,
        received: &ReceivedProjection,
    ) {
        let Some(address) = result
            .peer_address
            .as_deref()
            .filter(|value| !value.is_empty())
        else {
            return;
        };
        let Ok(peer) = PeerBinding::new(address.to_string(), received.from.clone()) else {
            return;
        };
        if let Some(contact) =
            ghost_contacts::ContactService::resolve_peer(&profile.contacts, &peer)
        {
            self.track_contact(profile, &contact.id);
        }
    }

    fn track_room_wire(&mut self, profile: &Profile, from: &str, wire: &RoomWire) {
        let room_id = room_wire_id(wire);
        self.track_room(profile, room_id);
        if let Some(room) = profile.rooms.iter().find(|room| room.id == room_id) {
            self.track_tombstone(profile, room_id, &room.owner_hydra_id);
        }
        self.track_room_wire_side_effects(profile, from, wire, room_id);
    }

    fn track_room_wire_side_effects(
        &mut self,
        profile: &Profile,
        from: &str,
        wire: &RoomWire,
        room_id: &str,
    ) {
        match wire {
            RoomWire::State { owner_hydra_id, .. } => {
                self.track_tombstone(profile, room_id, owner_hydra_id);
                self.track_chats(profile, room_transport_chat_ids(profile, from));
            }
            RoomWire::Leave {
                member_hydra_id, ..
            } => {
                self.track_chats(profile, peer_chat_ids(profile, member_hydra_id));
            }
            RoomWire::Kick { .. } | RoomWire::Ban { .. } | RoomWire::Disband { .. } => {
                self.track_chats(profile, peer_chat_ids(profile, from));
            }
            RoomWire::Message { .. } | RoomWire::Reaction { .. } => {}
        }
    }
}
