#![forbid(unsafe_code)]

use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use ghost_core::{ContactId, RoomId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VoiceRoute {
    WebRtc,
    Kaspa,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VoicePreference {
    Automatic,
    KaspaOnly,
}

pub fn route_order(p: VoicePreference) -> &'static [VoiceRoute] {
    match p {
        VoicePreference::Automatic => &[VoiceRoute::WebRtc, VoiceRoute::Kaspa],
        VoicePreference::KaspaOnly => &[VoiceRoute::Kaspa],
    }
}

#[derive(Clone, Debug)]
pub struct MediaKey([u8; 32]);

impl MediaKey {
    pub fn random() -> Self {
        Self(rand::random())
    }

    pub fn from_hydra_export(export: &[u8], context: &[u8]) -> Self {
        let mut h = blake3::Hasher::new_derive_key("ghost-talk/media-key/v1");
        h.update(export);
        h.update(context);
        Self(*h.finalize().as_bytes())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MediaFrame {
    pub room: Option<RoomId>,
    pub sender: ContactId,
    pub sequence: u64,
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

impl MediaFrame {
    pub fn seal(
        key: &MediaKey,
        room: Option<RoomId>,
        sender: ContactId,
        sequence: u64,
        opus: &[u8],
    ) -> Result<Self, String> {
        if opus.len() > 80 * 1024 - 128 {
            return Err("voice frame too large".into());
        }
        let nonce: [u8; 24] = rand::random();
        let cipher = XChaCha20Poly1305::new_from_slice(&key.0).map_err(|e| e.to_string())?;
        let aad = media_aad(room, sender, sequence);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: opus,
                    aad: &aad,
                },
            )
            .map_err(|_| "media encryption failed".to_string())?;
        Ok(Self {
            room,
            sender,
            sequence,
            nonce,
            ciphertext,
        })
    }

    pub fn open(&self, key: &MediaKey) -> Result<Vec<u8>, String> {
        let cipher = XChaCha20Poly1305::new_from_slice(&key.0).map_err(|e| e.to_string())?;
        let aad = media_aad(self.room, self.sender, self.sequence);
        cipher
            .decrypt(
                XNonce::from_slice(&self.nonce),
                Payload {
                    msg: self.ciphertext.as_ref(),
                    aad: &aad,
                },
            )
            .map_err(|_| "media authentication failed".into())
    }
}

fn media_aad(room: Option<RoomId>, sender: ContactId, sequence: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(47);
    aad.extend_from_slice(b"GTVA1");
    match room {
        Some(id) => {
            aad.push(1);
            aad.extend_from_slice(&id.0)
        }
        None => {
            aad.push(0);
            aad.extend_from_slice(&[0u8; 16])
        }
    }
    aad.extend_from_slice(&sender.0);
    aad.extend_from_slice(&sequence.to_le_bytes());
    aad
}

#[derive(Default)]
pub struct ReplayWindow {
    seen: BTreeSet<u64>,
    max: Option<u64>,
}

impl ReplayWindow {
    pub fn accept(&mut self, s: u64) -> bool {
        if self.seen.contains(&s) {
            return false;
        }
        if let Some(m) = self.max {
            if s.saturating_add(2048) < m {
                return false;
            }
        }
        self.seen.insert(s);
        self.max = Some(self.max.map_or(s, |m| m.max(s)));
        let floor = self.max.unwrap().saturating_sub(2048);
        self.seen.retain(|x| *x >= floor);
        true
    }
}

#[derive(Default)]
pub struct JitterBuffer {
    q: VecDeque<MediaFrame>,
}

impl JitterBuffer {
    pub fn push(&mut self, f: MediaFrame) {
        let pos = self
            .q
            .iter()
            .position(|x| x.sequence > f.sequence)
            .unwrap_or(self.q.len());
        self.q.insert(pos, f);
        while self.q.len() > 256 {
            self.q.pop_front();
        }
    }

    pub fn pop(&mut self) -> Option<MediaFrame> {
        self.q.pop_front()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpendBudget {
    pub max_sompi: u64,
    pub spent_sompi: u64,
}

impl SpendBudget {
    pub fn reserve(&mut self, n: u64) -> Result<(), String> {
        if self.spent_sompi.saturating_add(n) > self.max_sompi {
            return Err("Kaspa voice spend ceiling exceeded".into());
        }
        self.spent_sompi += n;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_auth_and_replay() {
        let key = MediaKey::random();
        let sender = ghost_core::Id128([3; 16]);
        let f = MediaFrame::seal(&key, None, sender, 7, b"opus").unwrap();
        assert_eq!(f.open(&key).unwrap(), b"opus");
        let mut altered = f.clone();
        altered.sequence = 8;
        assert!(altered.open(&key).is_err());
        let mut w = ReplayWindow::default();
        assert!(w.accept(7));
        assert!(!w.accept(7))
    }

    #[test]
    fn automatic_prefers_webrtc_then_kaspa() {
        assert_eq!(
            route_order(VoicePreference::Automatic),
            &[VoiceRoute::WebRtc, VoiceRoute::Kaspa]
        )
    }

    #[test]
    fn kaspa_only_has_no_direct_fallback() {
        assert_eq!(
            route_order(VoicePreference::KaspaOnly),
            &[VoiceRoute::Kaspa]
        )
    }
}
