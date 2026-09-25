use super::{
    model::{CallDirection, CallEvent, CallPhase, CallRecord, CallSignal, SignalDisposition},
    transition::apply_event,
};

const MAX_CALL_RECORDS: usize = 32;

#[derive(Clone, Debug, Default)]
pub(super) struct CallStore {
    calls: Vec<CallRecord>,
}

impl CallStore {
    pub(super) fn active(&self) -> Option<&CallRecord> {
        self.calls.iter().rev().find(|call| call.phase.is_active())
    }
    pub(super) fn visible(&self) -> Option<&CallRecord> {
        self.calls.iter().rev().find(|call| call.phase.is_visible())
    }
    pub(super) fn get(&self, call_id: &str) -> Option<&CallRecord> {
        self.calls.iter().find(|call| call.call_id == call_id)
    }
    pub(super) fn find_by_bootstrap_request(&self, request_id: &str) -> Option<&CallRecord> {
        self.calls.iter().find(|call| {
            call.phase.is_active() && call.bootstrap_request_id.as_deref() == Some(request_id)
        })
    }

    pub(super) fn begin_outgoing(
        &mut self,
        call_id: String,
        chat_id: String,
        peer_address: String,
        peer_hydra_id: String,
        peer_label: String,
    ) -> Result<(), String> {
        if self.active().is_some() {
            return Err("Another call is already active.".into());
        }
        self.calls.push(CallRecord {
            call_id,
            chat_id: Some(chat_id),
            peer_address,
            peer_hydra_id,
            peer_label,
            direction: CallDirection::Outgoing,
            phase: CallPhase::OutgoingRinging,
            muted: false,
            bootstrap_request_id: None,
            error: None,
        });
        self.trim();
        Ok(())
    }

    pub(super) fn receive_signal(&mut self, signal: &CallSignal) -> SignalDisposition {
        if self.signal_targets_closed_call(signal) {
            return SignalDisposition::Duplicate;
        }
        if signal.action == "request" {
            return self.receive_request(signal);
        }
        remote_signal_event(&signal.action)
            .map(|event| self.remote_event(&signal.call_id, event))
            .unwrap_or(SignalDisposition::Ignored)
    }

    fn signal_targets_closed_call(&self, signal: &CallSignal) -> bool {
        self.get(&signal.call_id)
            .is_some_and(|existing| !existing.phase.is_active())
    }

    fn receive_request(&mut self, signal: &CallSignal) -> SignalDisposition {
        if self.get(&signal.call_id).is_some() {
            return SignalDisposition::Duplicate;
        }
        if self.active().is_some() {
            return SignalDisposition::Busy;
        }
        self.calls.push(CallRecord {
            call_id: signal.call_id.clone(),
            chat_id: None,
            peer_address: signal.peer_address.clone(),
            peer_hydra_id: signal.peer_hydra_id.clone(),
            peer_label: signal.peer_label.clone(),
            direction: CallDirection::Incoming,
            phase: CallPhase::IncomingRinging,
            muted: false,
            bootstrap_request_id: None,
            error: None,
        });
        self.trim();
        SignalDisposition::Applied
    }

    fn remote_event(&mut self, call_id: &str, event: CallEvent) -> SignalDisposition {
        let Some(call) = self.calls.iter_mut().find(|call| call.call_id == call_id) else {
            return SignalDisposition::Ignored;
        };
        if !call.phase.is_active() {
            return SignalDisposition::Duplicate;
        }
        let error = (event == CallEvent::Fail).then(|| "Call declined.".to_string());
        if apply_event(call, event, error).is_ok() {
            SignalDisposition::Applied
        } else {
            SignalDisposition::Ignored
        }
    }

    pub(super) fn attach_chat(&mut self, call_id: &str, chat_id: String) -> Result<(), String> {
        self.get_mut(call_id)?.chat_id = Some(chat_id);
        Ok(())
    }
    pub(super) fn set_bootstrap_request(
        &mut self,
        call_id: &str,
        request_id: Option<String>,
    ) -> Result<(), String> {
        self.get_mut(call_id)?.bootstrap_request_id = request_id;
        Ok(())
    }
    pub(super) fn event(&mut self, call_id: &str, event: CallEvent) -> Result<(), String> {
        let call = self.get_mut(call_id)?;
        apply_event(call, event, None)
    }
    pub(super) fn fail(&mut self, call_id: &str, error: String) -> Result<(), String> {
        let call = self.get_mut(call_id)?;
        apply_event(call, CallEvent::Fail, Some(error))
    }
    pub(super) fn reset(&mut self) {
        self.calls.clear();
    }

    fn get_mut(&mut self, call_id: &str) -> Result<&mut CallRecord, String> {
        self.calls
            .iter_mut()
            .find(|call| call.call_id == call_id)
            .ok_or_else(|| "Call no longer exists.".to_string())
    }
    fn trim(&mut self) {
        if self.calls.len() > MAX_CALL_RECORDS {
            self.calls.drain(0..self.calls.len() - MAX_CALL_RECORDS);
        }
    }
}

fn remote_signal_event(action: &str) -> Option<CallEvent> {
    match action {
        "accept" => Some(CallEvent::AcceptRemote),
        "decline" => Some(CallEvent::Fail),
        "cancel" | "hangup" => Some(CallEvent::Hangup),
        _ => None,
    }
}
