use crate::model::{Chat, Message, Profile, ProfilePatch, ReactionKind};

pub(crate) struct ReactionPlan {
    before: Profile,
    working: Profile,
    chat_id: String,
    event: ghost_protocol::GhostReactionEvent,
    transport_message_id: String,
    peer_hydra_id: String,
    destination: String,
}

pub(crate) fn prepare_reaction(
    profile: &Profile,
    chat: &Chat,
    message: &Message,
    selected: ReactionKind,
) -> Result<(ProfilePatch, ReactionPlan), String> {
    let context = reaction_context(profile, chat, message, selected)?;
    let mut working = profile.clone();
    apply_local_reaction(&mut working.chats, chat, message, &context)?;
    let patch = super::chat_profile_transition(profile, &working, &chat.id);
    Ok((patch, build_plan(profile, working, chat, context)?))
}

struct ReactionContext {
    actor_id: String,
    target_message_id: String,
    next: Option<ReactionKind>,
}

fn reaction_context(
    profile: &Profile,
    chat: &Chat,
    message: &Message,
    selected: ReactionKind,
) -> Result<ReactionContext, String> {
    if !chat.bootstrap_complete() {
        return Err("Reactions require an established Ghost PQ session.".into());
    }
    let actor_id = profile
        .hydra_identity_id
        .clone()
        .ok_or_else(|| "HYDRA identity is unavailable.".to_string())?;
    let target_message_id = message.wire_id.clone().ok_or_else(|| {
        "This message has not received a shared Ghost wire identifier yet.".to_string()
    })?;
    ghost_protocol::validate_kktp_message_id(&target_message_id).map_err(|_| {
        "This message does not have a reaction-capable Ghost wire identifier.".to_string()
    })?;
    let next = (message.reaction_for(&actor_id) != Some(selected)).then_some(selected);
    Ok(ReactionContext {
        actor_id,
        target_message_id,
        next,
    })
}

fn apply_local_reaction(
    chats: &mut ghost_chat::ChatStore,
    chat: &Chat,
    message: &Message,
    context: &ReactionContext,
) -> Result<(), String> {
    ghost_chat::ChatService::set_message_reaction(
        chats,
        &chat.id,
        &message.id,
        &context.actor_id,
        context.next,
    )
    .then_some(())
    .ok_or_else(|| "Reaction state did not change.".to_string())
}

fn build_plan(
    before: &Profile,
    working: Profile,
    chat: &Chat,
    context: ReactionContext,
) -> Result<ReactionPlan, String> {
    Ok(ReactionPlan {
        before: before.clone(),
        working,
        chat_id: chat.id.clone(),
        event: ghost_protocol::GhostReactionEvent {
            target_message_id: context.target_message_id,
            reaction: context.next,
        },
        transport_message_id: crate::random_id()?,
        peer_hydra_id: chat
            .peer_hydra_handle()
            .map(str::to_owned)
            .ok_or_else(|| "This chat has no HYDRA peer route.".to_string())?,
        destination: chat
            .peer_kaspa_address()
            .map(str::to_owned)
            .ok_or_else(|| "This chat has no Kaspa destination.".to_string())?,
    })
}

pub(crate) async fn send_reaction(
    mut plan: ReactionPlan,
    password: &str,
) -> Result<ProfilePatch, (ProfilePatch, String)> {
    match crate::native::send_mailbox_reaction(
        &plan.working,
        password,
        &plan.peer_hydra_id,
        &plan.destination,
        &plan.event,
        &plan.transport_message_id,
    )
    .await
    {
        Ok(sent) => {
            let before = plan.working.clone();
            crate::model::WalletStateService::merge_progress(&mut plan.working.wallet, sent.public);
            let mut patch = ProfilePatch::new(&before.id);
            patch.wallet(before.wallet, plan.working.wallet);
            Ok(patch)
        }
        Err(error) => {
            let rollback =
                super::chat_profile_transition(&plan.working, &plan.before, &plan.chat_id);
            Err((rollback, error))
        }
    }
}

#[cfg(test)]
mod tests {
    use ghost_protocol::validate_kktp_message_id;

    #[test]
    fn reaction_targets_require_wire_message_ids() {
        assert!(validate_kktp_message_id(&"ab".repeat(16)).is_ok());
        assert!(validate_kktp_message_id("message").is_err());
    }
}
