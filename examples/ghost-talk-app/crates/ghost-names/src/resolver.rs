use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ghost_domain::identity::{GhostNameNamespace, ProfileName, VerifiedName};

use crate::{DotkResolver, KnsResolver};

const DEFAULT_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug)]
struct CacheEntry {
    verified: VerifiedName,
}

/// Unified facade for normalization, resolution, verification, and bounded caching.
#[derive(Clone, Debug)]
pub struct GhostNameResolver {
    kns: KnsResolver,
    dotk: DotkResolver,
    ttl: Duration,
    cache: BTreeMap<(GhostNameNamespace, String), CacheEntry>,
}

impl Default for GhostNameResolver {
    fn default() -> Self {
        Self::new(KnsResolver::default(), DotkResolver::default(), DEFAULT_TTL)
    }
}

impl GhostNameResolver {
    pub fn new(kns: KnsResolver, dotk: DotkResolver, ttl: Duration) -> Self {
        Self {
            kns,
            dotk,
            ttl,
            cache: BTreeMap::new(),
        }
    }

    pub fn normalize(namespace: GhostNameNamespace, input: &str) -> Result<ProfileName, String> {
        let name = match namespace {
            GhostNameNamespace::Kns => KnsResolver::normalize(input)?,
            GhostNameNamespace::DotK => DotkResolver::normalize(input)?,
        };
        Ok(ProfileName { namespace, name })
    }

    pub async fn resolve_name(
        &mut self,
        claim: &ProfileName,
        wrpc_endpoint: Option<&str>,
    ) -> Result<VerifiedName, String> {
        let normalized = Self::normalize(claim.namespace, &claim.name)?;
        let now = now_ms()?;
        let key = (normalized.namespace, normalized.name.clone());
        if let Some(entry) = self.cache.get(&key) {
            if entry.verified.is_fresh_at(now) {
                return Ok(entry.verified.clone());
            }
        }
        let address = self.resolve_owner(&normalized, wrpc_endpoint).await?;
        let ttl_ms = u64::try_from(self.ttl.as_millis()).unwrap_or(u64::MAX);
        let verified = VerifiedName {
            namespace: normalized.namespace,
            name: normalized.name,
            kaspa_address: address,
            verified_at_ms: now,
            expires_at_ms: now.saturating_add(ttl_ms),
        };
        self.cache.insert(
            key,
            CacheEntry {
                verified: verified.clone(),
            },
        );
        Ok(verified)
    }

    pub async fn verify_binding(
        &mut self,
        claim: &ProfileName,
        expected_address: &str,
        wrpc_endpoint: Option<&str>,
    ) -> Result<VerifiedName, String> {
        let verified = self.resolve_name(claim, wrpc_endpoint).await?;
        verified_for_expected(verified, expected_address)
    }

    async fn resolve_owner(
        &self,
        claim: &ProfileName,
        wrpc_endpoint: Option<&str>,
    ) -> Result<String, String> {
        match claim.namespace {
            GhostNameNamespace::Kns => self.kns.resolve_owner(&claim.name).await,
            GhostNameNamespace::DotK => {
                let endpoint = wrpc_endpoint.ok_or_else(|| {
                    "dot.k verification requires the selected Kaspa wRPC endpoint".to_string()
                })?;
                self.dotk
                    .resolve_owner(&claim.name, endpoint)
                    .await
                    .map(|value| value.address)
            }
        }
    }

    pub fn invalidate(&mut self, namespace: GhostNameNamespace, name: &str) {
        if let Ok(normalized) = Self::normalize(namespace, name) {
            self.cache.remove(&(namespace, normalized.name));
        }
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

fn verified_for_expected(verified: VerifiedName, expected: &str) -> Result<VerifiedName, String> {
    if verified.matches_address(expected) {
        Ok(verified)
    } else {
        Err("cached name binding belongs to a different Kaspa identity".into())
    }
}

fn now_ms() -> Result<u64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before Unix epoch".to_string())?;
    u64::try_from(duration.as_millis()).map_err(|_| "system clock is out of range".to_string())
}
