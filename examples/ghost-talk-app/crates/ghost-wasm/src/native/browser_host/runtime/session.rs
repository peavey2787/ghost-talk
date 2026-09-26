use ghost_api::{HydraSessionBindingProjection, PeerRouteRegistration};
use ghost_protocol::{kktp_mailbox_id, KktpDirection};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

mod debug;

pub(in crate::native::browser_host) use debug::debug_value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::native::browser_host) enum Role {
    Initiator,
    Responder,
}
impl Role {
    pub(in crate::native::browser_host) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "initiator" => Ok(Self::Initiator),
            "responder" => Ok(Self::Responder),
            _ => Err("unknown KKTP session role".into()),
        }
    }
    pub(in crate::native::browser_host) fn label(self) -> &'static str {
        match self {
            Self::Initiator => "initiator",
            Self::Responder => "responder",
        }
    }
    pub(in crate::native::browser_host) fn outbound(self) -> KktpDirection {
        match self {
            Self::Initiator => KktpDirection::AtoB,
            Self::Responder => KktpDirection::BtoA,
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::native::browser_host) struct Route {
    pub(in crate::native::browser_host) kaspa_address: String,
    pub(in crate::native::browser_host) display_name: String,
    pub(in crate::native::browser_host) session_sid: Option<String>,
    pub(in crate::native::browser_host) session_role: Option<Role>,
    pub(in crate::native::browser_host) resume_required: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::native::browser_host) enum SessionState {
    Discovered,
    Handshake,
    Active,
}

#[derive(Clone, Debug)]
pub(in crate::native::browser_host) struct Binding {
    pub(in crate::native::browser_host) sid: String,
    pub(in crate::native::browser_host) role: Role,
    pub(in crate::native::browser_host) peer: String,
    pub(in crate::native::browser_host) mailbox_id: String,
    pub(in crate::native::browser_host) send_seq: u64,
    pub(in crate::native::browser_host) recv_next_seq: u64,
    pub(in crate::native::browser_host) state: SessionState,
}
impl Binding {
    pub(in crate::native::browser_host) fn inbound(&self) -> KktpDirection {
        self.role.outbound().opposite()
    }
}

#[derive(Clone, Debug)]
pub(in crate::native::browser_host) struct PreparedCompletion {
    pub(in crate::native::browser_host) destination: String,
    pub(in crate::native::browser_host) message_id: String,
    pub(in crate::native::browser_host) payloads_hex: Vec<String>,
}

#[derive(Clone, Debug)]
pub(in crate::native::browser_host) struct PendingSend {
    pub(in crate::native::browser_host) id: String,
    pub(in crate::native::browser_host) sid: String,
    pub(in crate::native::browser_host) destination: String,
    pub(in crate::native::browser_host) body: String,
    pub(in crate::native::browser_host) message_id: String,
    pub(in crate::native::browser_host) stego_profile: String,
}

#[derive(Default)]
pub(in crate::native::browser_host) struct BrowserTransport {
    pub(in crate::native::browser_host) identity_id: String,
    pub(in crate::native::browser_host) routes: HashMap<String, Route>,
    pub(in crate::native::browser_host) sessions: HashMap<String, Binding>,
    pub(in crate::native::browser_host) pending_contact_sids: HashSet<String>,
    pub(in crate::native::browser_host) pending_outbound: HashMap<String, PendingSend>,
    pub(in crate::native::browser_host) prepared_completion: HashMap<String, PreparedCompletion>,
    pub(in crate::native::browser_host) blocked: HashSet<String>,
}

thread_local! {
    static TRANSPORTS: RefCell<HashMap<String, BrowserTransport>> = RefCell::new(HashMap::new());
}

pub(in crate::native::browser_host) fn set_identity(profile_id: &str, identity_id: &str) {
    TRANSPORTS.with(|all| {
        let mut all = all.borrow_mut();
        let runtime = all.entry(profile_id.to_owned()).or_default();
        if runtime.identity_id != identity_id {
            runtime.identity_id = identity_id.to_owned();
            runtime.sessions.clear();
            runtime.pending_outbound.clear();
            runtime.prepared_completion.clear();
        }
    });
}

pub(in crate::native::browser_host) fn with<R>(
    profile_id: &str,
    f: impl FnOnce(&BrowserTransport) -> Result<R, String>,
) -> Result<R, String> {
    TRANSPORTS.with(|all| {
        let all = all.borrow();
        let runtime = all
            .get(profile_id)
            .ok_or_else(|| "browser HYDRA transport is not initialized".to_string())?;
        f(runtime)
    })
}

pub(in crate::native::browser_host) fn with_mut<R>(
    profile_id: &str,
    f: impl FnOnce(&mut BrowserTransport) -> Result<R, String>,
) -> Result<R, String> {
    TRANSPORTS.with(|all| {
        let mut all = all.borrow_mut();
        let runtime = all
            .get_mut(profile_id)
            .ok_or_else(|| "browser HYDRA transport is not initialized".to_string())?;
        f(runtime)
    })
}

pub(in crate::native::browser_host) fn register_routes(
    profile_id: &str,
    identity_id: &str,
    routes: Vec<PeerRouteRegistration>,
) -> Result<(), String> {
    set_identity(profile_id, identity_id);
    with_mut(profile_id, |runtime| {
        runtime.routes.clear();
        for route in routes {
            let role = route.session_role.as_deref().map(Role::parse).transpose()?;
            runtime.routes.insert(
                route.contact_id,
                Route {
                    kaspa_address: route.kaspa_address,
                    display_name: route.display_name,
                    session_sid: route.session_sid,
                    session_role: role,
                    resume_required: route.resume_required,
                },
            );
        }
        Ok(())
    })
}

pub(in crate::native::browser_host) fn binding_projection(
    profile_id: &str,
    peer: &str,
) -> Result<Option<HydraSessionBindingProjection>, String> {
    with(profile_id, |runtime| {
        Ok(runtime
            .sessions
            .get(peer)
            .filter(|binding| binding.state == SessionState::Active)
            .map(|binding| HydraSessionBindingProjection {
                sid: binding.sid.clone(),
                role: binding.role.label().into(),
                restart_resumable: false,
            }))
    })
}

pub(in crate::native::browser_host) fn install(
    profile_id: &str,
    peer: &str,
    sid: String,
    role: Role,
    state: SessionState,
) -> Result<Binding, String> {
    with_mut(profile_id, |runtime| {
        let local = runtime.identity_id.clone();
        let (initiator, responder) = match role {
            Role::Initiator => (local, peer.to_owned()),
            Role::Responder => (peer.to_owned(), local),
        };
        let binding = Binding {
            mailbox_id: kktp_mailbox_id(&sid, &initiator, &responder)?,
            sid,
            role,
            peer: peer.to_owned(),
            send_seq: 0,
            recv_next_seq: 0,
            state,
        };
        runtime.sessions.insert(peer.to_owned(), binding.clone());
        Ok(binding)
    })
}

pub(in crate::native::browser_host) fn remember_route(
    profile_id: &str,
    peer: &str,
    address: String,
    label: String,
    sid: String,
    role: Role,
) -> Result<(), String> {
    with_mut(profile_id, |runtime| {
        runtime.blocked.remove(peer);
        runtime.routes.insert(
            peer.to_owned(),
            Route {
                kaspa_address: address,
                display_name: label,
                session_sid: Some(sid),
                session_role: Some(role),
                resume_required: false,
            },
        );
        Ok(())
    })
}

pub(in crate::native::browser_host) fn remember_contact_request(
    profile_id: &str,
    sid: &str,
) -> Result<(), String> {
    with_mut(profile_id, |runtime| {
        runtime
            .pending_contact_sids
            .insert(sid.to_ascii_lowercase());
        Ok(())
    })
}

pub(in crate::native::browser_host) fn clear(profile_id: &str) {
    TRANSPORTS.with(|all| {
        all.borrow_mut().remove(profile_id);
    });
}
