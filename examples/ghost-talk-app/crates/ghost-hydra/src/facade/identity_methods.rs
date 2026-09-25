use super::runtime::{HydraFacade, IdentityProjection, MlDsaKeyPair, Zeroize, Zeroizing};

#[cfg(feature = "upstream")]
impl HydraFacade {
    pub fn generate_identity(&mut self, password: &str) -> Result<String, String> {
        self.inner
            .generate_id(password)
            .map(|x| x.hex())
            .map_err(|e| e.to_string())
    }

    pub fn import_identity_seed(
        &mut self,
        seed: [u8; 32],
        password: &str,
    ) -> Result<String, String> {
        let seed = Zeroizing::new(seed);
        let signing_key = MlDsaKeyPair::from_seed(*seed)
            .map_err(|error| error.to_string())?
            .signing_key;
        let mut encoded = b"HYDRA-MSG-ID\n".to_vec();
        encoded.extend_from_slice(hex::encode(*seed).as_bytes());
        encoded.push(b'\n');
        let result = self.inner.import_id(&encoded, password);
        encoded.zeroize();
        let id = result.map_err(|error| error.to_string())?;
        self.inner
            .set_active_id(id, password)
            .map_err(|e| e.to_string())?;
        self.application_signing_key = Some(signing_key);
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
        let parsed_id = hydra_msg::IdentityId::from_hex(id).map_err(|error| error.to_string())?;
        self.inner
            .set_active_id(parsed_id, password)
            .map_err(|error| error.to_string())?;

        let seed = Zeroizing::new(self.export_identity_seed(id, password)?);
        let signing_key = MlDsaKeyPair::from_seed(*seed)
            .map_err(|error| error.to_string())?
            .signing_key;
        self.application_signing_key = Some(signing_key);
        Ok(())
    }

    /// Unlock the HYDRA identity while deriving Ghost Talk's application signing
    /// key from the already-unlocked wallet seed. Ghost Talk profiles contain one
    /// deterministic wallet-derived HYDRA identity, so exporting that identity
    /// back out of HYDRA solely to recover the same 32-byte seed is redundant and
    /// would run HYDRA's password KDF multiple extra times on every cold unlock.
    pub fn set_active_identity_with_seed(
        &mut self,
        id: &str,
        password: &str,
        seed: [u8; 32],
    ) -> Result<(), String> {
        let parsed_id = hydra_msg::IdentityId::from_hex(id).map_err(|error| error.to_string())?;
        self.inner
            .set_active_id(parsed_id, password)
            .map_err(|error| error.to_string())?;

        let seed = Zeroizing::new(seed);
        let signing_key = MlDsaKeyPair::from_seed(*seed)
            .map_err(|error| error.to_string())?
            .signing_key;
        self.application_signing_key = Some(signing_key);
        Ok(())
    }

    pub fn lock_active_identity(&mut self) -> Result<(), String> {
        if self
            .inner
            .list_ids()
            .iter()
            .any(|identity| identity.unlocked())
        {
            self.inner
                .lock_active_id()
                .map_err(|error| error.to_string())?;
        }
        self.application_signing_key = None;
        Ok(())
    }
}
