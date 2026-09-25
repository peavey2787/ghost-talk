use super::ChatService;
use crate::{Chat, ChatStore, Message};
use ghost_domain::reaction::merge_reactions;

impl ChatService {
    /// Reconcile one asynchronous chat result against the current store without
    /// exposing transport/message lifecycle field writes to application code.
    pub fn reconcile_upsert(chats: &mut ChatStore, before: Option<&Chat>, mut changed: Chat) {
        let Some(latest) = chats
            .as_mut_slice()
            .iter_mut()
            .find(|chat| chat.id == changed.id)
        else {
            if before.is_none() {
                chats.push_owned(changed);
            }
            return;
        };
        let Some(before) = before else {
            return;
        };
        preserve_concurrent_fields(before, &mut changed, latest);
        *latest = changed;
    }
}

fn preserve_concurrent_fields(before: &Chat, changed: &mut Chat, latest: &Chat) {
    macro_rules! keep_latest_if_unchanged {
        ($name:ident) => {
            if changed.$name == before.$name && latest.$name != before.$name {
                changed.$name = latest.$name.clone();
            }
        };
    }
    keep_latest_if_unchanged!(label);
    keep_latest_if_unchanged!(contact_id);
    keep_latest_if_unchanged!(peer_kaspa_address);
    keep_latest_if_unchanged!(peer_kns_name);
    keep_latest_if_unchanged!(peer_dotk_name);
    keep_latest_if_unchanged!(peer_hydra_handle);
    keep_latest_if_unchanged!(verified_public);
    keep_latest_if_unchanged!(bootstrap_complete);
    keep_latest_if_unchanged!(archived);
    keep_latest_if_unchanged!(left);
    keep_latest_if_unchanged!(peer_left);
    keep_latest_if_unchanged!(session_sid);
    keep_latest_if_unchanged!(session_role);
    keep_latest_if_unchanged!(incoming_request);
    keep_latest_if_unchanged!(room_transport_only);
    keep_latest_if_unchanged!(unread_count);
    keep_latest_if_unchanged!(transport_restore_pending);
    keep_latest_if_unchanged!(transport_restore_message_id);
    changed.messages = merge_messages(
        &before.messages,
        std::mem::take(&mut changed.messages),
        &latest.messages,
    );
    if changed.left || changed.peer_left {
        changed.bootstrap_complete = false;
        changed.transport_restore_pending = false;
        changed.transport_restore_message_id = None;
    }
}

fn merge_messages(
    before: &[Message],
    mut changed: Vec<Message>,
    latest: &[Message],
) -> Vec<Message> {
    for concurrent in latest {
        if let Some(slot) = changed
            .iter_mut()
            .find(|message| message.id == concurrent.id)
        {
            if let Some(baseline) = before.iter().find(|message| message.id == concurrent.id) {
                merge_message_fields(baseline, slot, concurrent);
            }
        } else {
            changed.push(concurrent.clone());
        }
    }
    changed.sort_by(|left, right| left.created_at.total_cmp(&right.created_at));
    changed
}

fn merge_message_fields(baseline: &Message, changed: &mut Message, latest: &Message) {
    macro_rules! keep_latest_if_unchanged {
        ($field:ident) => {
            if changed.$field == baseline.$field && latest.$field != baseline.$field {
                changed.$field = latest.$field.clone();
            }
        };
    }
    keep_latest_if_unchanged!(wire_id);
    keep_latest_if_unchanged!(session_sid);
    keep_latest_if_unchanged!(body);
    keep_latest_if_unchanged!(txid);
    keep_latest_if_unchanged!(send_state);
    keep_latest_if_unchanged!(send_error);
    keep_latest_if_unchanged!(pending);
    keep_latest_if_unchanged!(pending_id);
    keep_latest_if_unchanged!(pending_stage);
    keep_latest_if_unchanged!(contact_request_id);
    changed.reactions = merge_reactions(&baseline.reactions, &changed.reactions, &latest.reactions);
}
