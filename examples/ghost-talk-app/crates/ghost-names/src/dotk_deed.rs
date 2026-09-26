use kaspa_portal::primitives::address::{
    encode_address_for_network, script_hash, AddressType, KaspaNetwork, MAX_ADDR_LEN,
};
use serde::Deserialize;

pub const DOTK_REGISTRY: &str = "ee2128c03dfac7f6d74734bb3c879bd999434c47a55945b8a6daae2a1e4a21de";
pub const DOTK_BOND_SOMPI: u64 = 100_000_000;
const STATE_LEN: usize = 103;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DerivedDeed {
    pub name: String,
    pub address: Option<String>,
    pub deed_address: String,
    pub script_public_key: String,
    pub bond: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Deployment {
    registry_covenant_id: String,
    deed_template_hash: String,
    state_offset: usize,
    state_length: usize,
    bond: u64,
    bytecode: String,
}

pub(crate) fn normalize_dotk(input: &str) -> Result<String, String> {
    let normalized = input.trim().to_ascii_lowercase();
    if !normalized.ends_with(".k") {
        return Err("Enter a complete dot.k name, for example name.k.".into());
    }
    let bare = &normalized[..normalized.len() - 2];
    validate_bare_name(bare)?;
    Ok(format!("{bare}.k"))
}

pub(crate) fn bare_dotk(input: &str) -> Result<String, String> {
    let normalized = normalize_dotk(input)?;
    Ok(normalized[..normalized.len() - 2].to_string())
}

fn validate_bare_name(name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= 32
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
    if valid {
        Ok(())
    } else {
        Err("Invalid dot.k name. Use 1-32 lowercase letters, digits, or internal hyphens.".into())
    }
}

pub(crate) fn derive_deed(
    name: &str,
    owner_type: u8,
    owner_hex: &str,
) -> Result<DerivedDeed, String> {
    validate_bare_name(name)?;
    let owner =
        hex::decode(owner_hex).map_err(|_| "dot.k owner is not valid hexadecimal".to_string())?;
    if owner.len() != 32 || owner.iter().all(|byte| *byte == 0) {
        return Err("dot.k owner must be one non-zero 32-byte value".into());
    }
    let address = owner_address(owner_type, &owner, owner_hex)?;
    let deployment = deployment()?;
    let mut redeem = hex::decode(&deployment.bytecode)
        .map_err(|_| "pinned dot.k bytecode is invalid".to_string())?;
    if redeem.len() < deployment.state_offset + deployment.state_length {
        return Err("pinned dot.k deployment is internally inconsistent".into());
    }
    let state = active_state(name, owner_type, &owner)?;
    redeem[deployment.state_offset..deployment.state_offset + STATE_LEN].copy_from_slice(&state);
    let hash = script_hash(&redeem);
    // Standard P2SH lock: OP_BLAKE2B OP_DATA32 <hash> OP_EQUAL.
    let mut script = Vec::with_capacity(35);
    script.extend_from_slice(&[0xaa, 0x20]);
    script.extend_from_slice(&hash);
    script.push(0x87);
    Ok(DerivedDeed {
        name: name.to_string(),
        address,
        deed_address: mainnet_address(&hash, AddressType::P2sh)?,
        script_public_key: hex::encode(script),
        bond: deployment.bond,
    })
}

fn owner_address(owner_type: u8, owner: &[u8], owner_hex: &str) -> Result<Option<String>, String> {
    let address = match owner_type {
        0 => {
            secp256k1::XOnlyPublicKey::from_slice(owner)
                .map_err(|_| "dot.k Schnorr owner key is invalid".to_string())?;
            Some(mainnet_address(owner, AddressType::P2pk)?)
        }
        3 => Some(mainnet_address(owner, AddressType::P2sh)?),
        4 => covenant_owner(owner_hex)?,
        0x85 | 0x86 => {
            let mut compressed = Vec::with_capacity(33);
            compressed.push(0x02 | (owner_type & 1));
            compressed.extend_from_slice(owner);
            secp256k1::PublicKey::from_slice(&compressed)
                .map_err(|_| "dot.k ECDSA owner key is invalid".to_string())?;
            Some(mainnet_address(&compressed, AddressType::P2pkEcdsa)?)
        }
        _ => return Err("unsupported dot.k owner type".into()),
    };
    Ok(address)
}

fn mainnet_address(payload: &[u8], kind: AddressType) -> Result<String, String> {
    let mut out = [0u8; MAX_ADDR_LEN];
    let len = encode_address_for_network(payload, kind, KaspaNetwork::Mainnet, &mut out);
    std::str::from_utf8(&out[..len])
        .ok()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "could not encode dot.k address".to_string())
}

fn covenant_owner(owner_hex: &str) -> Result<Option<String>, String> {
    if owner_hex.eq_ignore_ascii_case(DOTK_REGISTRY) {
        return Err("dot.k covenant owner cannot be the registry itself".into());
    }
    Ok(None)
}

fn active_state(name: &str, owner_type: u8, owner: &[u8]) -> Result<[u8; STATE_LEN], String> {
    let mut state = Vec::with_capacity(STATE_LEN);
    state.extend_from_slice(&[1, 2, 32]);
    state.extend_from_slice(blake3::hash(name.as_bytes()).as_bytes());
    state.extend_from_slice(&[1, owner_type, 32]);
    state.extend_from_slice(owner);
    state.push(32);
    state.extend_from_slice(name.as_bytes());
    if state.len() > STATE_LEN {
        return Err("dot.k state exceeds the pinned v4 layout".into());
    }
    state.resize(STATE_LEN, 0);
    state
        .try_into()
        .map_err(|_| "dot.k state encoding failed".into())
}

fn deployment() -> Result<Deployment, String> {
    let value: Deployment = serde_json::from_str(include_str!("dotk_mainnet.json"))
        .map_err(|_| "pinned dot.k deployment manifest is invalid".to_string())?;
    let valid_hash = value.deed_template_hash.len() == 64
        && value
            .deed_template_hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit());
    if value.registry_covenant_id != DOTK_REGISTRY
        || value.state_offset != 1
        || value.state_length != STATE_LEN
        || value.bond != DOTK_BOND_SOMPI
        || !valid_hash
    {
        return Err(
            "pinned dot.k deployment manifest does not match the supported v4 deployment".into(),
        );
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "079ab96f3b42f1b3667010e6d855172bb8e905e3369fe5ad57b662c4bc365449";

    #[test]
    fn dotk_live_mainnet_vector_matches() {
        let result = derive_deed("21millioncoven", 0, OWNER).unwrap();
        assert_eq!(
            result.address.as_deref(),
            Some("kaspa:qqre4wt08dp0rvmxwqgwdkz4zu4m36g9uvmfledd27mx939uxe2yjpmmd37zq")
        );
        assert_eq!(
            result.deed_address,
            "kaspa:pz7hc3eyu6z6f6xgmp5e843a7xpwqag6rywp3s8r96a0yvg0kgy2xcazlrru8"
        );
        assert_eq!(
            result.script_public_key,
            "aa20bd7c4724e685a4e8c8d86993d63df182e0751a191c18c0e32ebaf2310fb208a387"
        );
    }

    #[test]
    fn dotk_name_grammar_is_strict_ascii() {
        assert_eq!(normalize_dotk("  Name-1.K ").unwrap(), "name-1.k");
        for name in [
            "x",
            ".k",
            "-bad.k",
            "bad-.k",
            "bad_name.k",
            "téšt.k",
            "a.b.k",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.k",
        ] {
            assert!(normalize_dotk(name).is_err(), "{name}");
        }
    }

    #[test]
    fn covenant_owned_name_has_no_payment_address() {
        let result = derive_deed("owned", 4, OWNER).unwrap();
        assert!(result.address.is_none());
    }
}
