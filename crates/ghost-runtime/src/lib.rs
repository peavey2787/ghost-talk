#![forbid(unsafe_code)]
use ghost_accounts::AccountRegistry;
use ghost_contacts::{AnonymousPolicy, Contact};
use ghost_kaspa::UtxoReservations;
use ghost_rooms::Room;
use ghost_sync::SyncState;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub anonymous: AnonymousPolicy,
    pub stego_default: bool,
    pub auto_route: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            anonymous: AnonymousPolicy::RequestsOnly,
            stego_default: false,
            auto_route: true,
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    pub accounts: AccountRegistry,
    pub contacts: Vec<Contact>,
    pub rooms: Vec<Room>,
    pub sync: SyncState,
    pub reservations: UtxoReservations,
    pub settings: Settings,
}
impl RuntimeSnapshot {
    pub fn seal(&self, password: &str) -> Result<ghost_storage::SealedVault, String> {
        let b = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        ghost_storage::seal(password, &b)
    }
    pub fn open(password: &str, v: &ghost_storage::SealedVault) -> Result<Self, String> {
        let b = ghost_storage::open(password, v)?;
        serde_json::from_slice(&b).map_err(|e| e.to_string())
    }
}
pub struct DcutrContract;
impl DcutrContract {
    pub fn requires_relay_coordination() -> bool {
        true
    }
    pub fn is_kaspa_signaling_replacement() -> bool {
        false
    }
}
