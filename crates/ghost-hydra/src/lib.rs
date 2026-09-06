#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StegoProfile {
    Off,
    Deterministic,
    FastUnicode,
    FastHybrid,
    Arithmetic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContactProjection {
    pub handle: String,
    pub label: String,
    pub fingerprint: String,
    pub verified: bool,
    pub blocked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceivedProjection {
    pub from: String,
    pub plaintext: String,
    /// Exact authenticated KKTP session id when this plaintext came through the
    /// Ghost Talk KKTP wrapper. Legacy/raw HYDRA callers leave this unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_sid: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentityProjection {
    pub id: String,
    pub label: String,
    pub unlocked: bool,
}

#[cfg(feature = "upstream")]
pub struct HydraFacade {
    inner: hydra_msg::Hydra,
    stego: hydra_stego::Stego,
}

#[cfg(feature = "upstream")]
impl HydraFacade {
    pub fn open(path: impl AsRef<std::path::Path>, password: &str) -> Result<Self, String> {
        Ok(Self {
            inner: hydra_msg::Hydra::open(path, password).map_err(|e| e.to_string())?,
            stego: hydra_stego::Stego::new(),
        })
    }

    pub fn generate_identity(&mut self, password: &str) -> Result<String, String> {
        self.inner
            .generate_id(password)
            .map(|x| x.hex())
            .map_err(|e| e.to_string())
    }

    pub fn import_identity_seed(
        &mut self,
        mut seed: [u8; 32],
        password: &str,
    ) -> Result<String, String> {
        let mut encoded = b"HYDRA-MSG-ID\n".to_vec();
        encoded.extend_from_slice(hex::encode(seed).as_bytes());
        encoded.push(b'\n');
        seed.zeroize();
        let result = self.inner.import_id(&encoded, password);
        encoded.zeroize();
        let id = result.map_err(|error| error.to_string())?;
        self.inner
            .set_active_id(id, password)
            .map_err(|e| e.to_string())?;
        Ok(id.hex())
    }

    pub fn export_identity_seed(&self, id: &str, password: &str) -> Result<[u8; 32], String> {
        let id = hydra_msg::IdentityId::from_hex(id).map_err(|error| error.to_string())?;
        let mut encoded = self
            .inner
            .export_id(id, password)
            .map_err(|error| error.to_string())?;
        const MAGIC: &[u8] = b"HYDRA-MSG-ID\n";
        let parsed = (|| {
            if !encoded.starts_with(MAGIC) {
                return Err("HYDRA identity export magic mismatch".to_string());
            }
            let text = std::str::from_utf8(&encoded[MAGIC.len()..])
                .map_err(|_| "HYDRA identity export is not UTF-8".to_string())?;
            hex::decode(text.trim())
                .map_err(|_| "HYDRA identity export seed is not hex".to_string())
        })();
        encoded.zeroize();

        let mut decoded = parsed?;
        let result = decoded
            .as_slice()
            .try_into()
            .map_err(|_| "HYDRA identity export seed must be exactly 32 bytes".to_string());
        decoded.zeroize();
        result
    }

    pub fn list_identities(&self) -> Vec<IdentityProjection> {
        self.inner
            .list_ids()
            .into_iter()
            .map(|identity| IdentityProjection {
                id: identity.id().hex(),
                label: identity.label().to_owned(),
                unlocked: identity.unlocked(),
            })
            .collect()
    }

    pub fn set_active_identity(&mut self, id: &str, password: &str) -> Result<(), String> {
        let id = hydra_msg::IdentityId::from_hex(id).map_err(|error| error.to_string())?;
        self.inner
            .set_active_id(id, password)
            .map_err(|error| error.to_string())
    }

    pub fn contact_card(&self) -> Result<Vec<u8>, String> {
        self.inner.create_contact_card().map_err(|e| e.to_string())
    }

    pub fn preview_contact(&self, card: &[u8]) -> Result<ContactProjection, String> {
        let c = self
            .inner
            .preview_contact_card(card)
            .map_err(|e| e.to_string())?;
        Ok(ContactProjection {
            handle: c.id().hex(),
            label: c.label().into(),
            fingerprint: c.id().hex(),
            verified: c.verified(),
            blocked: c.blocked(),
        })
    }

    pub fn add_contact(&mut self, card: &[u8]) -> Result<ContactProjection, String> {
        let c = self.inner.add_contact(card).map_err(|e| e.to_string())?;
        Ok(ContactProjection {
            handle: c.id().hex(),
            label: c.label().into(),
            fingerprint: c.id().hex(),
            verified: c.verified(),
            blocked: c.blocked(),
        })
    }

    pub fn has_contact(&self, handle: &str) -> Result<bool, String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        Ok(self.inner.list_contacts().into_iter().any(|contact| contact.id() == id))
    }

    pub fn close_session(&mut self, handle: &str) -> Result<(), String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        self.inner.close_session(id).map_err(|e| e.to_string())
    }

    pub fn abort_handshake(&mut self, handle: &str) -> Result<(), String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        self.inner.abort_handshake(id).map_err(|e| e.to_string())
    }

    pub fn sign_application_context(&self, context: &[u8]) -> Result<Vec<u8>, String> {
        self.inner
            .sign_application_context(context)
            .map_err(|e| e.to_string())
    }

    pub fn verify_contact_application_context(
        &self,
        handle: &str,
        context: &[u8],
        signature: &[u8],
    ) -> Result<(), String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        self.inner
            .verify_contact_application_context(id, context, signature)
            .map_err(|e| e.to_string())
    }

    pub fn session_status(&self, handle: &str) -> Result<String, String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        self.inner
            .session_status(id)
            .map(|status| match status {
                hydra_msg::HydraSessionStatus::Missing => "missing",
                hydra_msg::HydraSessionStatus::Pending => "pending",
                hydra_msg::HydraSessionStatus::Active => "active",
                hydra_msg::HydraSessionStatus::Closed => "closed",
            }.to_string())
            .map_err(|e| e.to_string())
    }

    pub fn init_handshake(&mut self, handle: &str) -> Result<Vec<u8>, String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        self.inner
            .init_handshake(id)
            .map(|x| x.into_bytes())
            .map_err(|e| e.to_string())
    }

    pub fn reply_handshake(&mut self, offer: &[u8]) -> Result<Vec<u8>, String> {
        self.inner
            .reply_handshake(offer)
            .map(|x| x.into_bytes())
            .map_err(|e| e.to_string())
    }

    pub fn finish_handshake(&mut self, answer: &[u8]) -> Result<Vec<u8>, String> {
        self.inner
            .finish_handshake(answer)
            .map(|x| x.into_bytes())
            .map_err(|e| e.to_string())
    }

    pub fn accept_finish(&mut self, finish: &[u8]) -> Result<(), String> {
        self.inner
            .accept_handshake_finish(finish)
            .map_err(|e| e.to_string())
    }

    pub fn send(
        &mut self,
        handle: &str,
        data: &[u8],
        profile: StegoProfile,
    ) -> Result<Vec<Vec<u8>>, String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        let msg = hydra_msg::HydraMessage::text(
            std::str::from_utf8(data).map_err(|_| "Ghost Talk text is not valid UTF-8".to_string())?,
        );
        // Kaspa's physical transaction payload is far smaller than HYDRA's
        // fixed 4/32/144 KiB padded envelope classes. For the non-steganographic
        // Kaspa carrier use HYDRA's authenticated compact profile as well: it
        // keeps the same ratchet/AEAD semantics while avoiding tens of thousands
        // of padding bytes that Ghost Talk would otherwise have to split across
        // many on-chain transactions.
        if profile == StegoProfile::Off {
            let env = self
                .inner
                .send_compact(id, msg)
                .map_err(|e| e.to_string())?;
            return Ok(vec![env.into_bytes()]);
        }
        let env = self
            .inner
            .send_compact(id, msg)
            .map_err(|e| e.to_string())?;
        let p = map_profile(profile)?;
        let cover = self
            .stego
            .encode(env.as_bytes(), p)
            .map_err(|e| e.to_string())?;
        Ok(vec![cover.into_bytes()])
    }

    pub fn receive(
        &mut self,
        data: &[u8],
        profile: StegoProfile,
    ) -> Result<Option<ReceivedProjection>, String> {
        let msg = if profile == StegoProfile::Off {
            // Backward-compatible receive: fixed HYDRA envelopes from older
            // Ghost Talk builds remain valid, while new Kaspa traffic uses the
            // variable-length compact envelope. Fixed classes have exact public
            // sizes, so selecting the decoder does not consume ratchet state on
            // a speculative failed decode.
            if matches!(
                data.len(),
                hydra_core::LITE_ENVELOPE_SIZE
                    | hydra_core::STANDARD_ENVELOPE_SIZE
                    | hydra_core::FULL_ENVELOPE_SIZE
            ) {
                self.inner.receive(data).map_err(|e| e.to_string())?
            } else {
                Some(
                    self.inner
                        .receive_compact(data)
                        .map_err(|e| e.to_string())?,
                )
            }
        } else {
            let cover =
                std::str::from_utf8(data).map_err(|_| "stego cover not utf-8".to_string())?;
            let env = self
                .stego
                .decode(cover, map_profile(profile)?)
                .map_err(|e| e.to_string())?;
            Some(self.inner.receive_compact(env).map_err(|e| e.to_string())?)
        };
        match msg {
            None => Ok(None),
            Some(m) => Ok(Some(ReceivedProjection {
                from: m.from().hex(),
                plaintext: m.text().map_err(|e| e.to_string())?,
                session_sid: None,
            })),
        }
    }
}

#[cfg(feature = "upstream")]
fn map_profile(p: StegoProfile) -> Result<hydra_stego::StegoProfile, String> {
    Ok(match p {
        StegoProfile::Off => return Err("off has no stego profile".into()),
        StegoProfile::Deterministic => hydra_stego::StegoProfile::Deterministic,
        StegoProfile::FastUnicode => hydra_stego::StegoProfile::FastUnicode,
        StegoProfile::FastHybrid => hydra_stego::StegoProfile::FastHybrid,
        StegoProfile::Arithmetic => hydra_stego::StegoProfile::Arithmetic,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupMemberBootstrap {
    pub member_id: [u8; 32],
    pub fingerprint: [u8; 32],
    pub role: String,
    pub leaf: u32,
}

/// One already-committed HYDRA group epoch. Membership/role changes are not
/// mutated locally: callers must install a newly authenticated commit/epoch.
#[cfg(feature = "upstream")]
pub struct GroupGuard {
    state: hydra_group::GroupState,
}

#[cfg(feature = "upstream")]
impl GroupGuard {
    pub fn from_epoch(
        group_id: [u8; 32],
        broadcast: bool,
        epoch: u64,
        state_version: u64,
        members: &[GroupMemberBootstrap],
        epoch_key: [u8; 32],
    ) -> Result<Self, String> {
        let mode = if broadcast {
            hydra_group::GroupMode::Broadcast
        } else {
            hydra_group::GroupMode::Interactive
        };
        let mut roster = Vec::with_capacity(members.len());
        for m in members {
            roster.push(hydra_group::RosterEntry {
                member_id: hydra_group::MemberId(m.member_id),
                device_identity_fingerprint: hydra_core::types::IdentityFingerprint(m.fingerprint),
                role: parse_role(&m.role, mode)?,
                status: hydra_group::MemberStatus::Active,
                tree_leaf_slot: m.leaf,
                joined_epoch: hydra_core::types::Epoch(epoch),
                removed_epoch: hydra_core::types::Epoch(0),
            });
        }
        let governor = roster
            .iter()
            .find(|m| m.role == hydra_group::GroupRole::Moderator)
            .ok_or_else(|| "group epoch requires a moderator/governor".to_string())?
            .member_id;
        let mut state = hydra_group::GroupState::new_validated(hydra_group::GroupStateConfig {
            group_id: hydra_core::types::GroupId(group_id),
            mode,
            mechanism: mode.required_mechanism(),
            epoch: hydra_core::types::Epoch(epoch),
            state_version: hydra_group::StateVersion(state_version),
            governance_policy: hydra_group::GovernancePolicy::single_signer(governor),
            mode_policy: hydra_group::ModePolicy::default(),
            roster,
        })
        .map_err(group_error)?;
        state
            .install_epoch_sender_chains(&hydra_core::types::Secret32(epoch_key))
            .map_err(group_error)?;
        Ok(Self { state })
    }

    pub fn can_send(&self, member: [u8; 32]) -> bool {
        self.state
            .require_sender(hydra_group::MemberId(member))
            .is_ok()
    }

    pub fn seal(&mut self, sender: [u8; 32], content: &[u8]) -> Result<Vec<u8>, String> {
        self.state
            .seal_group_data(hydra_group::MemberId(sender), content)
            .map(|m| m.envelope)
            .map_err(group_error)
    }

    pub fn open(&mut self, envelope: &[u8]) -> Result<Vec<u8>, String> {
        self.state
            .open_group_data(envelope)
            .map(|m| m.content)
            .map_err(group_error)
    }
}

#[cfg(feature = "upstream")]
fn group_error(error: hydra_group::GroupError) -> String {
    format!("HYDRA group error: {error:?}")
}

#[cfg(feature = "upstream")]
fn parse_role(role: &str, mode: hydra_group::GroupMode) -> Result<hydra_group::GroupRole, String> {
    let role = match role.to_ascii_lowercase().as_str() {
        "moderator" => hydra_group::GroupRole::Moderator,
        "presenter" => hydra_group::GroupRole::Presenter,
        "audience" => hydra_group::GroupRole::Audience,
        "member" => hydra_group::GroupRole::Member,
        _ => return Err("unknown group role".into()),
    };
    if !role.is_active_in_mode(mode) {
        return Err("role is invalid for this HYDRA group mode".into());
    }
    Ok(role)
}

#[cfg(all(test, feature = "upstream"))]
mod stego_integration_tests {
    use super::{map_profile, StegoProfile};

    #[test]
    fn current_hydra_deterministic_stego_roundtrips_compact_carrier_bytes() {
        let payload = b"GHOST-TALK-HYDRA-COMPACT-ENVELOPE";
        let stego = hydra_stego::Stego::new();
        let profile = map_profile(StegoProfile::Deterministic).expect("deterministic profile");
        let cover = stego.encode(payload, profile).expect("deterministic cover encode");
        assert!(!cover.is_empty());
        assert_ne!(cover.as_bytes(), payload);
        let decoded = stego.decode(&cover, profile).expect("deterministic cover decode");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn current_hydra_model_free_profile_is_exactly_deterministic() {
        assert_eq!(
            map_profile(StegoProfile::Deterministic).expect("deterministic profile"),
            hydra_stego::StegoProfile::Deterministic,
        );
    }
}
