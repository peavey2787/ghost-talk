use ghost_domain::identity::{GhostNameNamespace, ProfileName, VerifiedName};
use std::sync::Arc;

#[derive(Clone, Default)]
pub(crate) struct NameResolverState {
    inner: Arc<tokio::sync::Mutex<ghost_names::GhostNameResolver>>,
}

impl NameResolverState {
    async fn resolve(
        &self,
        claim: &ProfileName,
        wrpc_endpoint: Option<&str>,
    ) -> Result<VerifiedName, String> {
        self.inner
            .lock()
            .await
            .resolve_name(claim, wrpc_endpoint)
            .await
    }
}

pub(super) struct ResolvedTarget {
    pub(super) address: String,
    pub(super) primary_name: Option<ProfileName>,
    pub(super) verified_names: Vec<VerifiedName>,
}

impl ResolvedTarget {
    pub(super) fn kns_name(&self) -> Option<String> {
        self.primary_name
            .as_ref()
            .filter(|name| name.namespace == GhostNameNamespace::Kns)
            .map(|name| name.name.clone())
    }

    pub(super) fn dotk_name(&self) -> Option<String> {
        self.primary_name
            .as_ref()
            .filter(|name| name.namespace == GhostNameNamespace::DotK)
            .map(|name| name.name.clone())
    }
}

pub(super) async fn resolve_target(
    names: &NameResolverState,
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    target: &str,
    network: &str,
    wrpc_override: Option<&str>,
) -> Result<ResolvedTarget, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("Enter a Kaspa address, KNS name, or dot.k name".into());
    }
    let verified = resolve_human_name(names, gateway, target, network, wrpc_override).await?;
    let address = verified
        .as_ref()
        .map(|name| name.kaspa_address.clone())
        .unwrap_or_else(|| target.to_owned());
    let parsed = ghost_kaspa::validate_destination(&address)?;
    Ok(ResolvedTarget {
        address: parsed.as_str().to_owned(),
        primary_name: verified.as_ref().map(VerifiedName::as_profile_name),
        verified_names: verified.into_iter().collect(),
    })
}

async fn resolve_human_name(
    names: &NameResolverState,
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    target: &str,
    network: &str,
    wrpc_override: Option<&str>,
) -> Result<Option<VerifiedName>, String> {
    let lower = target.to_ascii_lowercase();
    if lower.ends_with(".kas") {
        require_mainnet_name(network, "KNS")?;
        let claim = ghost_names::GhostNameResolver::normalize(GhostNameNamespace::Kns, target)?;
        return names.resolve(&claim, None).await.map(Some);
    }
    if lower.ends_with(".k") {
        require_mainnet_name(network, "dot.k")?;
        let portal = gateway.portal_for_network(network, wrpc_override).await?;
        let endpoint = portal.endpoint()?;
        let claim = ghost_names::GhostNameResolver::normalize(GhostNameNamespace::DotK, target)?;
        return names.resolve(&claim, Some(&endpoint)).await.map(Some);
    }
    Ok(None)
}

fn require_mainnet_name(network: &str, label: &str) -> Result<(), String> {
    if network.trim().eq_ignore_ascii_case("mainnet") {
        Ok(())
    } else {
        Err(format!(
            "{label} names resolve on Kaspa mainnet; use a Kaspa address on testnet"
        ))
    }
}
