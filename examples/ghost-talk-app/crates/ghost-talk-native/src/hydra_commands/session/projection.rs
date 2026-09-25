use super::super::{
    runtime_state::HydraProfileRuntime,
    session_types::{KktpInnerMessage, KktpSessionState, ReceivedProjection},
};

pub(super) fn close_inner_session(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
) -> Result<(), String> {
    if runtime.hydra.session_status(peer_hydra_id)? == "active" {
        runtime.hydra.close_session(peer_hydra_id)?;
    }
    if let Some(current) = runtime.kktp_sessions.get_mut(peer_hydra_id) {
        current.state = KktpSessionState::Closed;
    }
    Ok(())
}

pub(super) fn project_kktp_inner(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
    sid: &str,
    inner: KktpInnerMessage,
) -> Result<Option<ReceivedProjection>, String> {
    let kind = inner.kind.clone();
    match kind.as_str() {
        "msg" | "reaction" => project_kktp_content(peer_hydra_id, sid, inner),
        "session_end" => {
            close_inner_session(runtime, peer_hydra_id)?;
            Ok(None)
        }
        _ => Err("unknown authenticated KKTP inner message type".into()),
    }
}

fn project_kktp_content(
    peer_hydra_id: &str,
    sid: &str,
    inner: KktpInnerMessage,
) -> Result<Option<ReceivedProjection>, String> {
    let kind = inner.kind.clone();
    match kind.as_str() {
        "msg" => Ok(Some(ReceivedProjection {
            from: peer_hydra_id.to_owned(),
            plaintext: inner.body,
            content_type: None,
            session_sid: Some(sid.to_owned()),
        })),
        "reaction" => project_kktp_reaction(peer_hydra_id, sid, inner),
        _ => Err("unknown authenticated KKTP content type".into()),
    }
}

fn project_kktp_reaction(
    peer_hydra_id: &str,
    sid: &str,
    inner: KktpInnerMessage,
) -> Result<Option<ReceivedProjection>, String> {
    let reaction = inner
        .reaction
        .ok_or_else(|| "authenticated KKTP reaction metadata is missing".to_string())?;
    Ok(Some(ReceivedProjection {
        from: peer_hydra_id.to_owned(),
        plaintext: serde_json::to_string(&reaction).map_err(|error| error.to_string())?,
        content_type: Some("ghost-reaction-v1".into()),
        session_sid: Some(sid.to_owned()),
    }))
}
