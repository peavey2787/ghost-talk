use serde::{Deserialize, Serialize};

/// Current KaChat/Kasia handshake plaintext.
///
/// KaChat accepts camelCase and snake_case for the routing fields and currently
/// writes both spellings. Ghost Talk mirrors that wire contract so handshakes
/// survive cross-client version skew without inventing a second format.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasiaHandshake {
    pub message_type: Option<String>,
    pub alias: Option<String>,
    pub public_key: Option<String>,
    pub timestamp: u64,
    pub conversation_id: Option<String>,
    pub version: Option<u32>,
    pub recipient_address: Option<String>,
    pub send_to_recipient: Option<bool>,
    pub is_response: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireHandshake<'a> {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    message_type: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_key: &'a Option<String>,
    timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: &'a Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recipient_address: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    send_to_recipient: &'a Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_response: &'a Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_key_compat: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id_compat: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recipient_address_compat: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    send_to_recipient_compat: &'a Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_response_compat: &'a Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadHandshake {
    #[serde(rename = "type")]
    message_type: Option<String>,
    alias: Option<String>,
    public_key: Option<String>,
    #[serde(default)]
    timestamp: u64,
    conversation_id: Option<String>,
    version: Option<u32>,
    recipient_address: Option<String>,
    send_to_recipient: Option<bool>,
    is_response: Option<bool>,
    #[serde(rename = "public_key")]
    public_key_compat: Option<String>,
    #[serde(rename = "conversation_id")]
    conversation_id_compat: Option<String>,
    #[serde(rename = "recipient_address")]
    recipient_address_compat: Option<String>,
    #[serde(rename = "send_to_recipient")]
    send_to_recipient_compat: Option<bool>,
    #[serde(rename = "is_response")]
    is_response_compat: Option<bool>,
}

impl KasiaHandshake {
    pub fn request(
        alias: Option<String>,
        public_key: String,
        timestamp: u64,
        conversation_id: String,
        recipient_address: String,
    ) -> Self {
        Self {
            message_type: Some("handshake".into()),
            alias: normalized_alias(alias),
            public_key: Some(public_key),
            timestamp,
            conversation_id: Some(conversation_id),
            version: Some(1),
            recipient_address: Some(recipient_address),
            send_to_recipient: Some(true),
            is_response: None,
        }
    }

    pub fn response(mut self) -> Self {
        self.is_response = Some(true);
        self
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        validate_alias(self.alias.as_deref())?;
        validate_public_key(self.public_key.as_deref())?;
        let wire = WireHandshake {
            message_type: &self.message_type,
            alias: &self.alias,
            public_key: &self.public_key,
            timestamp: self.timestamp,
            conversation_id: &self.conversation_id,
            version: &self.version,
            recipient_address: &self.recipient_address,
            send_to_recipient: &self.send_to_recipient,
            is_response: &self.is_response,
            public_key_compat: &self.public_key,
            conversation_id_compat: &self.conversation_id,
            recipient_address_compat: &self.recipient_address,
            send_to_recipient_compat: &self.send_to_recipient,
            is_response_compat: &self.is_response,
        };
        let value = serde_json::to_value(wire)
            .map_err(|error| format!("Kasia handshake encode failed: {error}"))?;
        let mut object = value.as_object().cloned().unwrap_or_default();
        rename_compat(&mut object);
        serde_json::to_vec(&object)
            .map_err(|error| format!("Kasia handshake encode failed: {error}"))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let raw: ReadHandshake = serde_json::from_slice(bytes)
            .map_err(|error| format!("Kasia handshake decode failed: {error}"))?;
        let value = Self {
            message_type: raw.message_type,
            alias: normalized_alias(raw.alias),
            public_key: raw.public_key.or(raw.public_key_compat),
            timestamp: raw.timestamp,
            conversation_id: raw.conversation_id.or(raw.conversation_id_compat),
            version: raw.version,
            recipient_address: raw.recipient_address.or(raw.recipient_address_compat),
            send_to_recipient: raw.send_to_recipient.or(raw.send_to_recipient_compat),
            is_response: raw.is_response.or(raw.is_response_compat),
        };
        validate_alias(value.alias.as_deref())?;
        validate_public_key(value.public_key.as_deref())?;
        Ok(value)
    }
}

fn normalized_alias(alias: Option<String>) -> Option<String> {
    alias.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn validate_alias(alias: Option<&str>) -> Result<(), String> {
    if alias.is_some_and(|value| value.len() > 64) {
        return Err("Kasia alias must be at most 64 bytes".into());
    }
    Ok(())
}

fn validate_public_key(public_key: Option<&str>) -> Result<(), String> {
    let Some(public_key) = public_key else {
        return Ok(());
    };
    let bytes = hex::decode(public_key)
        .map_err(|_| "Kasia handshake public key must be hex".to_string())?;
    if bytes.len() != 33 || !matches!(bytes[0], 0x02 | 0x03) {
        return Err("Kasia handshake public key must be a compressed secp256k1 key".into());
    }
    Ok(())
}

fn rename_compat(object: &mut serde_json::Map<String, serde_json::Value>) {
    const KEYS: [(&str, &str); 5] = [
        ("publicKeyCompat", "public_key"),
        ("conversationIdCompat", "conversation_id"),
        ("recipientAddressCompat", "recipient_address"),
        ("sendToRecipientCompat", "send_to_recipient"),
        ("isResponseCompat", "is_response"),
    ];
    for (generated, wire) in KEYS {
        if let Some(value) = object.remove(generated) {
            object.insert(wire.into(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_both_current_and_compatibility_keys() {
        let key = format!("02{}", "11".repeat(32));
        let value = KasiaHandshake::request(
            Some("Alice".into()),
            key.clone(),
            7,
            "conversation".into(),
            "kaspa:q".into(),
        );
        let json: serde_json::Value = serde_json::from_slice(&value.encode().unwrap()).unwrap();
        assert_eq!(json["publicKey"], key);
        assert_eq!(json["public_key"], json["publicKey"]);
        assert_eq!(json["conversationId"], "conversation");
        assert_eq!(json["conversation_id"], "conversation");
        assert!(json.get("isResponse").is_none());
        assert!(json.get("is_response").is_none());
    }

    #[test]
    fn api_serde_round_trip_preserves_handshake() {
        let value = KasiaHandshake::request(
            Some("Alice".into()),
            format!("02{}", "11".repeat(32)),
            7,
            "conversation".into(),
            "kaspa:q".into(),
        );
        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("\"publicKey\""));
        assert!(!json.contains("public_key"));
        let decoded: KasiaHandshake = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn reads_snake_case_only_peer() {
        let bytes = br#"{"timestamp":9,"conversation_id":"c","recipient_address":"kaspa:q","is_response":true}"#;
        let value = KasiaHandshake::decode(bytes).unwrap();
        assert_eq!(value.conversation_id.as_deref(), Some("c"));
        assert_eq!(value.is_response, Some(true));
    }

    #[test]
    fn rejects_non_compressed_sender_public_key() {
        let bytes = br#"{"publicKey":"04deadbeef","timestamp":9}"#;
        assert!(KasiaHandshake::decode(bytes).is_err());
    }
}
