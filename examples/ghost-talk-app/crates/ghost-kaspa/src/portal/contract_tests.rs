#[cfg(all(test, feature = "upstream"))]
mod portal_send_contract_tests {
    use super::super::client::PortalFacade;

    fn require_send<T: Send>(_: T) {}
    fn require_send_sync<T: Send + Sync>() {}

    #[test]
    fn facade_handle_and_connect_future_are_send_safe() {
        require_send_sync::<PortalFacade>();
        require_send(PortalFacade::connect("mainnet", "wss://example.invalid"));
        let _ = public_portal_futures_remain_send;
    }

    fn public_portal_futures_remain_send<'a>(
        portal: &'a PortalFacade,
        addresses: &'a [String],
        wallet: kaspa_portal::wallet::account::derivation::WalletData,
    ) {
        require_send(portal.current_utxos(addresses));
        require_send(portal.current_virtual_daa_score());
        require_send(portal.plan_send_with_payload(
            wallet,
            "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
            1,
            1,
            b"x",
        ));
    }
}
