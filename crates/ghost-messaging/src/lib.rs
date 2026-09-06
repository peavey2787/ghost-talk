#![forbid(unsafe_code)]
use ghost_core::PacketId;
use ghost_protocol::{CarrierFrame, ChatEvent};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Reassembler {
    packets: BTreeMap<PacketId, Pending>,
}
struct Pending {
    count: u16,
    parts: BTreeMap<u16, Vec<u8>>,
}
impl Reassembler {
    pub fn push(&mut self, raw: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let f = CarrierFrame::decode(raw)?;
        let p = self.packets.entry(f.packet).or_insert_with(|| Pending {
            count: f.count,
            parts: BTreeMap::new(),
        });
        if p.count != f.count {
            return Err("fragment-count conflict".into());
        }
        if let Some(old) = p.parts.get(&f.index) {
            if old != &f.payload {
                return Err("fragment-content conflict".into());
            }
            return Ok(None);
        }
        p.parts.insert(f.index, f.payload);
        if p.parts.len() != p.count as usize {
            return Ok(None);
        }
        let p = self.packets.remove(&f.packet).unwrap();
        let mut out = Vec::new();
        for i in 0..p.count {
            out.extend_from_slice(p.parts.get(&i).ok_or("missing fragment")?)
        }
        Ok(Some(out))
    }
}
pub fn encode_event_for_kaspa(
    event: &ChatEvent,
    opaque_hydra_envelope: &[u8],
) -> Result<Vec<Vec<u8>>, String> {
    let _ = event;
    ghost_protocol::fragment(PacketId::new_random(), opaque_hydra_envelope)
}
