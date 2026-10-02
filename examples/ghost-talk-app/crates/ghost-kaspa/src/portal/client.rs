#[cfg(all(feature = "upstream", not(target_arch = "wasm32")))]
use super::carriers::LiveBlockEvent;
#[cfg(all(feature = "upstream", not(target_arch = "wasm32")))]
use crate::LiveTransactionObservation;
#[cfg(all(feature = "upstream", not(target_arch = "wasm32")))]
use crate::{fallback_live_event_id, is_live_ghost_carrier};

#[cfg(feature = "upstream")]
pub type PortalMassAnalysis = kaspa_portal::transaction::mass::TransactionAnalysis;

#[cfg(feature = "upstream")]
#[derive(Clone, Debug)]
pub struct PortalCurrentUtxo {
    pub amount: u64,
    pub script_public_key: Vec<u8>,
}

#[cfg(feature = "upstream")]
#[derive(Clone)]
pub struct PortalFacade {
    pub(crate) portal: kaspa_portal::KaspaPortal,
}

#[cfg(feature = "upstream")]
impl PortalFacade {
    /// Connect through the published Kaspa Portal 1.0.1 API. Native Portal
    /// futures are Send-safe and the SDK owns the persistent wRPC transport,
    /// reconnect/replay behavior, transaction planning, and notification mux.
    pub async fn connect(network: &str, endpoint: &str) -> Result<Self, String> {
        let endpoint = endpoint.trim();
        if endpoint.is_empty() {
            return Err("Kaspa Portal connection requires a concrete wRPC endpoint".into());
        }
        let network = kaspa_portal::primitives::NetworkId::parse(network)?;
        let portal = kaspa_portal::KaspaPortal::builder()
            .network(network)
            .endpoint(endpoint)
            .connect()
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self { portal })
    }

    /// Portal network API for live notification subscriptions.
    pub fn network_api(&self) -> Result<kaspa_portal::network::NetworkApi, String> {
        self.portal
            .network()
            .cloned()
            .map_err(|error| error.to_string())
    }

    /// Blocks accepted after `low_hash_hex`, to backfill what a dropped
    /// BlockAdded stream missed. One bounded node page per call.
    pub async fn blocks_since(
        &self,
        low_hash_hex: &str,
    ) -> Result<Vec<kaspa_portal::network::wrpc::block_added::OwnedBlockAddedNotification>, String>
    {
        let hash: [u8; 32] = hex::decode(low_hash_hex)
            .map_err(|error| format!("invalid block hash: {error}"))?
            .try_into()
            .map_err(|_| "block hash must be 32 bytes".to_string())?;
        self.portal
            .chain()
            .map_err(|error| error.to_string())?
            .blocks_since(&kaspa_portal::primitives::BlockHash::new(hash))
            .await
            .map_err(|error| error.to_string())
    }

    /// Close this facade's persistent transport.
    pub fn disconnect(&self) {
        let _ = self.portal.disconnect();
    }

    pub fn endpoint(&self) -> Result<String, String> {
        self.portal
            .network()
            .map(|network| network.endpoint().to_owned())
            .map_err(|error| error.to_string())
    }

    /// Current spendable outputs from the selected public Kaspa node through
    /// Kaspa Portal. This is live wallet state, never REST.
    pub async fn current_utxos(
        &self,
        addresses: &[String],
    ) -> Result<Vec<PortalCurrentUtxo>, String> {
        self.portal
            .chain()
            .map_err(|error| error.to_string())?
            .utxos_many(addresses)
            .await
            .map_err(|error| error.to_string())
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|entry| PortalCurrentUtxo {
                        amount: entry.amount,
                        script_public_key: entry.script_public_key,
                    })
                    .collect()
            })
    }

    /// Current virtual DAA score from the selected public Kaspa node through
    /// Kaspa Portal's shared persistent connection.
    pub async fn current_virtual_daa_score(&self) -> Result<u64, String> {
        self.portal
            .chain()
            .map_err(|error| error.to_string())?
            .virtual_daa_score()
            .await
            .map(kaspa_portal::primitives::DaaScore::get)
            .map_err(|error| error.to_string())
    }

    pub fn script_pubkey_for_address(address: &str) -> Result<Vec<u8>, String> {
        kaspa_portal::primitives::address::address_to_script_pubkey(address)
    }

    /// Use Portal 1.0.1's payload-aware planner for both payload and ordinary
    /// sends. Passing an empty payload preserves normal KAS-send semantics while
    /// still using the new mass/fee-aware UTXO selection.
    pub(crate) async fn plan_send_with_payload(
        &self,
        wallet: kaspa_portal::wallet::account::derivation::WalletData,
        destination: &str,
        amount_sompi: u64,
        requested_fee_sompi: u64,
        payload: &[u8],
    ) -> Result<String, String> {
        self.portal
            .transaction()
            .plan_send_with_payload(
                &wallet,
                destination,
                amount_sompi,
                requested_fee_sompi,
                payload,
            )
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn plan_consolidation(
        &self,
        wallet: kaspa_portal::wallet::account::derivation::WalletData,
        requested_fee_sompi: u64,
    ) -> Result<String, String> {
        self.portal
            .transaction()
            .plan_consolidation(&wallet, requested_fee_sompi)
            .await
            .map_err(|error| error.to_string())
    }

    /// Exact signed/finalizable transaction analysis using Portal 1.0.1's
    /// node-aware normal fee estimate and central mass policy.
    pub(crate) async fn analyze(&self, signed_pskb: &str) -> Result<PortalMassAnalysis, String> {
        self.portal
            .transaction()
            .analyze(signed_pskb)
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn broadcast_signed_pskb(&self, signed_pskb: &str) -> Result<String, String> {
        let tx = self.portal.transaction();
        let consensus = tx
            .finalize(signed_pskb)
            .map_err(|error| error.to_string())?;
        tx.broadcast(&consensus)
            .await
            .map_err(|error| error.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn subscribe_block_added(&self) -> Result<(), String> {
        self.portal
            .network()
            .map_err(|error| error.to_string())?
            .subscribe_block_added()
            .await
            .map_err(|error| error.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn next_block_added(&self) -> Result<LiveBlockEvent, String> {
        let block = self
            .portal
            .network()
            .map_err(|error| error.to_string())?
            .next_block_added()
            .await
            .map_err(|error| error.to_string())?;

        let transaction_count = block.transactions.len();
        let mut observations = Vec::new();
        for (tx_index, transaction) in block.transactions.into_iter().enumerate() {
            if !is_live_ghost_carrier(&transaction.payload) {
                continue;
            }
            let txid = transaction.transaction_id.unwrap_or_else(|| {
                fallback_live_event_id(&block.block_hash, tx_index, &transaction.payload)
            });
            observations.push(LiveTransactionObservation {
                txid,
                daa_score: block.daa_score,
                payload: transaction.payload,
            });
        }
        Ok(LiveBlockEvent {
            block_hash: block.block_hash,
            daa_score: block.daa_score,
            transaction_count,
            observations,
        })
    }
}
