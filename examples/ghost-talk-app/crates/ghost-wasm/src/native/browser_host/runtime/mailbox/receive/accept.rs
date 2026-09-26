use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::{HydraContactAcceptedProjection, HydraMailboxResult};
use ghost_protocol::GhostContactAccept;

use crate::native::browser_host::runtime::{
    handshake,
    mailbox::common::discard,
    secure_transport::with_hydra_runtime,
    session::{self, Role, SessionState},
};

pub(super) fn receive_accept(
    profile: &str,
    local: &[String],
    envelope: &[u8],
) -> Result<HydraMailboxResult, String> {
    let accepted = GhostContactAccept::decode(envelope)?;
    ghost_kaspa::verify_contact_accept(&accepted)?;
    if !accept_is_actionable(profile, local, &accepted)? {
        return Ok(discard());
    }
    let peer = install_responder(profile, &accepted)?;
    Ok(HydraMailboxResult {
        contact_accepted: Some(HydraContactAcceptedProjection {
            request_id: accepted.request_id,
            peer_address: accepted.responder.kaspa_address,
            acceptor_address: accepted.acceptor_kaspa_address,
            peer_label: accepted.responder.display_name,
            peer_hydra_id: peer,
        }),
        ..discard()
    })
}

/// Only an acceptance of a request this profile sent (or of its current
/// session) from another identity, addressed to this wallet, is actionable.
fn accept_is_actionable(
    profile: &str,
    local: &[String],
    accepted: &GhostContactAccept,
) -> Result<bool, String> {
    let identity = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    if accepted.responder.hydra_identity_id == identity {
        return Ok(false);
    }
    if !local
        .iter()
        .any(|address| address == &accepted.recipient_kaspa_address)
    {
        return Err("contact acceptance is not addressed to this Ghost Talk wallet".into());
    }
    session::with(profile, |runtime| {
        Ok(runtime
            .pending_contact_sids
            .contains(&accepted.request_id.to_ascii_lowercase())
            || runtime
                .routes
                .get(&accepted.responder.hydra_identity_id)
                .and_then(|route| route.session_sid.as_deref())
                == Some(accepted.request_id.as_str()))
    })
}

/// Add the responder's authenticated HYDRA contact and install the
/// initiator-side session and route for the accepted SID.
fn install_responder(profile: &str, accepted: &GhostContactAccept) -> Result<String, String> {
    let card = BASE64
        .decode(&accepted.responder.hydra_contact_card_b64)
        .map_err(|_| "contact-accept HYDRA contact card is not valid base64".to_string())?;
    let peer = with_hydra_runtime(profile, |hydra| {
        let contact = hydra.add_contact(&card)?;
        if contact.handle != accepted.responder.hydra_identity_id {
            return Err(
                "contact-accept HYDRA identity does not match its authenticated contact card"
                    .into(),
            );
        }
        Ok(contact.handle)
    })?;
    handshake::reset_peer_crypto(profile, &peer)?;
    let sid = accepted.request_id.clone();
    session::install(
        profile,
        &peer,
        sid.clone(),
        Role::Initiator,
        SessionState::Discovered,
    )?;
    session::remember_route(
        profile,
        &peer,
        accepted.responder.kaspa_address.clone(),
        accepted.responder.display_name.clone(),
        sid,
        Role::Initiator,
    )?;
    session::with_mut(profile, |runtime| {
        runtime
            .pending_contact_sids
            .remove(&accepted.request_id.to_ascii_lowercase());
        Ok(())
    })?;
    Ok(peer)
}
