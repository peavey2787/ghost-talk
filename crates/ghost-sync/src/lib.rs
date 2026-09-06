#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncCheckpoint {
    pub daa_score: u64,
    pub selected_tip: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Observation {
    pub txid: String,
    pub daa_score: u64,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncState {
    pub checkpoint: SyncCheckpoint,
    pub seen_txids: BTreeSet<String>,
    pub live_buffer: VecDeque<Observation>,
    pub backfilling: bool,
}

impl SyncState {
    pub fn begin_backfill(&mut self) {
        self.backfilling = true
    }

    pub fn observe_live(&mut self, o: Observation) -> Option<Observation> {
        if self.seen_txids.contains(&o.txid) {
            return None;
        }
        if self.backfilling {
            self.live_buffer.push_back(o);
            None
        } else {
            Some(o)
        }
    }

    pub fn commit(&mut self, o: &Observation) {
        self.seen_txids.insert(o.txid.clone());
        self.checkpoint.daa_score = self.checkpoint.daa_score.max(o.daa_score)
    }

    pub fn finish_backfill(&mut self) -> Vec<Observation> {
        self.backfilling = false;
        self.live_buffer.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_is_buffered_during_backfill() {
        let mut s = SyncState::default();
        s.begin_backfill();
        assert!(s
            .observe_live(Observation {
                txid: "x".into(),
                daa_score: 9,
                payload: vec![],
            })
            .is_none());
        let v = s.finish_backfill();
        assert_eq!(v.len(), 1)
    }
}
