#[derive(Clone, Debug)]
pub struct KnsResolver {
    base: String,
    client: reqwest::Client,
}

impl Default for KnsResolver {
    fn default() -> Self {
        Self::new("https://api.knsdomains.org")
    }
}

impl KnsResolver {
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn normalize(name: &str) -> Result<String, String> {
        let n = name.trim().to_ascii_lowercase();
        if !n.ends_with(".kas")
            || n.len() > 255
            || n.len() < 5
            || !n.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
            })
        {
            return Err("invalid KNS name".into());
        }
        Ok(n)
    }

    pub async fn resolve_owner(&self, name: &str) -> Result<String, String> {
        let n = Self::normalize(name)?;
        let url = format!("{}/mainnet/api/v1/{}", self.base, n);
        let v: serde_json::Value = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        find_address(&v).ok_or_else(|| "KNS response has no Kaspa owner".into())
    }
}

fn find_address(v: &serde_json::Value) -> Option<String> {
    v.as_str()
        .filter(|value| value.starts_with("kaspa:"))
        .map(str::to_owned)
        .or_else(|| v.as_object()?.values().find_map(find_address))
        .or_else(|| v.as_array()?.iter().find_map(find_address))
}
