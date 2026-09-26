//! `ghost-e2e`: support tool for the two-instance end-to-end suite.
//!
//! ```text
//! ghost-e2e status  [--network testnet-10]        wallet addresses + balances (JSON)
//! ghost-e2e ensure  [--network ..] --min-kas N    wait (interactively) until the dev wallet is funded
//! ghost-e2e fund    [--network ..] --min-kas N --topup-kas M
//! ghost-e2e relay   [--network ..]                run a local p2p-net relay (prints JSON)
//! ```
//!
//! Wallet state lives in `$GHOST_TALK_E2E_HOME` (default `~/.ghost-talk/e2e`),
//! outside the repository, so one funded dev wallet serves every run.

mod kaspa;
mod relay;
mod state;

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    // Kaspa Portal (rustls) and p2p-net (QUIC/WebRTC) enable different
    // rustls backends; pick one process-wide provider explicitly.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ghost-e2e: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: &[String]) -> Result<(), String> {
    let network = option(args, "--network").unwrap_or_else(|| "testnet-10".into());
    match args.first().map(String::as_str) {
        Some("status") => kaspa::status(&network).await,
        Some("ensure") => kaspa::ensure_funded(&network, kas(args, "--min-kas", 25.0)?).await,
        Some("fund") => {
            let min = kas(args, "--min-kas", 5.0)?;
            let topup = kas(args, "--topup-kas", 10.0)?;
            kaspa::fund(&network, min, topup).await
        }
        Some("relay") => relay::run(&network).await,
        _ => Err("usage: ghost-e2e <status|ensure|fund|relay> [--network testnet-10]".into()),
    }
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn kas(args: &[String], name: &str, default: f64) -> Result<u64, String> {
    let value = match option(args, name) {
        Some(raw) => raw
            .parse::<f64>()
            .map_err(|_| format!("{name} must be a number of KAS"))?,
        None => default,
    };
    Ok((value * 100_000_000.0).round() as u64)
}
