use super::model::{CallEvent, CallPhase, CallRecord};

pub(super) fn apply_event(
    call: &mut CallRecord,
    event: CallEvent,
    error: Option<String>,
) -> Result<(), String> {
    if let CallEvent::SetMuted(value) = event {
        return set_muted(call, value);
    }
    let next = next_phase(call.phase, event)?;
    call.phase = next;
    call.error = (next == CallPhase::Failed).then_some(error).flatten();
    if matches!(
        next,
        CallPhase::Ending | CallPhase::Ended | CallPhase::Failed
    ) {
        call.bootstrap_request_id = None;
        call.muted = false;
    }
    Ok(())
}

fn set_muted(call: &mut CallRecord, value: bool) -> Result<(), String> {
    if call.phase != CallPhase::Connected {
        return Err(format!(
            "Cannot change mute state while call is {:?}",
            call.phase
        ));
    }
    call.muted = value;
    Ok(())
}

fn next_phase(phase: CallPhase, event: CallEvent) -> Result<CallPhase, String> {
    use CallEvent::*;
    match event {
        AcceptLocal | AcceptRemote | TransportConnected => connection_phase(phase, event),
        Decline | Cancel | Hangup => terminal_end_phase(phase, event),
        CompleteEnd => complete_end_phase(phase, event),
        Fail | Close => completion_phase(phase, event),
        SetMuted(_) => invalid_transition(phase, event),
    }
}

fn connection_phase(phase: CallPhase, event: CallEvent) -> Result<CallPhase, String> {
    use CallEvent::*;
    use CallPhase::*;
    match event {
        AcceptLocal => transition_if(phase, matches!(phase, IncomingRinging), Answering, event),
        AcceptRemote => transition_if(phase, matches!(phase, OutgoingRinging), Connecting, event),
        TransportConnected => transition_if(
            phase,
            matches!(phase, Answering | Connecting),
            Connected,
            event,
        ),
        _ => invalid_transition(phase, event),
    }
}

fn completion_phase(phase: CallPhase, event: CallEvent) -> Result<CallPhase, String> {
    use CallEvent::*;
    use CallPhase::*;
    match event {
        Fail => transition_if(
            phase,
            phase.is_active() || matches!(phase, Ending),
            Failed,
            event,
        ),
        Close => transition_if(phase, matches!(phase, Failed | Ended), Ended, event),
        _ => invalid_transition(phase, event),
    }
}

fn terminal_end_phase(phase: CallPhase, event: CallEvent) -> Result<CallPhase, String> {
    use CallPhase::*;
    if phase.is_active() {
        return Ok(Ending);
    }
    if matches!(phase, Ending | Ended | Failed) {
        return Ok(phase);
    }
    invalid_transition(phase, event)
}

fn complete_end_phase(phase: CallPhase, event: CallEvent) -> Result<CallPhase, String> {
    use CallPhase::*;
    match phase {
        Ending | Ended => Ok(Ended),
        _ => invalid_transition(phase, event),
    }
}

fn transition_if(
    phase: CallPhase,
    allowed: bool,
    next: CallPhase,
    event: CallEvent,
) -> Result<CallPhase, String> {
    if allowed {
        Ok(next)
    } else {
        invalid_transition(phase, event)
    }
}

fn invalid_transition(phase: CallPhase, event: CallEvent) -> Result<CallPhase, String> {
    Err(format!(
        "Invalid call transition from {:?} via {:?}",
        phase, event
    ))
}
