//! Replan sends that lost a race for an input.
//!
//! Kaspa Portal plans from the node's UTXO index, which trails the mempool by
//! about a block. A send issued right after another one from the same wallet
//! (a chat message racing a realtime announcement, say) can therefore pick an
//! input that is already spent and be rejected. Such a rejection is safe to
//! replan once the index catches up; every other error is returned unchanged.

use std::future::Future;

const MAX_REPLANS: usize = 6;
const REPLAN_DELAY_MS: u32 = 1_000;

pub(super) fn is_spent_conflict(error: &str) -> bool {
    error.contains("already spent")
}

pub(super) async fn retry_on_spent_conflict<T, F, Fut>(mut attempt: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let mut replans = 0;
    loop {
        match attempt().await {
            Err(error) if is_spent_conflict(&error) && replans < MAX_REPLANS => {
                replans += 1;
                delay().await;
            }
            result => return result,
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn delay() {
    gloo_timers::future::TimeoutFuture::new(REPLAN_DELAY_MS).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn delay() {
    tokio::time::sleep(std::time::Duration::from_millis(u64::from(REPLAN_DELAY_MS))).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const SPENT: &str = "Rejected transaction ab: output (cd, 1) already spent by transaction 1";

    #[tokio::test(start_paused = true)]
    async fn spent_conflicts_are_replanned_until_success() {
        let calls = Cell::new(0);
        let result = retry_on_spent_conflict(|| {
            calls.set(calls.get() + 1);
            let outcome = if calls.get() < 3 {
                Err(SPENT.to_string())
            } else {
                Ok(7)
            };
            async move { outcome }
        })
        .await;
        assert_eq!(result, Ok(7));
        assert_eq!(calls.get(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn other_errors_and_exhausted_replans_are_returned() {
        let calls = Cell::new(0);
        let other: Result<(), String> = retry_on_spent_conflict(|| {
            calls.set(calls.get() + 1);
            async { Err("insufficient funds".to_string()) }
        })
        .await;
        assert_eq!(other, Err("insufficient funds".into()));
        assert_eq!(calls.get(), 1);
        let exhausted: Result<(), String> =
            retry_on_spent_conflict(|| async { Err(SPENT.to_string()) }).await;
        assert!(exhausted.unwrap_err().contains("already spent"));
    }
}
