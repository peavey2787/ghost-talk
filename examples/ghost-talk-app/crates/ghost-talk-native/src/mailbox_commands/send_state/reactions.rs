pub(crate) fn prepare_active_reaction_send(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    reaction: &ghost_protocol::GhostReactionEvent,
    stego_profile: &str,
) -> Result<super::PreparedSend, String> {
    if !super::active_kktp_binding(runtime, contact_id) {
        return Err("Reactions require an established Ghost PQ session".into());
    }
    let body = ghost_protocol::canonical_json(reaction)
        .and_then(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()))?;
    if let Some(prepared) = super::cached_durable_message(
        runtime,
        contact_id,
        message_id,
        "reaction",
        &body,
        stego_profile,
    )? {
        return reaction_prepared_send(runtime, contact_id, prepared);
    }
    super::validate_new_durable_message(runtime, contact_id, message_id)?;
    let prepared = crate::hydra_commands::prepare_kktp_reaction_mailbox(
        runtime,
        contact_id,
        message_id,
        reaction,
        stego_profile,
    )?;
    super::remember_prepared_delivery(
        runtime,
        contact_id,
        message_id,
        "reaction",
        &body,
        stego_profile,
        &prepared,
    );
    super::checkpoint_prepared_delivery(runtime, contact_id, message_id)?;
    reaction_prepared_send(runtime, contact_id, prepared)
}

fn reaction_prepared_send(
    runtime: &mut crate::hydra_commands::HydraProfileRuntime,
    contact_id: &str,
    prepared: crate::hydra_commands::PreparedMailbox,
) -> Result<super::PreparedSend, String> {
    runtime.pending_outbound.remove(contact_id);
    runtime.prepared_completion.remove(contact_id);
    Ok(super::PreparedSend {
        payloads: super::decode_mailbox_payloads(&prepared.payloads_hex)?,
        pending_id: None,
        destination: super::authenticated_destination(
            runtime,
            contact_id,
            "active HYDRA session has no authenticated Kaspa peer route",
        )?,
    })
}
