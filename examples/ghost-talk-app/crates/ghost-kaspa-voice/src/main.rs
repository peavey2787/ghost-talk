use ghost_talk::{VoiceChunk, VoiceCodec};

fn main() {
    let chunk = VoiceChunk::new(
        0x1234,
        0,
        VoiceCodec::OpusWebM,
        b"complete-opus-window".to_vec(),
    );
    let encoded = chunk.encode().expect("encode voice chunk");
    let decoded = VoiceChunk::decode(&encoded).expect("decode voice chunk");
    assert_eq!(decoded, chunk);
    println!("complete Ghost Talk VoiceChunk ready for host transport");
}
