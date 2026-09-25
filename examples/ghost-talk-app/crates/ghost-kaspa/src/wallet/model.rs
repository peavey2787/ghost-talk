use kaspa_portal::wallet::account::derivation::WalletData;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub(crate) const ADDRESS_LOOKAHEAD: u32 = 64;
pub(crate) const HARDENED: u32 = 0x8000_0000;

pub const MAILBOX_OUTPUT_SOMPI: u64 = 10_000_000;

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct WalletSecret {
    pub mnemonic: String,
    #[serde(default)]
    pub passphrase: String,
    pub account_path: String,
    pub network: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletPublic {
    pub network: String,
    pub account_path: String,
    pub receive_addresses: Vec<String>,
    pub change_addresses: Vec<String>,
    pub next_receive_index: usize,
    pub next_change_index: usize,
}

impl WalletPublic {
    pub fn projection(&self) -> ghost_domain::wallet::WalletProjection {
        ghost_domain::wallet::WalletProjection {
            network: self.network.clone(),
            account_path: self.account_path.clone(),
            receive_addresses: self.receive_addresses.clone(),
            change_addresses: self.change_addresses.clone(),
            next_receive_index: self.next_receive_index,
            next_change_index: self.next_change_index,
        }
    }

    pub fn from_projection(public: &ghost_domain::wallet::WalletProjection) -> Self {
        Self {
            network: public.network.clone(),
            account_path: public.account_path.clone(),
            receive_addresses: public.receive_addresses.clone(),
            change_addresses: public.change_addresses.clone(),
            next_receive_index: public.next_receive_index,
            next_change_index: public.next_change_index,
        }
    }

    pub fn receive_address(&self) -> Result<&str, String> {
        self.receive_addresses
            .get(self.next_receive_index)
            .map(String::as_str)
            .ok_or_else(|| "receive-address lookahead exhausted".to_string())
    }

    pub fn change_address(&self) -> Result<&str, String> {
        self.change_addresses
            .get(self.next_change_index)
            .map(String::as_str)
            .ok_or_else(|| "change-address lookahead exhausted".to_string())
    }

    pub fn all_addresses(&self) -> impl Iterator<Item = &String> {
        self.receive_addresses
            .iter()
            .chain(self.change_addresses.iter())
    }

    pub fn advance_receive(&mut self) -> Result<(), String> {
        if self.next_receive_index + 1 >= self.receive_addresses.len() {
            return Err("receive-address lookahead exhausted".into());
        }
        self.next_receive_index += 1;
        Ok(())
    }

    pub fn advance_change_if_available(&mut self) -> bool {
        if self.next_change_index + 1 >= self.change_addresses.len() {
            return false;
        }
        self.next_change_index += 1;
        true
    }

    pub fn advance_change(&mut self) -> Result<(), String> {
        if !self.advance_change_if_available() {
            return Err("change-address lookahead exhausted".into());
        }
        Ok(())
    }

    pub fn portal_wallet(&self) -> WalletData {
        WalletData {
            kpub: String::new(),
            receive_addresses: self.receive_addresses.clone(),
            change_addresses: self.change_addresses.clone(),
            next_receive_index: self.next_receive_index,
            next_change_index: self.next_change_index,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CreatedWallet {
    pub mnemonic: String,
    pub public: WalletPublic,
}
