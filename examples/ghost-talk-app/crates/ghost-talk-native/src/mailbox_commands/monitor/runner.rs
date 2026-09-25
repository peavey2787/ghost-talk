use super::super::{
    delivery_ack::WalletMonitorWorker,
    gateway::emit_network_status,
    gateway_helpers::{live_public_profile_updates, to_live_mailbox_event},
    send_state::{
        DirectoryLiveEvent, LiveTransactionObservation, WalletLiveEvent, MAILBOX_SCAN_INTERVAL,
    },
};
impl WalletMonitorWorker {
    pub(crate) fn publish_directory_updates(
        &self,
        observations: &[LiveTransactionObservation],
        daa_score: u64,
    ) {
        let public_profiles = live_public_profile_updates(observations, daa_score);
        if public_profiles.is_empty() {
            return;
        }
        let _ = self.app.emit(
            "ghost://directory-live",
            DirectoryLiveEvent {
                profile_id: self.profile_id.clone(),
                directory_checkpoint: daa_score.to_string(),
                public_profiles,
            },
        );
    }

    pub(crate) fn handle_live_block(&mut self, event: ghost_kaspa::LiveBlockEvent) {
        let observations = event.observations;
        if observations.is_empty() {
            return;
        }
        let txids = observations
            .iter()
            .map(|observation| observation.txid.as_str())
            .collect::<Vec<_>>()
            .join(",");
        crate::debug_log::record(
            "info",
            "mailbox",
            "live-block-carriers-observed",
            format!(
                "profile={} block={} daa={} count={} txids={}",
                self.profile_id,
                event.block_hash,
                event.daa_score,
                observations.len(),
                txids,
            ),
        );
        for observation in &observations {
            self.directory.ingest_payload(
                &observation.payload,
                observation.daa_score,
                event.daa_score,
            );
        }
        self.publish_directory_updates(&observations, event.daa_score);
        let _ = self.app.emit(
            "ghost://wallet-live",
            WalletLiveEvent {
                profile_id: self.profile_id.clone(),
                snapshot: self.snapshot.clone(),
                checkpoint: self.checkpoint.clone(),
                mailbox: observations
                    .into_iter()
                    .map(to_live_mailbox_event)
                    .collect(),
            },
        );
    }

    pub(crate) fn live_stream_closed(&mut self) {
        crate::debug_log::record(
            "warn",
            "mailbox",
            "live-block-subscription-closed",
            format!("profile={}", self.profile_id),
        );
        self.live_stream = None;
        self.carrier_connected = false;
        self.sync_network_status(self.reconnect_attempts.saturating_add(1));
        self.next_live_retry = Instant::now();
    }

    pub(crate) async fn poll_live_stream(&mut self) {
        let Some(live) = self.live_stream.as_mut() else {
            tokio::time::sleep(MAILBOX_SCAN_INTERVAL).await;
            return;
        };
        let next = tokio::time::timeout(MAILBOX_SCAN_INTERVAL, live.blocks.recv()).await;
        self.handle_live_stream_poll(next);
    }

    fn handle_live_stream_poll(
        &mut self,
        next: Result<
            Result<ghost_kaspa::LiveBlockEvent, tokio::sync::broadcast::error::RecvError>,
            tokio::time::error::Elapsed,
        >,
    ) {
        if let Ok(Ok(event)) = next {
            self.handle_live_block(event);
            return;
        }
        if let Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) = next {
            crate::debug_log::record(
                "error",
                "mailbox",
                "live-block-consumer-lagged",
                format!("profile={} skipped_blocks={}", self.profile_id, skipped),
            );
            return;
        }
        if let Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) = next {
            self.live_stream_closed();
        }
    }

    pub(crate) async fn run(mut self) {
        emit_network_status(&self.app, &self.profile_id, "connecting", 0);
        while self.is_active() {
            self.collect_live_reconnect().await;
            self.launch_live_reconnect();
            let public = self.current_public();
            self.ensure_wallet_event_stream(&public).await;
            self.drain_wallet_events(&public);
            self.poll_live_stream().await;
        }
        if let Some(task) = self.live_reconnect.take() {
            task.abort();
        }
    }
}
use std::time::Instant;
use tauri::Emitter;
