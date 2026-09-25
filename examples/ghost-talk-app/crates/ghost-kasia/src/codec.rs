use base64::Engine;
use serde::{Deserialize, Serialize};

const CURRENT_ROOT: &str = "kchat:1:";
const COMPAT_ROOT: &str = "ciph_msg:1:";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KasiaPayload {
    Handshake(Vec<u8>),
    Comm { alias: String, ciphertext: Vec<u8> },
    Payment(Vec<u8>),
}

/// Handshakes are hex encoded after the ASCII protocol prefix.
pub fn encode_handshake(ciphertext: &[u8]) -> String {
    format!("{CURRENT_ROOT}handshake:{}", hex::encode(ciphertext))
}

pub fn encode_comm(alias: &str, ciphertext: &[u8]) -> Result<String, String> {
    let alias = normalized_alias(alias)?;
    Ok(format!("{CURRENT_ROOT}comm:{alias}:{}", b64(ciphertext)))
}

pub fn encode_payment(ciphertext: &[u8]) -> String {
    format!("{CURRENT_ROOT}pay:{}", b64(ciphertext))
}

/// Decode a raw Kaspa transaction payload. Handshake ciphertext is hex text;
/// contextual/payment ciphertext is base64 text by the Kasia wire contract.
pub fn decode_raw_payload(payload: &[u8]) -> Result<KasiaPayload, String> {
    if let Some(body) = payload.strip_prefix(CURRENT_ROOT.as_bytes()) {
        return decode_raw_body(body, false);
    }
    if let Some(body) = payload.strip_prefix(COMPAT_ROOT.as_bytes()) {
        return decode_raw_body(body, true);
    }
    Err("not a Kasia/KaChat payload".into())
}

pub fn decode_payload(payload: &str) -> Result<KasiaPayload, String> {
    decode_raw_payload(payload.as_bytes())
}

fn decode_raw_body(body: &[u8], compatibility_root: bool) -> Result<KasiaPayload, String> {
    let text =
        std::str::from_utf8(body).map_err(|_| "Kasia payload is not valid UTF-8".to_string())?;
    if let Some(ciphertext) = text.strip_prefix("handshake:").or_else(|| {
        compatibility_root
            .then(|| text.strip_prefix("hs:"))
            .flatten()
    }) {
        return Ok(KasiaPayload::Handshake(
            hex::decode(ciphertext.trim())
                .map_err(|error| format!("invalid Kasia handshake payload: {error}"))?,
        ));
    }
    if let Some(encoded) = text.strip_prefix("pay:") {
        return Ok(KasiaPayload::Payment(decode_binary(encoded)?));
    }
    if let Some(rest) = text.strip_prefix("comm:").or_else(|| {
        compatibility_root
            .then(|| text.strip_prefix("msg:"))
            .flatten()
    }) {
        let (alias, encoded) = rest
            .split_once(':')
            .ok_or_else(|| "Kasia comm payload is missing alias/ciphertext".to_string())?;
        return Ok(KasiaPayload::Comm {
            alias: normalized_alias(alias)?.into(),
            ciphertext: decode_binary(encoded)?,
        });
    }
    Err("unsupported Kasia/KaChat payload kind".into())
}

fn normalized_alias(alias: &str) -> Result<&str, String> {
    let alias = alias.trim();
    if alias.is_empty() || alias.contains(':') || alias.len() > 64 {
        return Err("Kasia alias must be 1-64 bytes and may not contain ':'".into());
    }
    Ok(alias)
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn decode_binary(value: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|error| format!("invalid Kasia encrypted payload: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_contextual_write_and_compatibility_read_match() {
        let wire = encode_comm("bob", b"cipher").unwrap();
        let expected = KasiaPayload::Comm {
            alias: "bob".into(),
            ciphertext: b"cipher".to_vec(),
        };
        assert_eq!(decode_payload(&wire).unwrap(), expected);
        let compatibility = wire.replacen("kchat:1:comm", "ciph_msg:1:comm", 1);
        assert_eq!(decode_payload(&compatibility).unwrap(), expected);
        assert_eq!(
            decode_payload("ciph_msg:1:msg:bob:Y2lwaGVy").unwrap(),
            expected,
        );
    }

    #[test]
    fn handshake_writes_hex_and_round_trips() {
        let wire = encode_handshake(&[0, 1, 0xff, 3]);
        assert_eq!(wire, "kchat:1:handshake:0001ff03");
        let expected = KasiaPayload::Handshake(vec![0, 1, 0xff, 3]);
        assert_eq!(decode_payload(&wire).unwrap(), expected);
        assert_eq!(
            decode_payload("ciph_msg:1:handshake:0001ff03").unwrap(),
            KasiaPayload::Handshake(vec![0, 1, 0xff, 3]),
        );
        assert_eq!(
            decode_payload("ciph_msg:1:hs:0001ff03").unwrap(),
            KasiaPayload::Handshake(vec![0, 1, 0xff, 3]),
        );
    }

    #[test]
    fn rejects_invalid_alias_and_binary_encoding() {
        assert!(encode_comm("bad:alias", b"cipher").is_err());
        assert!(decode_payload("kchat:1:comm:bob:not-base64!").is_err());
        assert!(decode_payload("kchat:1:handshake:not-hex").is_err());
    }
}
