use super::runtime::HydraFacade;
use hydra_msg::Hydra as UpstreamHydra;

impl HydraFacade {
    /// Open this profile from HYDRA's encrypted IndexedDB snapshot. The
    /// upstream store protects writes with a compare-and-swap revision so two
    /// browser tabs cannot silently overwrite one another's ratchet state.
    pub async fn open_browser(name: &str, password: &str) -> Result<Self, String> {
        let (inner, revision) = UpstreamHydra::open_browser_persistent(name, password)
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self {
            inner,
            stego: hydra_stego::Stego::new(),
            application_signing_key: None,
            browser_persistence_name: name.to_owned(),
            browser_persistence_revision: revision,
        })
    }

    /// Commit browser-side HYDRA mutations to the encrypted IndexedDB snapshot.
    pub async fn flush_browser(&mut self) -> Result<(), String> {
        let next = self
            .inner
            .flush_browser_persistent(
                &self.browser_persistence_name,
                self.browser_persistence_revision,
            )
            .await
            .map_err(|error| error.to_string())?;
        self.browser_persistence_revision = next;
        Ok(())
    }
}
