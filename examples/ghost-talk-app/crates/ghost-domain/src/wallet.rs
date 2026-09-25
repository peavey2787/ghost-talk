use serde::{Deserialize, Serialize};

/// UI/runtime projection of public wallet derivation state. This is deliberately
/// separate from ghost-kaspa's signing-oriented WalletPublic model so frontend
/// code cannot accidentally acquire wallet internals while sharing one stable
/// serialized shape across application domains.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WalletProjection {
    pub network: String,
    pub account_path: String,
    #[serde(default)]
    pub receive_addresses: Vec<String>,
    #[serde(default)]
    pub change_addresses: Vec<String>,
    #[serde(default)]
    pub next_receive_index: usize,
    #[serde(default)]
    pub next_change_index: usize,
}

impl WalletProjection {
    pub fn receive_address(&self) -> &str {
        self.receive_addresses
            .get(self.next_receive_index)
            .or_else(|| self.receive_addresses.first())
            .map(String::as_str)
            .unwrap_or("")
    }

    /// Merge only monotonic derivation progress. Late asynchronous network
    /// completions are not allowed to roll back a newer wallet observation.
    pub fn merge_progress(&mut self, incoming: Self) {
        if self.network != incoming.network || self.account_path != incoming.account_path {
            return;
        }
        if incoming.receive_addresses.len() > self.receive_addresses.len() {
            self.receive_addresses = incoming.receive_addresses;
        }
        if incoming.change_addresses.len() > self.change_addresses.len() {
            self.change_addresses = incoming.change_addresses;
        }
        self.next_receive_index = self.next_receive_index.max(incoming.next_receive_index);
        self.next_change_index = self.next_change_index.max(incoming.next_change_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_merge_is_monotonic() {
        let mut current = WalletProjection {
            network: "mainnet".into(),
            account_path: "m/44'/111111'/0'".into(),
            receive_addresses: vec!["a".into(), "b".into()],
            next_receive_index: 1,
            ..Default::default()
        };
        current.merge_progress(WalletProjection {
            network: current.network.clone(),
            account_path: current.account_path.clone(),
            receive_addresses: vec!["a".into()],
            next_receive_index: 0,
            ..Default::default()
        });
        assert_eq!(current.receive_addresses.len(), 2);
        assert_eq!(current.next_receive_index, 1);
    }
}
