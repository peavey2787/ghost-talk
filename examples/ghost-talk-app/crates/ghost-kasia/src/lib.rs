#![forbid(unsafe_code)]

mod api;
#[cfg(feature = "crypto")]
mod cipher;
mod codec;
mod handshake;
mod history;
#[cfg(feature = "native")]
mod indexer;
mod mapping;

pub use api::{KasiaHistoryRequest, KasiaIdentityProjection, KasiaSendRequest};
#[cfg(feature = "crypto")]
pub use cipher::{decrypt_for, encrypt_for, public_key_from_secret, KasiaCipherError};
pub use codec::{
    decode_payload, decode_raw_payload, encode_comm, encode_handshake, encode_payment, KasiaPayload,
};
#[cfg(feature = "native")]
pub use history::{decrypt_indexed_handshake, KasiaConversationHistory};
pub use history::{KasiaMessage, KasiaReceivedHandshake};
#[cfg(feature = "native")]
pub use indexer::{KasiaContextualMessageResponse, KasiaHandshakeResponse, KasiaIndexerClient};
pub use mapping::{KasiaContactMap, KasiaContactMapping};

pub use handshake::KasiaHandshake;
