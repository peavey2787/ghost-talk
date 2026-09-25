use ghost_api::PeerRouteRegistration;
use serde_json::Value;

use super::super::{
    support::util::{required, required_str, to_value},
    HYDRA_RUNTIMES,
};
use super::session::{self, SessionState};

pub(in crate::native::browser_host) fn register_routes(args: &Value) -> Result<Value, String> {
    let profile = required_str(args, "profileId")?;
    let identity = required_str(args, "identityId")?;
    let routes: Vec<PeerRouteRegistration> = required(args, "routes")?;
    session::register_routes(profile, identity, routes)?;
    Ok(Value::Null)
}

pub(in crate::native::browser_host) fn binding(args: &Value) -> Result<Value, String> {
    to_value(session::binding_projection(
        required_str(args, "profileId")?,
        required_str(args, "contactId")?,
    )?)
}

pub(in crate::native::browser_host) fn leave(args: &Value) -> Result<Value, String> {
    let profile = required_str(args, "profileId")?;
    let peer = required_str(args, "contactId")?;
    let expected = args.get("expectedSessionSid").and_then(Value::as_str);
    if let Some(expected) = expected {
        let current = session::with(profile, |runtime| {
            Ok(runtime
                .sessions
                .get(peer)
                .map(|binding| binding.sid.clone())
                .or_else(|| runtime.routes.get(peer).and_then(|route| route.session_sid.clone())))
        })?;
        if current.as_deref() != Some(expected) {
            return Err("stale chat leave no longer matches the active KKTP session".into());
        }
    }
    with_hydra_runtime(profile, |hydra| {
        match hydra.session_status(peer)?.as_str() {
            "active" => hydra.close_session(peer)?,
            "pending" => hydra.abort_handshake(peer)?,
            _ => {}
        }
        Ok(())
    })?;
    session::with_mut(profile, |runtime| {
        runtime.sessions.remove(peer);
        runtime.pending_outbound.remove(peer);
        runtime.blocked.insert(peer.to_owned());
        Ok(())
    })?;
    Ok(Value::Null)
}

pub(in crate::native::browser_host) fn rejoin(args: &Value) -> Result<Value, String> {
    let profile = required_str(args, "profileId")?;
    let peer = required_str(args, "contactId")?;
    session::with_mut(profile, |runtime| {
        runtime.blocked.remove(peer);
        runtime.sessions.remove(peer);
        runtime.pending_outbound.remove(peer);
        Ok(())
    })?;
    Ok(Value::Null)
}

pub(in crate::native::browser_host) fn require_active_binding(
    profile: &str,
    peer: &str,
    sid: &str,
) -> Result<(), String> {
    session::with(profile, |runtime| {
        let binding = runtime
            .sessions
            .get(peer)
            .ok_or_else(|| "realtime transport is not active for this peer".to_string())?;
        if binding.sid != sid || binding.state != SessionState::Active {
            return Err("realtime transport is closed for this peer".into());
        }
        if runtime.blocked.contains(peer) {
            return Err("realtime transport is closed for this peer".into());
        }
        Ok(())
    })
}

pub(in crate::native::browser_host) fn with_hydra_runtime<R>(
    profile: &str,
    f: impl FnOnce(&mut ghost_hydra::HydraFacade) -> Result<R, String>,
) -> Result<R, String> {
    HYDRA_RUNTIMES.with(|runtimes| {
        let mut runtimes = runtimes.borrow_mut();
        let hydra = runtimes.get_mut(profile).ok_or_else(|| {
            "HYDRA profile must be unlocked before using secure transport".to_string()
        })?;
        f(hydra)
    })
}
