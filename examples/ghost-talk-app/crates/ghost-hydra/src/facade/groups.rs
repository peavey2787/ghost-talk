#[cfg(feature = "upstream")]
use super::hydra_facade::{GroupGuard, GroupMemberBootstrap};
#[cfg(all(test, feature = "upstream"))]
use super::{hydra_facade::map_profile, runtime::StegoProfile};
#[cfg(feature = "upstream")]
impl GroupGuard {
    pub fn from_epoch(
        group_id: [u8; 32],
        broadcast: bool,
        epoch: u64,
        state_version: u64,
        members: &[GroupMemberBootstrap],
        epoch_key: [u8; 32],
    ) -> Result<Self, String> {
        let mode = if broadcast {
            hydra_group::GroupMode::Broadcast
        } else {
            hydra_group::GroupMode::Interactive
        };
        let mut roster = Vec::with_capacity(members.len());
        for m in members {
            roster.push(hydra_group::RosterEntry {
                member_id: hydra_group::MemberId(m.member_id),
                device_identity_fingerprint: hydra_core::types::IdentityFingerprint(m.fingerprint),
                role: parse_role(&m.role, mode)?,
                status: hydra_group::MemberStatus::Active,
                tree_leaf_slot: m.leaf,
                joined_epoch: hydra_core::types::Epoch(epoch),
                removed_epoch: hydra_core::types::Epoch(0),
            });
        }
        let governor = roster
            .iter()
            .find(|m| m.role == hydra_group::GroupRole::Moderator)
            .ok_or_else(|| "group epoch requires a moderator/governor".to_string())?
            .member_id;
        let mut state = hydra_group::GroupState::new_validated(hydra_group::GroupStateConfig {
            group_id: hydra_core::types::GroupId(group_id),
            mode,
            mechanism: mode.required_mechanism(),
            epoch: hydra_core::types::Epoch(epoch),
            state_version: hydra_group::StateVersion(state_version),
            governance_policy: hydra_group::GovernancePolicy::single_signer(governor),
            mode_policy: hydra_group::ModePolicy::default(),
            roster,
        })
        .map_err(group_error)?;
        state
            .install_epoch_sender_chains(&hydra_core::types::Secret32(epoch_key))
            .map_err(group_error)?;
        Ok(Self { state })
    }

    pub fn can_send(&self, member: [u8; 32]) -> bool {
        self.state
            .require_sender(hydra_group::MemberId(member))
            .is_ok()
    }

    pub fn seal(&mut self, sender: [u8; 32], content: &[u8]) -> Result<Vec<u8>, String> {
        self.state
            .seal_group_data(hydra_group::MemberId(sender), content)
            .map(|m| m.envelope)
            .map_err(group_error)
    }

    pub fn open(&mut self, envelope: &[u8]) -> Result<Vec<u8>, String> {
        self.state
            .open_group_data(envelope)
            .map(|m| m.content)
            .map_err(group_error)
    }
}

#[cfg(feature = "upstream")]
pub(crate) fn group_error(error: hydra_group::GroupError) -> String {
    format!("HYDRA group error: {error:?}")
}

#[cfg(feature = "upstream")]
pub(crate) fn parse_role(
    role: &str,
    mode: hydra_group::GroupMode,
) -> Result<hydra_group::GroupRole, String> {
    let normalized = role.to_ascii_lowercase();
    let roles = [
        ("moderator", hydra_group::GroupRole::Moderator),
        ("presenter", hydra_group::GroupRole::Presenter),
        ("audience", hydra_group::GroupRole::Audience),
        ("member", hydra_group::GroupRole::Member),
    ];
    let role = roles
        .into_iter()
        .find_map(|(name, role)| (name == normalized).then_some(role))
        .ok_or_else(|| "unknown group role".to_string())?;
    if !role.is_active_in_mode(mode) {
        return Err("role is invalid for this HYDRA group mode".into());
    }
    Ok(role)
}

#[cfg(all(test, feature = "upstream"))]
mod stego_integration_tests {
    use super::{map_profile, StegoProfile};

    #[test]
    fn current_hydra_deterministic_stego_roundtrips_compact_carrier_bytes() {
        let payload = b"GHOST-TALK-HYDRA-COMPACT-ENVELOPE";
        let stego = hydra_stego::Stego::new();
        let profile = map_profile(StegoProfile::Deterministic).expect("deterministic profile");
        let cover = stego
            .encode(payload, profile)
            .expect("deterministic cover encode");
        assert!(!cover.is_empty());
        assert_ne!(cover.as_bytes(), payload);
        let decoded = stego
            .decode(&cover, profile)
            .expect("deterministic cover decode");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn current_hydra_model_free_profile_is_exactly_deterministic() {
        assert_eq!(
            map_profile(StegoProfile::Deterministic).expect("deterministic profile"),
            hydra_stego::StegoProfile::Deterministic,
        );
    }
}
