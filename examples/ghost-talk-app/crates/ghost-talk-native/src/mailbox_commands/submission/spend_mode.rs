//! Per-profile mailbox carrier selection. A `NoSpend` profile never signs a
//! Kaspa transaction for mailbox traffic: the outbound boundary hands the exact
//! GHST frames back to the frontend, which publishes them on the swarm mailbox.

use ghost_api::{MailboxSendResult, SwarmDelivery};
use ghost_domain::settings::SpendMode;
use ghost_kaspa::wallet::{KaspaBroadcastResult, WalletPublic, MAILBOX_OUTPUT_SOMPI};
use tauri::State;

use crate::wallet_commands::WalletRuntimeState;

/// One outbound mailbox carrier: a Kaspa broadcast, or frames for the swarm.
pub(crate) struct MailboxSubmission {
    pub(crate) broadcast: KaspaBroadcastResult,
    pub(crate) swarm: Option<SwarmDelivery>,
}

impl MailboxSubmission {
    pub(crate) fn kaspa(broadcast: KaspaBroadcastResult) -> Self {
        Self {
            broadcast,
            swarm: None,
        }
    }

    /// The id of the last frame stands in for the transaction id, as the last
    /// transaction's id does for a fragmented Kaspa send.
    pub(crate) fn swarm(public: &WalletPublic, destination: &str, payloads: &[Vec<u8>]) -> Self {
        Self {
            broadcast: KaspaBroadcastResult {
                transaction_id: payloads
                    .last()
                    .map(|frame| ghost_protocol::swarm_frame_id(frame))
                    .unwrap_or_default(),
                fee_sompi: "0".into(),
                public: public.clone(),
                timings: None,
            },
            swarm: Some(SwarmDelivery {
                destination: destination.to_owned(),
                frames_hex: payloads.iter().map(hex::encode).collect(),
            }),
        }
    }

    pub(crate) fn transaction_id(&self) -> &str {
        &self.broadcast.transaction_id
    }

    pub(crate) fn into_send_result(
        self,
        pending_handshake: bool,
        pending_id: Option<String>,
    ) -> MailboxSendResult {
        MailboxSendResult {
            transaction_id: self.broadcast.transaction_id,
            fee_sompi: self.broadcast.fee_sompi,
            mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
            public: crate::wallet_commands::wallet_projection(&self.broadcast.public),
            pending_handshake,
            pending_id,
            swarm: self.swarm,
        }
    }
}

impl WalletRuntimeState {
    pub(crate) fn spend_mode(&self, profile_id: &str) -> Result<SpendMode, String> {
        Ok(self
            .spend_modes
            .lock()
            .map_err(|_| "wallet spend-mode state is poisoned".to_string())?
            .get(profile_id)
            .copied()
            .unwrap_or_default())
    }

    fn set_spend_mode(&self, profile_id: String, mode: SpendMode) -> Result<(), String> {
        self.spend_modes
            .lock()
            .map_err(|_| "wallet spend-mode state is poisoned".to_string())?
            .insert(profile_id, mode);
        Ok(())
    }
}

#[tauri::command]
pub fn mailbox_set_spend_mode(
    wallet_state: State<'_, WalletRuntimeState>,
    profile_id: String,
    spend_mode: SpendMode,
) -> Result<(), String> {
    wallet_state.set_spend_mode(profile_id, spend_mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spend_mode_defaults_to_kaspa_per_profile() {
        let state = WalletRuntimeState::default();
        assert_eq!(state.spend_mode("a").unwrap(), SpendMode::Kaspa);
        state
            .set_spend_mode("a".into(), SpendMode::NoSpend)
            .unwrap();
        assert_eq!(state.spend_mode("a").unwrap(), SpendMode::NoSpend);
        assert_eq!(state.spend_mode("b").unwrap(), SpendMode::Kaspa);
    }

    #[test]
    fn swarm_submission_hands_back_frames_without_spending() {
        let frame =
            ghost_protocol::fragment(ghost_core::Id128([1; 16]), b"KKTP:ANCHOR:{}").unwrap();
        let public = WalletPublic {
            network: "mainnet".into(),
            account_path: "m/44'/111111'/0'".into(),
            receive_addresses: Vec::new(),
            change_addresses: Vec::new(),
            next_receive_index: 0,
            next_change_index: 0,
        };
        let submission = MailboxSubmission::swarm(&public, "kaspa:qdest", &frame);
        assert_eq!(
            submission.transaction_id(),
            ghost_protocol::swarm_frame_id(&frame[0])
        );
        let result = submission.into_send_result(false, None);
        assert_eq!(result.fee_sompi, "0");
        let swarm = result.swarm.unwrap();
        assert_eq!(swarm.destination, "kaspa:qdest");
        assert_eq!(swarm.frames_hex, vec![hex::encode(&frame[0])]);
    }
}
