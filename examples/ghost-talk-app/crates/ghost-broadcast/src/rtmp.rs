use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RtmpTransport {
    Rtmp,
    Rtmps,
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct RtmpSecret(String);
impl RtmpSecret {
    pub fn new(value: String) -> Result<Self, String> {
        if value.trim().is_empty() {
            Err("stream key is required".into())
        } else {
            Ok(Self(value))
        }
    }
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for RtmpSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RtmpSecret([REDACTED])")
    }
}

#[derive(Clone, Debug)]
pub struct RtmpDestination {
    pub transport: RtmpTransport,
    pub server: String,
    pub stream_key: RtmpSecret,
}
impl RtmpDestination {
    pub fn new(server: String, stream_key: RtmpSecret) -> Result<Self, String> {
        let transport = if server.starts_with("rtmps://") {
            RtmpTransport::Rtmps
        } else if server.starts_with("rtmp://") {
            RtmpTransport::Rtmp
        } else {
            return Err("RTMP server must begin rtmp:// or rtmps://".into());
        };
        Ok(Self {
            transport,
            server: server.trim_end_matches('/').into(),
            stream_key,
        })
    }
    pub(crate) fn publish_url(&self) -> String {
        format!("{}/{}", self.server, self.stream_key.expose())
    }
    pub fn redacted(&self) -> String {
        format!("{}/[REDACTED]", self.server)
    }
}


pub fn validate_rtmp_configuration(
    server: Option<&str>,
    stream_key: Option<&str>,
) -> Result<(), String> {
    let server = server.map(str::trim).filter(|value| !value.is_empty());
    let stream_key = stream_key.map(str::trim).filter(|value| !value.is_empty());
    match (server, stream_key) {
        (None, None) => Ok(()),
        (Some(_), None) => Err("RTMP stream key is required".into()),
        (None, Some(_)) => Err("RTMP server is required when a stream key is provided".into()),
        (Some(server), Some(stream_key)) => {
            RtmpDestination::new(
                server.to_owned(),
                RtmpSecret::new(stream_key.to_owned())?,
            )?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_secret_never_appears_in_debug_or_redacted_destination() {
        let secret = RtmpSecret::new("super-secret-stream-key".into()).unwrap();
        assert!(!format!("{secret:?}").contains("super-secret-stream-key"));
        let destination =
            RtmpDestination::new("rtmps://stream.example/live".into(), secret).unwrap();
        assert_eq!(
            destination.redacted(),
            "rtmps://stream.example/live/[REDACTED]"
        );
        assert!(!format!("{destination:?}").contains("super-secret-stream-key"));
    }

    #[test]
    fn configuration_requires_a_complete_valid_pair() {
        assert!(validate_rtmp_configuration(None, None).is_ok());
        assert!(validate_rtmp_configuration(Some("rtmps://stream.example/live"), Some("key")).is_ok());
        assert!(validate_rtmp_configuration(Some("rtmps://stream.example/live"), None).is_err());
        assert!(validate_rtmp_configuration(None, Some("key")).is_err());
        assert!(validate_rtmp_configuration(Some("https://stream.example/live"), Some("key")).is_err());
    }

    #[test]
    fn destination_rejects_non_rtmp_transport() {
        let secret = RtmpSecret::new("key".into()).unwrap();
        assert!(RtmpDestination::new("https://stream.example/live".into(), secret).is_err());
    }
}
