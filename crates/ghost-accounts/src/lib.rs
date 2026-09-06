#![forbid(unsafe_code)]

use ghost_core::{AccountId, Network};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub label: String,
    pub network: Network,
    pub derivation_path: String,
    pub auto_login: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountRegistry {
    accounts: BTreeMap<AccountId, AccountIdRecord>,
    auto: Option<AccountId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AccountIdRecord {
    account: Account,
}

impl AccountRegistry {
    pub fn add(&mut self, a: Account) -> Result<(), String> {
        if self.accounts.contains_key(&a.id) {
            return Err("duplicate account".into());
        }
        if a.auto_login {
            self.set_auto(Some(a.id));
        }
        self.accounts.insert(a.id, AccountIdRecord { account: a });
        Ok(())
    }

    pub fn set_auto(&mut self, id: Option<AccountId>) {
        self.auto = id;
        for r in self.accounts.values_mut() {
            r.account.auto_login = Some(r.account.id) == id
        }
    }

    pub fn auto(&self) -> Option<AccountId> {
        self.auto
    }

    pub fn list(&self) -> Vec<Account> {
        self.accounts.values().map(|r| r.account.clone()).collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DerivationPreset {
    KaspaStandard,
    KaspaAccount(u32),
    Custom,
}

impl DerivationPreset {
    pub fn path(self) -> Option<String> {
        match self {
            Self::KaspaStandard => Some("m/44'/111111'/0'/0/0".into()),
            Self::KaspaAccount(n) => Some(format!("m/44'/111111'/{n}'/0/0")),
            Self::Custom => None,
        }
    }
}

pub fn validate_derivation_path(path: &str) -> Result<(), String> {
    let p = path.trim();
    if !p.starts_with("m/") {
        return Err("path must start m/".into());
    }
    if p.len() > 160 {
        return Err("path too long".into());
    }
    for s in p[2..].split('/') {
        let n = s.trim_end_matches('\'');
        if n.is_empty() || n.parse::<u32>().is_err() {
            return Err("invalid derivation segment".into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UnlockThrottle {
    failures: u8,
}

impl UnlockThrottle {
    pub fn record_failure(&mut self) {
        self.failures = self.failures.saturating_add(1)
    }

    pub fn record_success(&mut self) {
        self.failures = 0
    }

    pub fn delay_seconds(&self) -> u64 {
        if self.failures < 3 {
            0
        } else {
            1u64 << ((self.failures - 3).min(6))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_auto_login() {
        let mut r = AccountRegistry::default();
        for i in 0..2 {
            r.add(Account {
                id: ghost_core::Id128([i; 16]),
                label: i.to_string(),
                network: Network::Mainnet,
                derivation_path: DerivationPreset::KaspaStandard.path().unwrap(),
                auto_login: false,
            })
            .unwrap();
        }
        r.set_auto(Some(ghost_core::Id128([0; 16])));
        r.set_auto(Some(ghost_core::Id128([1; 16])));
        assert_eq!(r.list().iter().filter(|a| a.auto_login).count(), 1)
    }

    #[test]
    fn path_validation() {
        assert!(validate_derivation_path("m/44'/111111'/0'/0/0").is_ok());
        assert!(validate_derivation_path("44/0").is_err())
    }
}
