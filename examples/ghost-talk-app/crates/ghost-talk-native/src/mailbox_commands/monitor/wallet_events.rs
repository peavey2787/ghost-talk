use std::time::Duration;

pub(crate) async fn wait_for_wallet_transaction_event(
    receiver: &mut tokio::sync::watch::Receiver<Vec<String>>,
    transaction_id: &str,
) -> Result<(), String> {
    const EVENT_TIMEOUT: Duration = Duration::from_secs(10);
    let wait = async {
        loop {
            let observed = {
                let current = receiver.borrow_and_update();
                current.iter().any(|observed| observed == transaction_id)
            };
            if observed {
                return Ok(());
            }
            receiver
                .changed()
                .await
                .map_err(|_| "Kaspa UTXO notification stream closed while waiting for the submitted transaction".to_string())?;
        }
    };
    tokio::time::timeout(EVENT_TIMEOUT, wait)
        .await
        .map_err(|_| format!(
            "Kaspa accepted transaction {transaction_id}, but its UTXO-change notification was not observed before the send timeout"
        ))?
}
