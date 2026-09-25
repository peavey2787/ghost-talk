use crate::VoiceError;

/// Encoded audio formats supported by the Ghost Talk chunk wire format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum VoiceCodec {
    /// Opus audio carried in a WebM container.
    OpusWebM,
    /// Opus audio carried in an Ogg container.
    OpusOgg,
}

impl VoiceCodec {
    pub(crate) const fn wire_id(self) -> u8 {
        match self {
            Self::OpusWebM => 1,
            Self::OpusOgg => 2,
        }
    }

    pub(crate) fn from_wire_id(value: u8) -> Result<Self, VoiceError> {
        match value {
            1 => Ok(Self::OpusWebM),
            2 => Ok(Self::OpusOgg),
            _ => Err(VoiceError::UnsupportedCodec(value)),
        }
    }

    /// Returns the browser MIME type associated with this codec/container.
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::OpusWebM => "audio/webm;codecs=opus",
            Self::OpusOgg => "audio/ogg;codecs=opus",
        }
    }
}
