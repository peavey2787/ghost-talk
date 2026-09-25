use crate::app::state::native;
use crate::model::{ChatStore, MailboxSendResult, Profile};
struct QueuedMessageJob {
    chat_index: usize,
    message_index: usize,
    body: String,
    message_id: String,
    destination: String,
}

fn queued_message_jobs(profile: &Profile, peer: &str) -> Vec<QueuedMessageJob> {
    let mut jobs = Vec::new();
    for (chat_index, chat) in profile.chats.iter().enumerate() {
        if chat.peer_hydra_handle() != Some(peer) || !chat.bootstrap_complete() {
            continue;
        }
        for (message_index, message) in chat.messages().iter().enumerate() {
            if message.direction != "out"
                || !message.pending
                || message.pending_id.is_some()
                || message.pending_stage.as_deref() != Some("handshake")
            {
                continue;
            }
            jobs.push(QueuedMessageJob {
                chat_index,
                message_index,
                body: message.body.clone(),
                message_id: message
                    .wire_id
                    .clone()
                    .unwrap_or_else(|| message.id.clone()),
                destination: chat
                    .peer_kaspa_address()
                    .map(str::to_owned)
                    .unwrap_or_default(),
            });
        }
    }
    jobs
}

pub(super) async fn flush_queued_messages(
    mut profile: Profile,
    password: &str,
    peer: &str,
) -> Result<Profile, String> {
    for job in queued_message_jobs(&profile, peer) {
        let sent = native::send_mailbox_message(
            &profile,
            password,
            peer,
            &job.destination,
            &job.body,
            &job.message_id,
        )
        .await?;
        super::super::ApplicationRouter::route_wallet(
            &mut profile.wallet,
            super::super::ApplicationEvent::WalletUpdated {
                projection: sent.public.clone(),
            },
        );
        apply_queued_send_result(&mut profile.chats, &job, sent);
    }
    Ok(profile)
}

fn apply_queued_send_result(
    chats: &mut ChatStore,
    job: &QueuedMessageJob,
    sent: MailboxSendResult,
) {
    let txid = (!sent.pending_handshake).then_some(sent.transaction_id);
    let _ = ghost_chat::ChatService::apply_message_send_result_at(
        chats,
        job.chat_index,
        job.message_index,
        txid,
        sent.pending_handshake,
        sent.pending_id,
        sent.pending_handshake.then_some("handshake".into()),
    );
}
