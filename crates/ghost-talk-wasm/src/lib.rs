#![forbid(unsafe_code)]

#[cfg(any(target_arch = "wasm32", test))]
use ghost_talk::VoiceCodec;

#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
pub use browser::{
    blob_to_bytes, create_opus_media_recorder, microphone_stream, preferred_opus_mime,
    stop_stream_tracks, BrowserVoiceReceiver, BrowserVoiceSender,
};

/// Perform raw browser property lookup without imposing subsystem-specific error text.
///
/// Higher-level browser adapters map the JavaScript exception into their own
/// domain error while sharing this one reflection primitive.
#[cfg(target_arch = "wasm32")]
pub fn browser_property(
    target: &wasm_bindgen::JsValue,
    name: &str,
) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    js_sys::Reflect::get(target, &wasm_bindgen::JsValue::from_str(name))
}

#[cfg(any(target_arch = "wasm32", test))]
fn parse_stream_id(value: &str) -> Result<u128, &'static str> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("stream id must be exactly 16 bytes of hexadecimal");
    }
    u128::from_str_radix(value, 16).map_err(|_| "invalid stream id")
}

#[cfg(any(target_arch = "wasm32", test))]
fn codec_from_id(value: u8) -> Result<VoiceCodec, &'static str> {
    match value {
        1 => Ok(VoiceCodec::OpusWebM),
        2 => Ok(VoiceCodec::OpusOgg),
        _ => Err("unsupported voice codec id"),
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn codec_id(codec: VoiceCodec) -> u8 {
    match codec {
        VoiceCodec::OpusWebM => 1,
        VoiceCodec::OpusOgg => 2,
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use ghost_talk::{VoiceChunk, VOICE_WIRE_VERSION};
    use wasm_bindgen::prelude::*;

    use super::{codec_from_id, codec_id, parse_stream_id};

    #[wasm_bindgen]
    pub struct DecodedVoiceChunk {
        inner: VoiceChunk,
    }

    #[wasm_bindgen]
    impl DecodedVoiceChunk {
        #[wasm_bindgen(getter)]
        pub fn version(&self) -> u8 {
            self.inner.version
        }

        #[wasm_bindgen(getter, js_name = streamId)]
        pub fn stream_id(&self) -> String {
            format!("{:032x}", self.inner.stream_id)
        }

        #[wasm_bindgen(getter)]
        pub fn sequence(&self) -> u64 {
            self.inner.sequence
        }

        #[wasm_bindgen(getter, js_name = codecId)]
        pub fn codec_id(&self) -> u8 {
            codec_id(self.inner.codec)
        }

        #[wasm_bindgen(getter)]
        pub fn payload(&self) -> Vec<u8> {
            self.inner.payload.clone()
        }
    }

    #[wasm_bindgen]
    pub fn voice_wire_version() -> u8 {
        VOICE_WIRE_VERSION
    }

    #[wasm_bindgen]
    pub fn encode_voice_chunk(
        stream_id: &str,
        sequence: u64,
        codec: u8,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, JsValue> {
        let stream_id = parse_stream_id(stream_id).map_err(JsValue::from_str)?;
        let codec = codec_from_id(codec).map_err(JsValue::from_str)?;
        VoiceChunk::new(stream_id, sequence, codec, payload)
            .encode()
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    #[wasm_bindgen]
    pub fn decode_voice_chunk(bytes: &[u8]) -> Result<DecodedVoiceChunk, JsValue> {
        VoiceChunk::decode(bytes)
            .map(|inner| DecodedVoiceChunk { inner })
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_ids_accept_exact_128_bit_hex() {
        let value = "102030405060708090a0b0c0d0e0f001";
        assert_eq!(
            parse_stream_id(value).unwrap(),
            0x1020_3040_5060_7080_90a0_b0c0_d0e0_f001
        );
        assert!(parse_stream_id("abcd").is_err());
        assert!(parse_stream_id("zz2030405060708090a0b0c0d0e0f001").is_err());
    }

    #[test]
    fn codec_ids_match_the_core_wire_contract() {
        assert_eq!(codec_from_id(1).unwrap(), VoiceCodec::OpusWebM);
        assert_eq!(codec_from_id(2).unwrap(), VoiceCodec::OpusOgg);
        assert_eq!(codec_id(VoiceCodec::OpusWebM), 1);
        assert_eq!(codec_id(VoiceCodec::OpusOgg), 2);
        assert!(codec_from_id(3).is_err());
    }
}
