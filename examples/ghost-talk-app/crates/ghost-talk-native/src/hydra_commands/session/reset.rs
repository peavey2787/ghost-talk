use super::session_state::retire_kktp_binding;
use crate::hydra_commands::{
    handshake_admission::reset_hydra_peer_crypto, persist_transport_state,
    runtime_session_queries::clear_peer_handshake_state, HydraProfileRuntime,
};

pub(crate) fn reset_kktp_after_unsent_advance(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
) -> Result<(), String> {
    reset_hydra_peer_crypto(runtime, contact_id)?;
    retire_kktp_binding(runtime, contact_id);
    clear_peer_handshake_state(runtime, contact_id);
    persist_transport_state(runtime)?;
    Ok(())
}
