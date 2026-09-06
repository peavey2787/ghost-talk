use super::rollback::generation_bound_content;
use crate::{
    codec::unpack_lobby_payload,
    packet_fragments::{is_packet_fragment_for_kind, FragmentKind},
    ContactId, Hydra, HydraEnvelope, HydraMsgError, HydraResult, HydraSessionStatus,
};
use hydra_core::types::ContentKind;
use hydra_envelope::ProtectedRecord;
use hydra_crypto::{CryptoBackend, MlDsaKeyPair, MlDsaVerificationKey, RustCryptoBackend};
use hydra_session::{SessionError, SessionResult};

fn validate_direct_record(record: &ProtectedRecord) -> SessionResult<()> {
    if record.content_kind == ContentKind::Close {
        return Ok(());
    }
    let content =
        generation_bound_content(&record.content).ok_or(SessionError::AuthenticationFailed)?;
    if record.content_kind == ContentKind::Data
        && !is_packet_fragment_for_kind(FragmentKind::Lobby, content)
        && unpack_lobby_payload(content).is_err()
    {
        Ok(())
    } else {
        Err(SessionError::AuthenticationFailed)
    }
}

impl Hydra {
    pub fn session_status(&self, contact_id: ContactId) -> HydraResult<HydraSessionStatus> {
        if let Some(session) = self.sessions.get(&contact_id) {
            return Ok(if session.closed {
                HydraSessionStatus::Closed
            } else {
                HydraSessionStatus::Active
            });
        }
        let pending =
            self.pending_offers
                .values()
                .any(|offer| offer.contact_id == contact_id)
                || self.accepted_inits.values().any(|accepted| {
                    accepted.contact_id == contact_id && accepted.candidate.is_some()
                });
        Ok(if pending {
            HydraSessionStatus::Pending
        } else {
            HydraSessionStatus::Missing
        })
    }

    pub fn close_session(&mut self, contact_id: ContactId) -> HydraResult<()> {
        {
            let session = self
                .sessions
                .get_mut(&contact_id)
                .ok_or(HydraMsgError::SessionNotFound)?;
            session.closed = true;
        }
        self.remove_session_routes(contact_id);
        Ok(())
    }

    /// Cancel any in-progress INIT/RESP handshake state for one contact without
    /// altering an already-active session. This is intentionally distinct from
    /// `close_session`: a pending HYDRA handshake has no session record yet, so
    /// callers need an explicit way to abandon stale provisional state when an
    /// application-level session is left or replaced.
    pub fn abort_handshake(&mut self, contact_id: ContactId) -> HydraResult<()> {
        self.require_contact(contact_id)?;
        self.pending_offers
            .retain(|_, pending| pending.contact_id != contact_id);
        self.accepted_inits
            .retain(|_, accepted| accepted.contact_id != contact_id);
        Ok(())
    }

    /// Sign an application-protocol context with the active ML-DSA-65 identity.
    /// The domain is fixed so these signatures cannot be confused with HYDRA's
    /// own INIT/RESP transcript signatures.
    pub fn sign_application_context(&self, context: &[u8]) -> HydraResult<Vec<u8>> {
        let record = self.active_unlocked_record()?;
        let seed = record
            .seed
            .ok_or(HydraMsgError::InvalidInput("active identity is locked"))?;
        let pair = MlDsaKeyPair::from_seed(seed)?;
        let mut input = b"HYDRA-MSG/GhostTalk/KKTP/v2/context\0".to_vec();
        let context_len = u64::try_from(context.len())
            .map_err(|_| HydraMsgError::InvalidInput("application context too large"))?;
        input.extend_from_slice(&context_len.to_be_bytes());
        input.extend_from_slice(context);
        let digest = RustCryptoBackend::sha3_512(&input);
        Ok(pair.signing_key.sign_digest(&digest)?.to_vec())
    }

    /// Verify a Ghost Talk KKTP application-context signature against one
    /// already-authenticated HYDRA contact.
    pub fn verify_contact_application_context(
        &self,
        contact_id: ContactId,
        context: &[u8],
        signature: &[u8],
    ) -> HydraResult<()> {
        let contact = self.require_contact(contact_id)?;
        let verifying_key = MlDsaVerificationKey::from_bytes(&contact.public_key)?;
        let mut input = b"HYDRA-MSG/GhostTalk/KKTP/v2/context\0".to_vec();
        let context_len = u64::try_from(context.len())
            .map_err(|_| HydraMsgError::InvalidInput("application context too large"))?;
        input.extend_from_slice(&context_len.to_be_bytes());
        input.extend_from_slice(context);
        let digest = RustCryptoBackend::sha3_512(&input);
        verifying_key.verify_digest(&digest, signature)?;
        Ok(())
    }

    pub(crate) fn seal_payload_for_contact(
        &mut self,
        contact_id: ContactId,
        payload: &[u8],
    ) -> HydraResult<HydraEnvelope> {
        let contact = self.require_contact(contact_id)?;
        if contact.blocked {
            return Err(HydraMsgError::InvalidInput("contact is blocked"));
        }
        if payload.len() > self.max_payload_content_size()? {
            return Err(HydraMsgError::InvalidInput(
                "payload exceeds configured envelope capacity",
            ));
        }
        let (min_envelope_size, max_envelope_size) = self.envelope_size_bounds()?;
        let bound_payload = self.wrap_outbound_generation(payload)?;
        let session = self
            .sessions
            .get_mut(&contact_id)
            .ok_or(HydraMsgError::SessionNotFound)?;
        if session.closed {
            return Err(HydraMsgError::SessionNotFound);
        }
        let outbound = session.state.send_data_with_envelope_bounds(
            &bound_payload,
            min_envelope_size,
            max_envelope_size,
        )?;
        Ok(HydraEnvelope(outbound.envelope))
    }

    pub(crate) fn seal_compact_payload_for_contact(
        &mut self,
        contact_id: ContactId,
        payload: &[u8],
    ) -> HydraResult<HydraEnvelope> {
        let contact = self.require_contact(contact_id)?;
        if contact.blocked {
            return Err(HydraMsgError::InvalidInput("contact is blocked"));
        }
        if payload.len()
            > hydra_session::MAX_COMPACT_CONTENT_SIZE
                .saturating_sub(crate::STATE_GENERATION_BINDING_OVERHEAD)
        {
            return Err(HydraMsgError::InvalidInput(
                "payload exceeds compact carrier capacity",
            ));
        }
        let bound_payload = self.wrap_outbound_generation(payload)?;
        let session = self
            .sessions
            .get_mut(&contact_id)
            .ok_or(HydraMsgError::SessionNotFound)?;
        if session.closed {
            return Err(HydraMsgError::SessionNotFound);
        }
        let outbound = session.state.send_compact_data(&bound_payload)?;
        Ok(HydraEnvelope(outbound.envelope))
    }

    pub(crate) fn open_payload_from_contact(
        &mut self,
        envelope: &[u8],
    ) -> HydraResult<(ContactId, Vec<u8>)> {
        self.validate_inbound_envelope_size(envelope.len())?;
        let candidates = self.receive_route_candidates(envelope)?;
        for contact_id in candidates {
            let result = {
                let Some(session) = self.sessions.get_mut(&contact_id) else {
                    continue;
                };
                if session.closed {
                    continue;
                }
                session
                    .state
                    .receive_validated(envelope, validate_direct_record)
            };
            match result {
                Ok(message) => {
                    if message.content_kind == ContentKind::Close {
                        if let Some(session) = self.sessions.get_mut(&contact_id) {
                            session.closed = true;
                        }
                    }
                    self.refresh_session_routes(contact_id)?;
                    if self
                        .contacts
                        .get(&contact_id)
                        .is_some_and(|contact| contact.blocked)
                    {
                        return Err(HydraMsgError::InvalidInput("contact is blocked"));
                    }
                    let content = self.accept_inbound_generation(contact_id, message.content)?;
                    return Ok((contact_id, content));
                }
                Err(SessionError::AuthenticationFailed) => {}
                Err(SessionError::ReplayDetected) => {
                    return Err(HydraMsgError::Session(
                        SessionError::ReplayDetected.to_string(),
                    ));
                }
                Err(error) => return Err(HydraMsgError::Session(error.to_string())),
            }
        }
        Err(HydraMsgError::SessionNotFound)
    }

    pub(crate) fn open_compact_payload_from_contact(
        &mut self,
        envelope: &[u8],
    ) -> HydraResult<(ContactId, Vec<u8>)> {
        let candidates = self.receive_compact_route_candidates(envelope)?;
        for contact_id in candidates {
            let result = {
                let Some(session) = self.sessions.get_mut(&contact_id) else {
                    continue;
                };
                if session.closed {
                    continue;
                }
                session.state.receive_compact(envelope)
            };
            match result {
                Ok(message) => {
                    self.refresh_session_routes(contact_id)?;
                    if self
                        .contacts
                        .get(&contact_id)
                        .is_some_and(|contact| contact.blocked)
                    {
                        return Err(HydraMsgError::InvalidInput("contact is blocked"));
                    }
                    let content = self.accept_inbound_generation(contact_id, message.content)?;
                    return Ok((contact_id, content));
                }
                Err(SessionError::AuthenticationFailed) => {}
                Err(SessionError::ReplayDetected) => {
                    return Err(HydraMsgError::Session(
                        SessionError::ReplayDetected.to_string(),
                    ));
                }
                Err(error) => return Err(HydraMsgError::Session(error.to_string())),
            }
        }
        Err(HydraMsgError::SessionNotFound)
    }

    pub(crate) fn open_lobby_transport_payload_from_contact(
        &mut self,
        envelope: &[u8],
    ) -> HydraResult<(ContactId, Vec<u8>)> {
        self.validate_inbound_envelope_size(envelope.len())?;
        let candidates = self.receive_route_candidates(envelope)?;
        for contact_id in candidates {
            let result = {
                let Some(session) = self.sessions.get_mut(&contact_id) else {
                    continue;
                };
                if session.closed {
                    continue;
                }
                session.state.receive_validated(envelope, |record| {
                    let content = generation_bound_content(&record.content)
                        .ok_or(SessionError::AuthenticationFailed)?;
                    if record.content_kind == ContentKind::Data
                        && (is_packet_fragment_for_kind(FragmentKind::Lobby, content)
                            || unpack_lobby_payload(content).is_ok())
                    {
                        Ok(())
                    } else {
                        Err(SessionError::AuthenticationFailed)
                    }
                })
            };
            match result {
                Ok(message) => {
                    self.refresh_session_routes(contact_id)?;
                    if self
                        .contacts
                        .get(&contact_id)
                        .is_some_and(|contact| contact.blocked)
                    {
                        return Err(HydraMsgError::InvalidInput("contact is blocked"));
                    }
                    let content = self.accept_inbound_generation(contact_id, message.content)?;
                    return Ok((contact_id, content));
                }
                Err(SessionError::AuthenticationFailed) => {}
                Err(SessionError::ReplayDetected) => {
                    return Err(HydraMsgError::Session(
                        SessionError::ReplayDetected.to_string(),
                    ));
                }
                Err(error) => return Err(HydraMsgError::Session(error.to_string())),
            }
        }
        Err(HydraMsgError::SessionNotFound)
    }
}
