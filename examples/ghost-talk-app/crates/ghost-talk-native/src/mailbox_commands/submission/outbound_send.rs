use crate::kaspa_gateway::KaspaGatewayState;
use crate::wallet_commands::{parse_u64_decimal, WalletRuntimeState};
use ghost_kaspa::wallet::{KaspaBroadcastResult, WalletPublic, WalletSecret};

use super::super::monitor::gateway::MailboxSubmitContext;

/// Owns the unlocked secret and parsed fee for one serialized outbound mailbox operation.
/// Commands may borrow the secret for signing, but submission details stay centralized here.
pub(crate) struct OutboundMailboxSend<'a> {
    gateway: &'a KaspaGatewayState,
    wallet_state: &'a WalletRuntimeState,
    profile_id: &'a str,
    public: &'a WalletPublic,
    secret: WalletSecret,
    fee: u64,
    wrpc_override: Option<&'a str>,
}

impl<'a> OutboundMailboxSend<'a> {
    #[expect(
        clippy::too_many_arguments,
        reason = "explicit outbound ownership boundary"
    )]
    pub(crate) fn prepare(
        gateway: &'a KaspaGatewayState,
        wallet_state: &'a WalletRuntimeState,
        profile_id: &'a str,
        password: &str,
        sealed: &[u8],
        public: &'a WalletPublic,
        fee_sompi: &str,
        fee_label: &str,
        wrpc_override: Option<&'a str>,
    ) -> Result<Self, String> {
        let secret = wallet_state.secret_or_open(profile_id, password, sealed, public)?;
        let fee = parse_u64_decimal(fee_sompi, fee_label)?;
        Ok(Self {
            gateway,
            wallet_state,
            profile_id,
            public,
            secret,
            fee,
            wrpc_override,
        })
    }

    pub(crate) fn secret(&self) -> &WalletSecret {
        &self.secret
    }

    pub(crate) async fn send(
        &self,
        destination: &str,
        payloads: &[Vec<u8>],
        reuse_change: bool,
    ) -> Result<KaspaBroadcastResult, String> {
        MailboxSubmitContext::new(
            self.gateway,
            self.wallet_state,
            self.profile_id,
            &self.secret,
            self.public,
            self.wrpc_override,
        )
        .send(destination, self.fee, payloads, reuse_change)
        .await
    }
}
