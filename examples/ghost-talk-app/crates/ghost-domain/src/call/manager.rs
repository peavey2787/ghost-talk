use super::{
    model::{CallEvent, CallRecord, CallSignal, SignalDisposition},
    store::CallStore,
};

/// Sole mutable owner for process-local voice-call state. UI, media, transport,
/// and signaling layers issue commands here and never receive mutable store access.
#[derive(Clone, Debug, Default)]
pub struct CallManager {
    store: CallStore,
}

impl CallManager {
    pub fn active(&self) -> Option<&CallRecord> {
        self.store.active()
    }
    pub fn visible(&self) -> Option<&CallRecord> {
        self.store.visible()
    }
    pub fn get(&self, call_id: &str) -> Option<&CallRecord> {
        self.store.get(call_id)
    }
    pub fn find_by_bootstrap_request(&self, request_id: &str) -> Option<&CallRecord> {
        self.store.find_by_bootstrap_request(request_id)
    }
    pub fn begin_outgoing(
        &mut self,
        call_id: String,
        chat_id: String,
        peer_address: String,
        peer_hydra_id: String,
        peer_label: String,
    ) -> Result<(), String> {
        self.store
            .begin_outgoing(call_id, chat_id, peer_address, peer_hydra_id, peer_label)
    }
    pub fn receive_signal(&mut self, signal: &CallSignal) -> SignalDisposition {
        self.store.receive_signal(signal)
    }
    pub fn attach_chat(&mut self, call_id: &str, chat_id: String) -> Result<(), String> {
        self.store.attach_chat(call_id, chat_id)
    }
    pub fn set_bootstrap_request(
        &mut self,
        call_id: &str,
        request_id: Option<String>,
    ) -> Result<(), String> {
        self.store.set_bootstrap_request(call_id, request_id)
    }
    pub fn event(&mut self, call_id: &str, event: CallEvent) -> Result<(), String> {
        self.store.event(call_id, event)
    }
    pub fn fail(&mut self, call_id: &str, error: String) -> Result<(), String> {
        self.store.fail(call_id, error)
    }
    pub fn reset(&mut self) {
        self.store.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::CallManager;
    use crate::call::{CallEvent, CallPhase, CallSignal, SignalDisposition};

    fn outgoing() -> CallManager {
        let mut manager = CallManager::default();
        manager
            .begin_outgoing(
                "call".into(),
                "chat".into(),
                "kaspatest:peer".into(),
                "hydra".into(),
                "Peer".into(),
            )
            .unwrap();
        manager
    }

    #[test]
    fn hangup_is_local_and_idempotent_after_completion() {
        let mut manager = outgoing();
        manager.event("call", CallEvent::AcceptRemote).unwrap();
        manager
            .event("call", CallEvent::TransportConnected)
            .unwrap();
        manager.event("call", CallEvent::Hangup).unwrap();
        manager.event("call", CallEvent::Hangup).unwrap();
        assert!(manager.active().is_none());
        assert!(manager.visible().is_none());
        assert_eq!(manager.get("call").unwrap().phase, CallPhase::Ending);
        manager.event("call", CallEvent::CompleteEnd).unwrap();
        manager.event("call", CallEvent::CompleteEnd).unwrap();
        manager.event("call", CallEvent::Hangup).unwrap();
        assert_eq!(manager.get("call").unwrap().phase, CallPhase::Ended);
    }

    #[test]
    fn mute_is_explicit_not_toggle_racy() {
        let mut manager = outgoing();
        manager.event("call", CallEvent::AcceptRemote).unwrap();
        manager
            .event("call", CallEvent::TransportConnected)
            .unwrap();
        manager.event("call", CallEvent::SetMuted(true)).unwrap();
        manager.event("call", CallEvent::SetMuted(true)).unwrap();
        assert!(manager.get("call").unwrap().muted);
    }

    #[test]
    fn competing_incoming_call_is_busy() {
        let mut manager = outgoing();
        let signal = CallSignal {
            call_id: "other".into(),
            action: "request".into(),
            peer_address: "kaspatest:other".into(),
            peer_hydra_id: "hydra-other".into(),
            peer_label: "Other".into(),
        };
        assert_eq!(manager.receive_signal(&signal), SignalDisposition::Busy);
    }
}
