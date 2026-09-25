use ghost_domain::reaction::{actor_reaction, set_actor_reaction, MessageReaction, ReactionKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    #[serde(default)]
    pub wire_id: Option<String>,
    #[serde(default)]
    pub session_sid: Option<String>,
    pub direction: String,
    pub body: String,
    pub created_at: f64,
    #[serde(default)]
    pub txid: Option<String>,
    #[serde(default)]
    pub send_state: Option<String>,
    #[serde(default)]
    pub send_error: Option<String>,
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub pending_id: Option<String>,
    #[serde(default)]
    pub pending_stage: Option<String>,
    #[serde(default)]
    pub contact_request_id: Option<String>,
    #[serde(default)]
    pub reactions: Vec<MessageReaction>,
}

impl Message {
    pub fn session_sid(&self) -> Option<&str> {
        self.session_sid.as_deref()
    }

    pub fn mark_sending(&mut self) {
        self.send_state = Some("sending".into());
        self.send_error = None;
    }

    pub fn mark_delivered(&mut self) {
        self.pending = false;
        self.pending_id = None;
        self.pending_stage = None;
        self.send_state = Some("delivered".into());
        self.send_error = None;
    }

    pub fn mark_queued_handshake(&mut self) {
        self.pending = true;
        self.pending_id = None;
        self.pending_stage = Some("handshake".into());
        self.send_state = Some("queued".into());
        self.send_error = None;
    }

    pub fn mark_failed(&mut self, error: String) {
        self.pending = false;
        self.pending_id = None;
        self.pending_stage = None;
        self.send_state = Some("failed".into());
        self.send_error = Some(error);
    }

    pub fn apply_send_result(
        &mut self,
        transaction_id: Option<String>,
        pending: bool,
        pending_id: Option<String>,
        pending_stage: Option<String>,
    ) {
        self.pending = pending;
        self.pending_id = pending_id;
        self.pending_stage = pending_stage;
        self.send_state = Some("sent".into());
        self.send_error = None;
        if transaction_id.is_some() {
            self.txid = transaction_id;
        }
    }

    pub fn set_pending_stage(&mut self, stage: Option<String>) {
        self.pending_stage = stage;
    }
    pub fn clear_contact_request_id(&mut self) {
        self.contact_request_id = None;
    }

    pub fn mark_finish_sent(&mut self, transaction_id: String) {
        self.pending_stage = Some("finish".into());
        self.txid = Some(transaction_id);
        self.send_state = Some("sent".into());
    }

    pub fn mark_sent(&mut self, transaction_id: String) {
        self.txid = Some(transaction_id);
        self.send_state = Some("sent".into());
        self.send_error = None;
    }

    pub fn replace_body(&mut self, body: String) {
        self.body = body;
    }

    pub fn set_reaction(&mut self, actor_id: &str, kind: Option<ReactionKind>) -> bool {
        set_actor_reaction(&mut self.reactions, actor_id, kind)
    }

    pub fn reaction_for(&self, actor_id: &str) -> Option<ReactionKind> {
        actor_reaction(&self.reactions, actor_id)
    }

    pub fn clear_pending_id_for_restore(&mut self) {
        self.pending_id = None;
        self.send_state = Some("queued".into());
    }
}
