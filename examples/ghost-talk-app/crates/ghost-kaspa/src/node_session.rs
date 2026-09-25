#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSession {
    generation: u64,
    network: String,
    endpoint: String,
}

impl NodeSession {
    pub fn new(generation: u64, network: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            generation,
            network: network.into(),
            endpoint: endpoint.into(),
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn network(&self) -> &str {
        &self.network
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn matches(&self, network: &str, endpoint: Option<&str>) -> bool {
        self.network == network && endpoint.is_none_or(|candidate| candidate == self.endpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::NodeSession;

    #[test]
    fn match_requires_same_network_and_requested_endpoint() {
        let session = NodeSession::new(7, "testnet-10", "wss://node.example");
        assert!(session.matches("testnet-10", None));
        assert!(session.matches("testnet-10", Some("wss://node.example")));
        assert!(!session.matches("mainnet", None));
        assert!(!session.matches("testnet-10", Some("wss://other.example")));
    }
}
