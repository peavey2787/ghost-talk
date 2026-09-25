use super::{
    control_mailbox_result, HydraMailboxResult, HydraProfileRuntime, KktpRole, KktpSessionBinding,
};

pub(crate) fn valid_initiator_binding(
    runtime: &HydraProfileRuntime,
    peer: &str,
    sid: &str,
) -> Option<KktpSessionBinding> {
    runtime
        .kktp_sessions
        .get(peer)
        .cloned()
        .filter(|binding| binding.sid == sid && binding.role == KktpRole::Initiator)
}

pub(crate) fn replay_prepared_completion(
    runtime: &HydraProfileRuntime,
    peer: &str,
    sid: &str,
) -> Option<HydraMailboxResult> {
    let prepared = runtime.prepared_completion.get(peer)?.clone();
    (prepared.sid == sid).then(|| {
        control_mailbox_result(
            prepared.destination,
            prepared.payloads_hex,
            Some(prepared.pending_id),
            None,
        )
    })
}

pub(crate) fn replay_prepared_recovery(
    runtime: &HydraProfileRuntime,
    peer: &str,
    sid: &str,
) -> Option<HydraMailboxResult> {
    let prepared = runtime.prepared_recovery_finish.get(peer)?.clone();
    (prepared.sid == sid).then(|| {
        control_mailbox_result(
            prepared.destination,
            prepared.payloads_hex,
            None,
            Some(peer.to_owned()),
        )
    })
}
