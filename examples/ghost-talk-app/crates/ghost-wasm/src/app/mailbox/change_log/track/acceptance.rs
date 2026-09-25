use super::{
    contact_acceptance_chat_ids, room_contains_accepted_peer, HydraContactAcceptedProjection,
    MailboxChangeLog, Profile,
};
use ghost_domain::identity::PeerBinding;

impl MailboxChangeLog {
    pub(super) fn track_contact_acceptance(
        &mut self,
        profile: &Profile,
        accepted: &HydraContactAcceptedProjection,
    ) {
        let target_chat_ids = contact_acceptance_chat_ids(profile, &accepted.request_id);
        self.track_acceptance_chats_and_contacts(profile, &target_chat_ids);
        self.track_acceptance_peer_contact(profile, accepted);
        self.track_acceptance_rooms(profile, accepted, &target_chat_ids);
    }

    fn track_acceptance_chats_and_contacts(&mut self, profile: &Profile, chat_ids: &[String]) {
        for chat_id in chat_ids {
            self.track_chat(profile, chat_id);
            let contact_id = profile
                .chats
                .iter()
                .find(|chat| chat.id == *chat_id)
                .and_then(|chat| chat.contact_id());
            if let Some(contact_id) = contact_id {
                self.track_contact(profile, contact_id);
            }
        }
    }

    fn track_acceptance_peer_contact(
        &mut self,
        profile: &Profile,
        accepted: &HydraContactAcceptedProjection,
    ) {
        let Ok(peer) = PeerBinding::new(
            accepted.peer_address.clone(),
            accepted.peer_hydra_id.clone(),
        ) else {
            return;
        };
        if let Some(contact) =
            ghost_contacts::ContactService::resolve_peer(&profile.contacts, &peer)
        {
            self.track_contact(profile, &contact.id);
        }
    }

    fn track_acceptance_rooms(
        &mut self,
        profile: &Profile,
        accepted: &HydraContactAcceptedProjection,
        chat_ids: &[String],
    ) {
        let prior_addresses = chat_ids
            .iter()
            .filter_map(|chat_id| {
                profile
                    .chats
                    .iter()
                    .find(|chat| chat.id == *chat_id)
                    .and_then(|chat| chat.peer_kaspa_address().map(str::to_owned))
            })
            .collect::<Vec<_>>();
        let accepted_peer = PeerBinding::new(
            accepted.peer_address.clone(),
            accepted.peer_hydra_id.clone(),
        )
        .ok();
        let room_ids = profile
            .rooms
            .iter()
            .filter(|room| {
                room_contains_accepted_peer(room, &prior_addresses, accepted_peer.as_ref())
            })
            .map(|room| room.id.clone())
            .collect::<Vec<_>>();
        for room_id in room_ids {
            self.track_room(profile, &room_id);
        }
    }
}
