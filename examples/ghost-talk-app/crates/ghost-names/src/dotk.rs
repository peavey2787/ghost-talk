use std::time::Duration;

use reqwest::{header, Client, Url};
use serde_json::Value;

pub use crate::dotk_deed::DOTK_REGISTRY;
use crate::{
    dotk_chain::verify_live_deed_wrpc,
    dotk_deed::{bare_dotk, derive_deed, normalize_dotk},
};

const DEFAULT_DIRECTORY: &str = "https://api.dotk.name/v1";
const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;
const VERIFY_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedDotkName {
    pub name: String,
    pub address: String,
    pub deed_address: String,
    pub outpoint: String,
}

/// Resolves an untrusted dot.k directory hint, derives the deed locally, then
/// proves the live covenant-tagged UTXO against the caller-selected Kaspa node.
#[derive(Clone, Debug)]
pub struct DotkResolver {
    directory: String,
    client: Client,
}

impl Default for DotkResolver {
    fn default() -> Self {
        let directory =
            std::env::var("GHOST_DOTK_DIRECTORY_URL").unwrap_or_else(|_| DEFAULT_DIRECTORY.into());
        Self::new(directory)
    }
}

impl DotkResolver {
    pub fn new(directory: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(6))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            directory: directory.into().trim_end_matches('/').to_string(),
            client,
        }
    }

    pub fn normalize(input: &str) -> Result<String, String> {
        normalize_dotk(input)
    }

    pub async fn resolve_owner(
        &self,
        input: &str,
        wrpc_endpoint: &str,
    ) -> Result<VerifiedDotkName, String> {
        tokio::time::timeout(
            VERIFY_TIMEOUT,
            self.resolve_owner_inner(input, wrpc_endpoint),
        )
        .await
        .map_err(|_| {
            "dot.k verification timed out. No recipient was selected; please retry.".to_string()
        })?
    }

    async fn resolve_owner_inner(
        &self,
        input: &str,
        wrpc_endpoint: &str,
    ) -> Result<VerifiedDotkName, String> {
        let name = bare_dotk(input)?;
        let row = self
            .get_json(directory_url(&self.directory, &["names", &name])?)
            .await?;
        let owner_type = integer(&row, "ownerType")?;
        if owner_type > u8::MAX as u64 {
            return Err("dot.k owner type is invalid".into());
        }
        let owner = text(&row, "owner")?;
        let derived = derive_deed(&name, owner_type as u8, owner)?;
        let address = derived.address.clone().ok_or_else(|| {
            "This dot.k name is covenant-owned and has no Kaspa payment/peer address.".to_string()
        })?;
        let (txid, index) = verify_live_deed_wrpc(wrpc_endpoint, &derived).await?;
        Ok(VerifiedDotkName {
            name: format!("{name}.k"),
            address,
            deed_address: derived.deed_address,
            outpoint: format!("{txid}:{index}"),
        })
    }

    async fn get_json(&self, url: Url) -> Result<Value, String> {
        let response = self
            .client
            .get(url)
            .header(header::ACCEPT, "application/json")
            .header(header::CACHE_CONTROL, "no-cache, no-store")
            .send()
            .await
            .map_err(|_| "dot.k directory is unavailable".to_string())?;
        if !response.status().is_success() {
            return Err(
                "dot.k directory is unavailable or the name has no active registration".into(),
            );
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| "dot.k directory response could not be read".to_string())?;
        if bytes.len() > MAX_JSON_BYTES {
            return Err("dot.k directory response exceeded the safety limit".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "dot.k directory returned invalid JSON".into())
    }
}

fn directory_url(base: &str, segments: &[&str]) -> Result<Url, String> {
    let mut url =
        Url::parse(base).map_err(|_| "dot.k directory configuration is invalid".to_string())?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| "dot.k directory cannot accept path segments".to_string())?;
        path.pop_if_empty();
        for segment in segments {
            path.push(segment);
        }
    }
    Ok(url)
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("dot.k directory response is missing {key}"))
}

fn integer(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("dot.k directory response has invalid {key}"))
}
