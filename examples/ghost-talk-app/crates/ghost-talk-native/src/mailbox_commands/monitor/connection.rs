use super::super::{
    delivery_ack::WalletMonitorWorker,
    gateway::{live_retry_delay, monitor_generation_is_active, sync_network_status},
    gateway_helpers::start_live_block_stream,
    send_state::{LiveBlockStream, WalletLiveEvent},
};
impl WalletMonitorWorker {
    pub(crate) fn is_active(&self) -> bool {
        monitor_generation_is_active(&self.generations, &self.profile_id, self.generation)
    }

    pub(crate) fn sync_network_status(&mut self, attempts: u32) {
        sync_network_status(
            &self.app,
            &self.profile_id,
            self.carrier_connected,
            self.wallet_subscriptions_connected,
            attempts,
            &mut self.last_network_status,
        );
    }

    pub(crate) fn current_public(&self) -> WalletPublic {
        self.publics
            .lock()
            .ok()
            .and_then(|publics| publics.get(&self.profile_id).cloned())
            .unwrap_or_else(|| self.public.clone())
    }

    pub(crate) fn launch_live_reconnect(&mut self) {
        if self.live_stream.is_some()
            || self.live_reconnect.is_some()
            || Instant::now() < self.next_live_retry
        {
            return;
        }
        self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
        self.carrier_connected = false;
        let status = if self.reconnect_attempts == 1 {
            "connecting"
        } else {
            "reconnecting"
        };
        if status == "reconnecting" || self.last_network_status == "connected" {
            self.sync_network_status(self.reconnect_attempts);
        }
        let public = self.public.clone();
        let endpoint = self.wrpc_endpoint.clone();
        let profile = self.profile_id.clone();
        let gateway = self.gateway.clone();
        self.live_reconnect = Some(tokio::spawn(async move {
            start_live_block_stream(&profile, &public, endpoint.as_deref(), &gateway).await
        }));
    }

    pub(crate) async fn collect_live_reconnect(&mut self) {
        let finished = self
            .live_reconnect
            .as_ref()
            .is_some_and(|task| task.is_finished());
        if !finished {
            return;
        }
        let Some(task) = self.live_reconnect.take() else {
            return;
        };
        self.apply_live_reconnect_result(task.await);
    }

    fn apply_live_reconnect_result(
        &mut self,
        result: Result<Result<LiveBlockStream, String>, tokio::task::JoinError>,
    ) {
        match result {
            Ok(Ok(live)) => self.live_reconnect_succeeded(live),
            Ok(Err(error)) => {
                self.live_reconnect_failed("warn", "live-block-stream-connect-failed", error)
            }
            Err(error) => self.live_reconnect_failed(
                "error",
                "live-block-stream-task-failed",
                error.to_string(),
            ),
        }
    }

    pub(crate) fn live_reconnect_succeeded(&mut self, live: LiveBlockStream) {
        self.live_stream = Some(live);
        self.carrier_connected = true;
        self.reconnect_attempts = 0;
        self.next_live_retry = Instant::now();
        self.sync_network_status(0);
    }

    pub(crate) fn live_reconnect_failed(&mut self, level: &str, event: &str, error: String) {
        let delay = live_retry_delay(self.reconnect_attempts);
        crate::debug_log::record(
            level,
            "kaspa",
            event,
            format!(
                "profile={} attempt={} retry_ms={} error={}",
                self.profile_id,
                self.reconnect_attempts,
                delay.as_millis(),
                error
            ),
        );
        self.next_live_retry = Instant::now() + delay;
        self.carrier_connected = false;
        self.sync_network_status(self.reconnect_attempts);
    }

    pub(crate) async fn ensure_wallet_event_stream(&mut self, public: &WalletPublic) {
        if self.wallet_events.is_some() {
            return;
        }
        match self.start_wallet_event_stream(public).await {
            Ok(stream) => self.wallet_events = Some(stream),
            Err(error) => self.wallet_event_stream_failed(error),
        }
    }

    async fn start_wallet_event_stream(
        &self,
        public: &WalletPublic,
    ) -> Result<crate::kaspa_wallet_events::WalletEventStream, String> {
        // Follow the exact node selected by the shared Portal gateway so UTXO
        // observations cannot race transaction planning on a different node.
        let portal = self
            .gateway
            .portal(public, self.wrpc_endpoint.as_deref())
            .await?;
        let endpoint = portal.endpoint()?;
        let addresses = public.all_addresses().cloned().collect::<Vec<_>>();
        crate::kaspa_wallet_events::start(&public.network, Some(endpoint.as_str()), &addresses)
    }

    pub(crate) fn wallet_event_stream_failed(&mut self, error: String) {
        self.wallet_subscriptions_connected = false;
        crate::debug_log::record(
            "warn",
            "kaspa",
            "wallet-event-stream-start-failed",
            format!("profile={} error={}", self.profile_id, error),
        );
        self.sync_network_status(self.reconnect_attempts.saturating_add(1));
    }

    pub(crate) fn publish_wallet_snapshot(&self) {
        let _ = self.app.emit(
            "ghost://wallet-live",
            WalletLiveEvent {
                profile_id: self.profile_id.clone(),
                snapshot: self.snapshot.clone(),
                checkpoint: self.checkpoint.clone(),
                mailbox: Vec::new(),
            },
        );
    }

    pub(crate) fn handle_changed_transactions(&self, transaction_ids: Vec<String>) {
        if transaction_ids.is_empty() {
            return;
        }
        let result = self
            .app
            .state::<crate::wallet_commands::WalletRuntimeState>()
            .note_utxo_transactions(&self.profile_id, transaction_ids);
        if let Err(error) = result {
            crate::debug_log::record("warn", "kaspa", "wallet-utxo-event-publish-failed", error);
        }
    }
}
use ghost_kaspa::wallet::WalletPublic;
use std::time::Instant;
use tauri::{Emitter, Manager};
