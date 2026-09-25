use super::{OutgoingPlan, SendOutcome};
use crate::model::{MailboxSendResult, Profile};

pub(super) fn apply_success(
    before: &Profile,
    plan: &mut OutgoingPlan,
    sent: MailboxSendResult,
) -> SendOutcome {
    crate::model::WalletStateService::merge_progress(&mut plan.current.wallet, sent.public.clone());
    update_sent_message(plan, &sent);
    SendOutcome {
        patch: super::super::chat_wallet_transition(before, &plan.current, &plan.chat.id),
        status: if plan.needs_request {
            "Chat request sent. The first message will deliver after acceptance.".into()
        } else if sent.pending_handshake {
            "Re-establishing the encrypted HYDRA session. Your message is attached to the handshake and will deliver when it completes.".into()
        } else {
            "Encrypted message submitted to the Kaspa carrier.".into()
        },
    }
}

fn update_sent_message(plan: &mut OutgoingPlan, sent: &MailboxSendResult) {
    if plan.needs_request {
        let _ = ghost_chat::HydraSessionManager::begin(
            &mut plan.current.chats,
            &plan.chat.id,
            plan.request_id.clone(),
            Some("initiator".into()),
        );
    } else if plan.recover_existing {
        let _ =
            ghost_chat::HydraSessionManager::clear_restore(&mut plan.current.chats, &plan.chat.id);
    }
    let txid =
        (!sent.pending_handshake || plan.needs_request).then_some(sent.transaction_id.clone());
    let stage = if plan.needs_request {
        Some("request".into())
    } else if sent.pending_handshake {
        Some("handshake".into())
    } else {
        None
    };
    let _ = ghost_chat::ChatService::apply_message_send_result(
        &mut plan.current.chats,
        &plan.chat.id,
        &plan.wire_id,
        txid,
        plan.needs_request || sent.pending_handshake,
        sent.pending_id.clone(),
        stage,
    );
}

pub(super) fn apply_failure(
    before: &Profile,
    plan: &mut OutgoingPlan,
    error: String,
) -> SendOutcome {
    let restore_still_in_flight = plan.recover_existing && matches!(
        error.as_str(),
        "The secure-session FINISH is still awaiting the peer's signed acknowledgement; wait for handshake completion before sending another message"
            | "The secure-session recovery FINISH is still awaiting Kaspa broadcast; wait for handshake recovery before sending another message"
            | "A secure-session handshake is already pending for this peer; retry after it completes"
            | "The secure session is still being established for a previous message"
    );
    if restore_still_in_flight {
        let _ = ghost_chat::ChatService::mark_message_queued_handshake(
            &mut plan.current.chats,
            &plan.chat.id,
            &plan.wire_id,
        );
    } else {
        let _ = ghost_chat::ChatService::mark_message_failed(
            &mut plan.current.chats,
            &plan.chat.id,
            &plan.wire_id,
            error.clone(),
        );
    }
    SendOutcome {
        patch: super::super::chat_profile_transition(before, &plan.current, &plan.chat.id),
        status: if restore_still_in_flight {
            "Message queued. The encrypted HYDRA transport is finishing restart recovery and will send this automatically when ready.".into()
        } else {
            error
        },
    }
}
