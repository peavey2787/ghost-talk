use crate::model::{WalletCreateResponse, WalletImportResponse, WalletProjection};
use ghost_api::BroadcastResult;
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use serde_json::Value;

use super::{
    kaspa::profile_portal,
    support::util::{decimal, option_endpoint, require_password, required, required_str, supported_network, to_value},
    WALLET_SECRETS,
};

pub(super) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "wallet_send" | "wallet_consolidate" | "wallet_gather_history" => invoke_network(command, args).await,
        _ => invoke_local(command, args),
    }
}

fn invoke_local(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "wallet_create" => create(args),
        "wallet_import" => import(args),
        "wallet_unlock" => unlock(args),
        _ => invoke_local_state(command, args),
    }
}


fn invoke_local_state(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "wallet_lock" => lock(args),
        "wallet_reveal_recovery" => reveal_recovery(args),
        "wallet_next_receive" => next_receive(args),
        other => Err(format!("unknown browser wallet command: {other}")),
    }
}

async fn invoke_network(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "wallet_send" => send(args).await,
        "wallet_consolidate" => consolidate(args).await,
        "wallet_gather_history" => gather_history(args).await,
        other => Err(format!("unknown browser network wallet command: {other}")),
    }
}

fn create(args: &Value) -> Result<Value, String> {
    let password = required_str(args, "password")?;
    require_password(password, "wallet")?;
    let passphrase = required_str(args, "passphrase")?;
    let account_path = required_str(args, "accountPath")?;
    let network = supported_network(required_str(args, "network")?)?;
    let (secret, created) = ghost_kaspa::wallet::generate_wallet(passphrase, account_path, &network)?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &created.public)?;
    to_value(WalletCreateResponse {
        sealed: ghost_storage::seal_json(password, &secret, "wallet vault")?,
        mnemonic: created.mnemonic,
        public: created.public.projection(),
    })
}

fn import(args: &Value) -> Result<Value, String> {
    let password = required_str(args, "password")?;
    require_password(password, "wallet")?;
    let mnemonic = required_str(args, "mnemonic")?;
    let passphrase = required_str(args, "passphrase")?;
    let account_path = required_str(args, "accountPath")?;
    let network = supported_network(required_str(args, "network")?)?;
    let (secret, public) = ghost_kaspa::wallet::import_wallet(mnemonic, passphrase, account_path, &network)?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
    to_value(WalletImportResponse {
        sealed: ghost_storage::seal_json(password, &secret, "wallet vault")?,
        public: public.projection(),
    })
}

fn unlock(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let secret = open_and_validate(args)?;
    WALLET_SECRETS.with(|secrets| {
        secrets.borrow_mut().insert(profile_id.to_owned(), secret);
    });
    required::<WalletProjection>(args, "public").and_then(to_value)
}

fn lock(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    WALLET_SECRETS.with(|secrets| {
        secrets.borrow_mut().remove(profile_id);
    });
    Ok(Value::Null)
}

fn next_receive(args: &Value) -> Result<Value, String> {
    let mut public: WalletPublic = required(args, "public")?;
    public.advance_receive()?;
    to_value(public)
}

fn reveal_recovery(args: &Value) -> Result<Value, String> {
    let secret = open_and_validate(args)?;
    to_value(ghost_api::WalletRecovery {
        mnemonic: secret.mnemonic.clone(),
        passphrase: secret.passphrase.clone(),
        account_path: secret.account_path.clone(),
        network: secret.network.clone(),
    })
}

async fn send(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let secret = outbound_secret(args)?;
    let projection: WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let destination = required_str(args, "destination")?;
    let amount = decimal(required_str(args, "amountSompi")?, "amount")?;
    let fee = decimal(required_str(args, "feeSompi")?, "fee")?;
    let portal = profile_portal(profile_id, &public, option_endpoint(args, "wrpcEndpoint")).await?;
    let sent = ghost_kaspa::wallet::send(&portal, &secret, &public, destination, amount, fee).await?;
    to_value(project_broadcast(sent))
}

async fn consolidate(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let secret = outbound_secret(args)?;
    let projection: WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let fee = decimal(required_str(args, "feeSompi")?, "fee")?;
    let portal = profile_portal(profile_id, &public, option_endpoint(args, "wrpcEndpoint")).await?;
    let sent = ghost_kaspa::wallet::consolidate(&portal, &secret, &public, fee).await?;
    to_value(project_broadcast(sent))
}

async fn gather_history(args: &Value) -> Result<Value, String> {
    let projection: WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let priority: Vec<String> = required(args, "priorityAddresses")?;
    super::runtime::history::gather(&public, option_endpoint(args, "restEndpoint"), &priority).await.and_then(to_value)
}

fn open_and_validate(args: &Value) -> Result<WalletSecret, String> {
    let password = required_str(args, "password")?;
    require_password(password, "wallet")?;
    let sealed: Vec<u8> = required(args, "sealed")?;
    let projection: WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let secret: WalletSecret = ghost_storage::open_json(password, &sealed, "wallet vault")?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
    Ok(secret)
}

fn outbound_secret(args: &Value) -> Result<WalletSecret, String> {
    let profile_id = required_str(args, "profileId")?;
    let reuse = args.get("reuseUnlocked").and_then(Value::as_bool).unwrap_or(false);
    if reuse {
        if let Some(secret) = WALLET_SECRETS.with(|secrets| secrets.borrow().get(profile_id).cloned()) {
            return Ok(secret);
        }
    }
    let secret = open_and_validate(args)?;
    WALLET_SECRETS.with(|secrets| {
        secrets.borrow_mut().insert(profile_id.to_owned(), secret.clone());
    });
    Ok(secret)
}

fn project_broadcast(sent: ghost_kaspa::wallet::KaspaBroadcastResult) -> BroadcastResult {
    BroadcastResult {
        transaction_id: sent.transaction_id,
        fee_sompi: sent.fee_sompi,
        public: sent.public.projection(),
    }
}
