use serde::{de::DeserializeOwned, Deserialize, Serialize};

const PAGE_LIMIT: usize = 50;
const MAX_PAGES: usize = 20;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KasiaHandshakeResponse {
    #[serde(rename = "tx_id")]
    pub tx_id: String,
    pub sender: String,
    pub receiver: String,
    #[serde(rename = "block_time", default)]
    pub block_time: Option<u64>,
    #[serde(rename = "accepting_block", default)]
    pub accepting_block: Option<String>,
    #[serde(rename = "accepting_daa_score", default)]
    pub accepting_daa_score: Option<u64>,
    #[serde(rename = "message_payload", default)]
    pub message_payload: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KasiaContextualMessageResponse {
    #[serde(rename = "tx_id")]
    pub tx_id: String,
    pub sender: String,
    pub alias: String,
    #[serde(rename = "block_time", default)]
    pub block_time: Option<u64>,
    #[serde(rename = "accepting_block", default)]
    pub accepting_block: Option<String>,
    #[serde(rename = "accepting_daa_score", default)]
    pub accepting_daa_score: Option<u64>,
    #[serde(rename = "message_payload", default)]
    pub message_payload: Option<String>,
}

#[derive(Clone)]
pub struct KasiaIndexerClient {
    base_url: String,
    client: reqwest::Client,
}

impl KasiaIndexerClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, String> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let local =
            base_url.starts_with("http://127.0.0.1") || base_url.starts_with("http://localhost");
        if !base_url.starts_with("https://") && !local {
            return Err(
                "Kasia indexer must use HTTPS (localhost is allowed for development)".into(),
            );
        }
        Ok(Self {
            base_url,
            client: reqwest::Client::new(),
        })
    }

    pub async fn handshakes_by_receiver(
        &self,
        address: &str,
        block_time: u64,
    ) -> Result<Vec<KasiaHandshakeResponse>, String> {
        self.paginated(
            "/handshakes/by-receiver",
            &[("address", address.to_string())],
            block_time,
        )
        .await
    }

    pub async fn handshakes_by_sender(
        &self,
        address: &str,
        block_time: u64,
    ) -> Result<Vec<KasiaHandshakeResponse>, String> {
        self.paginated(
            "/handshakes/by-sender",
            &[("address", address.to_string())],
            block_time,
        )
        .await
    }

    pub async fn contextual_by_sender(
        &self,
        address: &str,
        alias: &str,
        block_time: u64,
    ) -> Result<Vec<KasiaContextualMessageResponse>, String> {
        let params = [
            ("address", address.to_string()),
            ("alias", hex::encode(alias.as_bytes())),
        ];
        self.paginated("/contextual-messages/by-sender", &params, block_time)
            .await
    }

    async fn paginated<T: DeserializeOwned + BlockTimed>(
        &self,
        endpoint: &str,
        params: &[(&str, String)],
        start_block_time: u64,
    ) -> Result<Vec<T>, String> {
        let mut cursor = start_block_time;
        let mut all = Vec::new();
        for _ in 0..MAX_PAGES {
            let page = self.get_page(endpoint, params, cursor).await?;
            let count = page.len();
            let next = page
                .iter()
                .filter_map(BlockTimed::block_time)
                .max()
                .unwrap_or(cursor);
            all.extend(page);
            if count < PAGE_LIMIT || next <= cursor {
                break;
            }
            cursor = next;
        }
        Ok(all)
    }

    async fn get_page<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        params: &[(&str, String)],
        block_time: u64,
    ) -> Result<Vec<T>, String> {
        let url = format!("{}{}", self.base_url, endpoint);
        let mut query: Vec<(&str, String)> = params.iter().map(|(k, v)| (*k, v.clone())).collect();
        query.push(("limit", PAGE_LIMIT.to_string()));
        query.push(("block_time", block_time.to_string()));
        let response = self
            .client
            .get(url)
            .query(&query)
            .send()
            .await
            .map_err(|error| format!("Kasia indexer request failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("Kasia indexer returned HTTP {}", response.status()));
        }
        response
            .json()
            .await
            .map_err(|error| format!("invalid Kasia indexer response: {error}"))
    }
}

trait BlockTimed {
    fn block_time(&self) -> Option<u64>;
}
impl BlockTimed for KasiaHandshakeResponse {
    fn block_time(&self) -> Option<u64> {
        self.block_time
    }
}
impl BlockTimed for KasiaContextualMessageResponse {
    fn block_time(&self) -> Option<u64> {
        self.block_time
    }
}
