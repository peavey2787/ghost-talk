#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use super::runtime::{acquire_native_profile_lease, recover_stale_native_profile_lock};
use super::runtime::{
    application_context_digest, ContactProjection, Deserialize, HydraFacade, MlDsaVerificationKey,
    ReceivedProjection, Serialize, StegoProfile,
};

#[cfg(feature = "upstream")]
impl HydraFacade {
    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    pub fn open(path: impl AsRef<std::path::Path>, password: &str) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        // Ghost Talk owns a long-lived OS advisory lease for the complete facade
        // lifetime. File existence is irrelevant; only the live OS lock excludes
        // another process. This remains crash-safe and prevents a stale PID-only
        // HYDRA sentinel from becoming Ghost Talk's source of ownership truth.
        let profile_lease = acquire_native_profile_lease(&path)?;
        let inner = open_hydra_with_stale_lock_recovery(&path, password, &profile_lease)?;
        Ok(Self {
            inner,
            stego: hydra_stego::Stego::new(),
            application_signing_key: None,
            _profile_lease: profile_lease,
        })
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
        Ok(self
            .inner
            .list_contacts()
            .into_iter()
            .any(|contact| contact.id() == id))
    }

    pub fn close_session(&mut self, handle: &str) -> Result<(), String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        self.inner.close_session(id).map_err(|e| e.to_string())
    }

    pub fn abort_handshake(&mut self, handle: &str) -> Result<(), String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        match self.inner.session_status(id).map_err(|e| e.to_string())? {
            hydra_msg::HydraSessionStatus::Pending => {
                // Upstream a8b4b317 has no public pending-handshake abort API.
                // Blocking a contact clears its provisional INIT/RESP state;
                // immediately unblocking restores the unchanged contact/session
                // policy while leaving that provisional state discarded. Ghost
                // Talk calls this path only for a visibly pending handshake.
                self.inner.block_contact(id).map_err(|e| e.to_string())?;
                self.inner.unblock_contact(id).map_err(|e| e.to_string())?;
                Ok(())
            }
            hydra_msg::HydraSessionStatus::Missing
            | hydra_msg::HydraSessionStatus::Active
            | hydra_msg::HydraSessionStatus::Closed => Ok(()),
        }
    }

    pub fn sign_application_context(&self, context: &[u8]) -> Result<Vec<u8>, String> {
        let signing_key = self.application_signing_key.as_ref().ok_or_else(|| {
            "Ghost Talk application signing key is unavailable; unlock the HYDRA identity first"
                .to_string()
        })?;
        let digest = application_context_digest(context)?;
        signing_key
            .sign_digest(&digest)
            .map(|signature| signature.to_vec())
            .map_err(|error| error.to_string())
    }

    pub fn verify_contact_application_context(
        &self,
        handle: &str,
        context: &[u8],
        signature: &[u8],
    ) -> Result<(), String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        let contact = self.inner.get_contact(id).map_err(|e| e.to_string())?;
        let verification_key = MlDsaVerificationKey::from_bytes(contact.public_key())
            .map_err(|error| error.to_string())?;
        let digest = application_context_digest(context)?;
        verification_key
            .verify_digest(&digest, signature)
            .map_err(|error| error.to_string())
    }

    pub fn session_status(&self, handle: &str) -> Result<String, String> {
        let id = hydra_msg::ContactId::from_hex(handle).map_err(|e| e.to_string())?;
        self.inner
            .session_status(id)
            .map(|status| session_status_label(status).to_string())
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
            std::str::from_utf8(data)
                .map_err(|_| "Ghost Talk text is not valid UTF-8".to_string())?,
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
            Some(
                self.inner
                    .receive_compact(data)
                    .map_err(|e| e.to_string())?,
            )
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
                content_type: None,
                session_sid: None,
            })),
        }
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn open_hydra_with_stale_lock_recovery(
    path: &std::path::Path,
    password: &str,
    profile_lease: &super::runtime::NativeProfileLease,
) -> Result<hydra_msg::Hydra, String> {
    let error = match hydra_msg::Hydra::open(path, password) {
        Ok(hydra) => return Ok(hydra),
        Err(error) => error,
    };
    let message = error.to_string();
    if !message.contains("native profile is already open") {
        return Err(message);
    }
    recover_stale_native_profile_lock(path, profile_lease)?;
    hydra_msg::Hydra::open(path, password).map_err(map_reopened_profile_error)
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn map_reopened_profile_error(error: hydra_msg::HydraMsgError) -> String {
    let message = error.to_string();
    if message.contains("native profile is already open") {
        "HYDRA profile became busy while Ghost Talk was opening it".to_string()
    } else {
        message
    }
}

#[cfg(feature = "upstream")]
fn session_status_label(status: hydra_msg::HydraSessionStatus) -> &'static str {
    [
        (
            matches!(status, hydra_msg::HydraSessionStatus::Missing),
            "missing",
        ),
        (
            matches!(status, hydra_msg::HydraSessionStatus::Pending),
            "pending",
        ),
        (
            matches!(status, hydra_msg::HydraSessionStatus::Active),
            "active",
        ),
        (
            matches!(status, hydra_msg::HydraSessionStatus::Closed),
            "closed",
        ),
    ]
    .into_iter()
    .find_map(|(selected, label)| selected.then_some(label))
    .expect("all upstream HYDRA session states are mapped")
}

#[cfg(feature = "upstream")]
pub(crate) fn map_profile(p: StegoProfile) -> Result<hydra_stego::StegoProfile, String> {
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
    pub(crate) state: hydra_group::GroupState,
}
