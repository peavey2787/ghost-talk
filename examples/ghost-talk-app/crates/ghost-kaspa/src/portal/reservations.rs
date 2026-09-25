pub(crate) use ghost_core::KaspaAddress;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use std::collections::BTreeMap;

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct Outpoint {
    pub txid: String,
    pub index: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UtxoReservations {
    pub(crate) reserved: BTreeMap<Outpoint, String>,
}

impl UtxoReservations {
    pub fn reserve<I: IntoIterator<Item = Outpoint>>(
        &mut self,
        id: &str,
        items: I,
    ) -> Result<(), String> {
        let items: Vec<_> = items.into_iter().collect();
        if items.iter().any(|x| self.reserved.contains_key(x)) {
            return Err("UTXO already reserved".into());
        }
        for x in items {
            self.reserved.insert(x, id.into());
        }
        Ok(())
    }

    pub fn release(&mut self, id: &str) {
        self.reserved.retain(|_, v| v != id)
    }

    pub fn is_reserved(&self, o: &Outpoint) -> bool {
        self.reserved.contains_key(o)
    }
}

pub fn validate_destination(s: &str) -> Result<KaspaAddress, String> {
    KaspaAddress::parse(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_are_atomic() {
        let o = Outpoint {
            txid: "a".into(),
            index: 0,
        };
        let mut r = UtxoReservations::default();
        r.reserve("a", [o.clone()]).unwrap();
        assert!(r.reserve("b", [o]).is_err())
    }
}
