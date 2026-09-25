use super::super::delivery_ack::WalletMonitorWorker;

impl WalletMonitorWorker {
    pub(crate) fn merge_observed_transaction(
        history: &mut Vec<crate::wallet_commands::WalletHistoryEntry>,
        observed: crate::kaspa_wallet_events::ObservedWalletTransaction,
    ) {
        let Some(existing) = history
            .iter_mut()
            .find(|entry| entry.transaction_id == observed.transaction_id)
        else {
            history.push(crate::wallet_commands::WalletHistoryEntry {
                transaction_id: observed.transaction_id,
                blue_score: observed.block_daa_score.to_string(),
                block_time: None,
                ghost_payload: false,
                addresses: observed.addresses,
            });
            return;
        };
        let score = existing.blue_score.parse::<u64>().unwrap_or_default();
        existing.blue_score = score.max(observed.block_daa_score).to_string();
        for address in observed.addresses {
            if !existing.addresses.contains(&address) {
                existing.addresses.push(address);
            }
        }
    }

    pub(crate) fn merged_history(
        &self,
        observed_transactions: Vec<crate::kaspa_wallet_events::ObservedWalletTransaction>,
    ) -> Vec<crate::wallet_commands::WalletHistoryEntry> {
        let mut history = self
            .snapshot
            .as_ref()
            .map(|current| current.history.clone())
            .unwrap_or_else(|| self.persisted_history.clone());
        for observed in observed_transactions {
            Self::merge_observed_transaction(&mut history, observed);
        }
        history.sort_by_key(|entry| {
            std::cmp::Reverse(entry.blue_score.parse::<u64>().unwrap_or_default())
        });
        history
    }

    pub(crate) fn observed_addresses(
        history: &[crate::wallet_commands::WalletHistoryEntry],
        active_addresses: &[String],
    ) -> Vec<String> {
        let mut addresses = active_addresses.to_vec();
        for entry in history {
            for address in &entry.addresses {
                if !addresses.contains(address) {
                    addresses.push(address.clone());
                }
            }
        }
        addresses
    }

    pub(crate) fn apply_utxo_set(
        &mut self,
        public: &WalletPublic,
        balance_sompi: u64,
        utxo_count: usize,
        active_addresses: Vec<String>,
        changed_transaction_ids: Vec<String>,
        observed_transactions: Vec<crate::kaspa_wallet_events::ObservedWalletTransaction>,
    ) {
        self.handle_changed_transactions(changed_transaction_ids);
        let history = self.merged_history(observed_transactions);
        self.persisted_history = history.clone();
        let observed_addresses = Self::observed_addresses(&history, &active_addresses);
        let recommended_receive_index =
            crate::wallet_commands::recommended_receive_index(public, &observed_addresses);
        self.snapshot = Some(crate::wallet_commands::WalletSnapshot {
            balance_sompi: balance_sompi.to_string(),
            utxo_count: utxo_count.to_string(),
            blue_score: self
                .latest_daa_score
                .map(|value| value.to_string())
                .unwrap_or_default(),
            history,
            active_addresses,
            recommended_receive_index,
        });
        self.publish_wallet_snapshot();
    }

    pub(crate) fn handle_wallet_event(
        &mut self,
        public: &WalletPublic,
        event: crate::kaspa_wallet_events::WalletNodeEvent,
    ) {
        if let Some(connected) = wallet_connection_state(&event) {
            self.wallet_subscriptions_connected = connected;
            let attempts = self
                .reconnect_attempts
                .saturating_add(u32::from(!connected));
            self.sync_network_status(attempts);
            return;
        }
        if let crate::kaspa_wallet_events::WalletNodeEvent::VirtualDaaScoreChanged(daa) = event {
            self.apply_daa_score(daa);
            return;
        }
        if let crate::kaspa_wallet_events::WalletNodeEvent::UtxoSet {
            balance_sompi,
            utxo_count,
            active_addresses,
            changed_transaction_ids,
            observed_transactions,
        } = event
        {
            self.apply_utxo_set(
                public,
                balance_sompi,
                utxo_count,
                active_addresses,
                changed_transaction_ids,
                observed_transactions,
            );
        }
    }

    fn apply_daa_score(&mut self, daa: u64) {
        self.latest_daa_score = Some(daa);
        if let Some(current) = self.snapshot.as_mut() {
            current.blue_score = daa.to_string();
            self.publish_wallet_snapshot();
        }
    }

    pub(crate) fn drain_wallet_events(&mut self, public: &WalletPublic) {
        while self.drain_wallet_event(public) {}
    }

    fn drain_wallet_event(&mut self, public: &WalletPublic) -> bool {
        let Some(stream) = self.wallet_events.as_mut() else {
            return false;
        };
        let result = stream.events.try_recv();
        if let Ok(event) = result {
            self.handle_wallet_event(public, event);
            return true;
        }
        if matches!(
            result,
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ) {
            return false;
        }
        if matches!(
            result,
            Err(tokio::sync::broadcast::error::TryRecvError::Closed)
        ) {
            self.drop_wallet_event_stream();
            return false;
        }
        let Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) = result else {
            return false;
        };
        crate::debug_log::record(
            "warn",
            "kaspa",
            "wallet-event-consumer-lagged",
            format!("profile={} skipped_events={skipped}", self.profile_id),
        );
        true
    }

    fn drop_wallet_event_stream(&mut self) {
        self.wallet_events = None;
        self.wallet_subscriptions_connected = false;
        self.sync_network_status(self.reconnect_attempts.saturating_add(1));
    }
}

fn wallet_connection_state(event: &crate::kaspa_wallet_events::WalletNodeEvent) -> Option<bool> {
    match event {
        crate::kaspa_wallet_events::WalletNodeEvent::Connected => Some(true),
        crate::kaspa_wallet_events::WalletNodeEvent::Reconnecting => Some(false),
        _ => None,
    }
}
use ghost_kaspa::wallet::WalletPublic;
