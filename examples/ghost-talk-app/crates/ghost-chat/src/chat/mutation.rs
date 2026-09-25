use super::{Chat, IncomingRequest, Message, PeerBinding};

impl Chat {
    pub(crate) fn set_archived(&mut self, archived: bool) {
        self.archived = archived;
    }
    pub(crate) fn toggle_archived(&mut self) {
        self.archived = !self.archived;
    }
    pub(crate) fn mark_read(&mut self) {
        self.unread_count = 0;
    }
    pub(crate) fn increment_unread(&mut self) {
        self.unread_count = self.unread_count.saturating_add(1);
    }
    pub(crate) fn set_contact_id(&mut self, contact_id: Option<String>) {
        self.contact_id = contact_id;
    }
    pub(crate) fn set_peer_name_aliases(
        &mut self,
        kns_name: Option<String>,
        dotk_name: Option<String>,
    ) {
        self.peer_kns_name = kns_name;
        self.peer_dotk_name = dotk_name;
    }
    pub(crate) fn set_room_transport_only(&mut self, value: bool) {
        self.room_transport_only = value;
    }
    pub(crate) fn set_session_role(&mut self, role: String) {
        self.session_role = Some(role);
    }
    pub(crate) fn set_incoming_request(&mut self, request: IncomingRequest) {
        self.incoming_request = Some(request);
    }
    pub(crate) fn clear_incoming_request(&mut self) {
        self.incoming_request = None;
    }
    pub(crate) fn set_incoming_request_state(&mut self, state: &str) {
        if let Some(request) = self.incoming_request.as_mut() {
            request.set_state(state);
        }
    }
    pub(crate) fn push_message(&mut self, message: Message) {
        self.messages.push(message);
    }
    pub(crate) fn sort_messages_by_created_at(&mut self) {
        self.messages
            .sort_by(|a, b| a.created_at.total_cmp(&b.created_at));
    }
    pub(crate) fn retain_messages(&mut self, predicate: impl FnMut(&Message) -> bool) {
        self.messages.retain(predicate);
    }
    pub(crate) fn set_peer_binding(&mut self, peer: &PeerBinding) {
        self.peer_kaspa_address = Some(peer.kaspa_address.clone());
        self.peer_hydra_handle = Some(peer.hydra_id.clone());
    }
    pub(crate) fn begin_session(&mut self, sid: Option<String>, role: Option<String>) {
        self.session_sid = sid;
        self.session_role = role;
        self.bootstrap_complete = false;
        self.transport_restore_pending = false;
        self.transport_restore_message_id = None;
    }
    pub(crate) fn establish_session(&mut self, sid: String, role: String) {
        self.session_sid = Some(sid);
        self.session_role = Some(role);
        self.bootstrap_complete = true;
        self.transport_restore_pending = false;
        self.transport_restore_message_id = None;
    }
    pub(crate) fn mark_transport_restore(&mut self, sid: Option<String>, role: Option<String>) {
        self.session_sid = sid;
        self.session_role = role;
        self.bootstrap_complete = false;
        self.transport_restore_pending = true;
        self.transport_restore_message_id = None;
    }
    pub(crate) fn set_transport_restore_message_id(&mut self, id: Option<String>) {
        self.transport_restore_message_id = id;
    }
    pub(crate) fn merge_resumed_transport(&mut self, resumed: &Chat) {
        if resumed.bootstrap_complete && !resumed.transport_restore_pending {
            self.session_sid = resumed.session_sid.clone();
            self.session_role = resumed.session_role.clone();
            self.bootstrap_complete = true;
            self.transport_restore_pending = false;
            self.transport_restore_message_id = None;
        } else if resumed.transport_restore_message_id.is_some() {
            self.set_transport_restore_message_id(resumed.transport_restore_message_id.clone());
        }
    }
    pub(crate) fn clear_transport_restore(&mut self) {
        self.transport_restore_pending = false;
        self.transport_restore_message_id = None;
    }
    pub(crate) fn complete_bootstrap(&mut self, sid: Option<String>) {
        if sid.is_some() {
            self.session_sid = sid;
        }
        self.bootstrap_complete = true;
        self.transport_restore_pending = false;
        self.transport_restore_message_id = None;
    }
    pub(crate) fn reset_transport(&mut self) {
        self.session_sid = None;
        self.session_role = None;
        self.bootstrap_complete = false;
        self.transport_restore_pending = false;
        self.transport_restore_message_id = None;
        self.incoming_request = None;
    }
    pub(crate) fn leave_local(&mut self) {
        self.reset_transport();
        self.left = true;
        self.archived = true;
        self.mark_read();
    }
    pub(crate) fn mark_peer_left(&mut self) {
        self.peer_left = true;
    }
    pub(crate) fn rejoin_with_session(
        &mut self,
        sid: Option<String>,
        role: Option<String>,
        restore_pending: bool,
        restore_message_id: Option<String>,
    ) {
        self.left = false;
        self.peer_left = false;
        self.archived = false;
        self.bootstrap_complete = true;
        self.session_sid = sid;
        self.session_role = role;
        self.transport_restore_pending = restore_pending;
        self.transport_restore_message_id = restore_message_id;
    }
    pub(crate) fn rejoin_without_session(&mut self) {
        self.left = false;
        self.peer_left = false;
        self.archived = false;
        self.session_sid = None;
        self.session_role = None;
        self.bootstrap_complete = false;
        self.transport_restore_pending = false;
        self.transport_restore_message_id = None;
    }
    pub(crate) fn convert_to_hidden_room_transport(&mut self, id: String) {
        self.id = id;
        self.messages.clear();
        self.incoming_request = None;
        self.archived = false;
        self.left = false;
        self.peer_left = false;
        self.room_transport_only = true;
    }
    pub(crate) fn reopen_room_transport(&mut self, label: String, hydra_id: Option<String>) {
        let was_closed = self.left || self.peer_left;
        self.label = label;
        if hydra_id.is_some() {
            self.peer_hydra_handle = hydra_id;
        }
        self.archived = false;
        self.left = false;
        self.peer_left = false;
        if was_closed {
            self.reset_transport();
        }
    }
    pub(crate) fn bind_accepted_identity(
        &mut self,
        contact_id: Option<String>,
        peer: &PeerBinding,
        room_transport_only: bool,
    ) {
        self.contact_id = contact_id;
        self.set_peer_binding(peer);
        self.room_transport_only = room_transport_only;
    }
}
