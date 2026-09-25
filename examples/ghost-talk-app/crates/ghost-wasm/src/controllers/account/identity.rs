use crate::model::{Profile, ProfilePatch};

pub(crate) struct PasswordlessUpdate {
    pub(crate) patch: ProfilePatch,
    pub(crate) activated: Option<Profile>,
}

pub(crate) async fn set_passwordless_and_maybe_unlock(
    profile: &Profile,
    password: &str,
    enabled: bool,
) -> Result<PasswordlessUpdate, String> {
    let credential = if enabled { password } else { "" };
    super::set_remembered_unlock(profile, enabled, credential).await?;
    let patch = super::state::auto_login_patch(profile, enabled);
    if !enabled {
        return Ok(PasswordlessUpdate {
            patch,
            activated: None,
        });
    }
    let mut runtime_profile = profile.clone();
    runtime_profile.set_auto_login(true);
    let activated = super::unlock_profile_runtime(runtime_profile, password).await?;
    Ok(PasswordlessUpdate {
        patch,
        activated: Some(activated),
    })
}

pub(crate) fn confirm_recovery_backup(profile: &Profile) -> Profile {
    let mut activated = profile.clone();
    activated.set_recovery_backup_confirmed(true);
    activated
}

#[derive(Clone)]
pub(crate) struct IdentitySetup {
    pub(crate) mode: String,
    pub(crate) label: String,
    pub(crate) password: String,
    pub(crate) mnemonic: String,
    pub(crate) passphrase: String,
    pub(crate) network: String,
    pub(crate) account_path: String,
}

/// Create or restore the initial profile as one account application command.
/// The component supplies form values only; wallet/history/HYDRA coordination
/// and profile initialization stay behind the controller boundary.
pub(crate) async fn create_or_restore_profile(
    input: IdentitySetup,
) -> Result<(Profile, Option<String>), String> {
    let id = crate::random_id()?;
    let mut profile = Profile::new(id, input.label.clone());
    let (wallet, words) = create_or_import_wallet(&input).await?;
    crate::model::WalletStateService::install(&mut profile.wallet, wallet);
    if input.mode == "restore" {
        if let Some(result) = restored_history(&profile).await {
            crate::model::WalletStateService::replace_restored_history(
                &mut profile.wallet,
                result.history,
                result.used_addresses,
                result.recommended_receive_index,
            );
        }
    }
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "wallet initialization did not produce a wallet".to_string())?;
    let hydra = crate::native::initialize_hydra(&profile.id, &input.password, wallet).await?;
    profile.hydra_identity_id = Some(hydra.identity_id);
    profile.set_recovery_backup_confirmed(input.mode == "restore");
    Ok((profile, words))
}

async fn create_or_import_wallet(
    input: &IdentitySetup,
) -> Result<(crate::model::WalletRecord, Option<String>), String> {
    if input.mode == "create" {
        let created = crate::native::create_wallet(
            &input.password,
            &input.passphrase,
            &input.account_path,
            &input.network,
        )
        .await?;
        Ok((
            crate::model::WalletRecord {
                sealed: created.sealed,
                public: created.public,
                ..Default::default()
            },
            Some(created.mnemonic),
        ))
    } else {
        let imported = crate::native::import_wallet(
            &input.password,
            &input.mnemonic,
            &input.passphrase,
            &input.account_path,
            &input.network,
        )
        .await?;
        Ok((
            crate::model::WalletRecord {
                sealed: imported.sealed,
                public: imported.public,
                ..Default::default()
            },
            None,
        ))
    }
}

async fn restored_history(profile: &Profile) -> Option<crate::model::WalletHistoryResult> {
    crate::native::gather_wallet_history(profile, &[])
        .await
        .ok()
}
