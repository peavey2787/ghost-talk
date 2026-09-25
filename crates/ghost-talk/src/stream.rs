use crate::VoiceError;

/// Random 128-bit identity for one logical sender stream.
pub type StreamId = u128;

pub(crate) fn random_stream_id() -> Result<StreamId, VoiceError> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| VoiceError::RandomnessUnavailable)?;
    Ok(u128::from_le_bytes(bytes))
}
