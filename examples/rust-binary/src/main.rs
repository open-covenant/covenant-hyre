//! Standalone Rust reproducer of the Covenant + Hyre x402 paid call.
//!
//! Mirrors examples/quick.sh: fetch the 402, validate the security pins,
//! pipe PaymentRequirements JSON to the covenant-x402-signer sidecar binary,
//! re-POST with the resulting X-PAYMENT header. No covenant deps; the only
//! thing this binary borrows from upstream is the sidecar process itself.
//!
//! Env:
//!   COVENANT_X402_SIGNER_BIN       path to the built sidecar binary
//!   COVENANT_X402_FUNDING_KEYPAIR  path to the funder Solana keypair JSON
//!   COVENANT_X402_RPC_URL          optional, defaults to mainnet-beta
//!   HYRE_ENDPOINT                  optional, defaults to /defi/tvl

use std::process::Stdio;

use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const DEFAULT_ENDPOINT: &str = "https://mpp.hyreagent.fun/defi/tvl";
const PER_CALL_CAP_ATOMIC: u128 = 50_000;
const PINNED_PAY_TO: &str = "7G73PLhKvAPBGTzG5ESAE4coE7QrVeTTKfhTxQZbyGgC";
const PINNED_FEE_PAYER: &str = "2wKupLR9q6wXYppw8Gr2NvWxKBUqm4PPJKkQfoxHDBg4";
const PINNED_USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const CAIP2_SOLANA: &str = "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp";

#[derive(Debug, Deserialize)]
struct Challenge {
    accepts: Vec<Accept>,
}

#[derive(Debug, Deserialize)]
struct Accept {
    scheme: String,
    asset: String,
    #[serde(rename = "payTo")]
    pay_to: String,
    #[serde(default, rename = "maxAmountRequired")]
    max_amount_required: Option<String>,
    #[serde(default)]
    amount: Option<String>,
    #[serde(default)]
    extra: Option<Extra>,
}

#[derive(Debug, Deserialize)]
struct Extra {
    #[serde(rename = "feePayer", default)]
    fee_payer: Option<String>,
}

impl Accept {
    fn atomic(&self) -> Option<u128> {
        self.max_amount_required
            .as_deref()
            .or(self.amount.as_deref())
            .and_then(|s| s.parse().ok())
    }
}

#[tokio::main]
async fn main() {
    let signer_bin = env("COVENANT_X402_SIGNER_BIN");
    let keypair = env("COVENANT_X402_FUNDING_KEYPAIR");
    let rpc = std::env::var("COVENANT_X402_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".into());
    let endpoint = std::env::var("HYRE_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.into());

    let http = reqwest::Client::new();

    eprintln!("[1/4] fetching the 402 challenge (free)...");
    let probe = http
        .get(&endpoint)
        .send()
        .await
        .unwrap_or_else(|e| die(format!("GET: {e}")));
    let status = probe.status();
    let body = probe.text().await.unwrap_or_default();
    if status.as_u16() != 402 {
        die(format!(
            "expected 402, got {}, body: {}",
            status,
            &body[..body.len().min(400)]
        ));
    }
    let challenge: Challenge =
        serde_json::from_str(&body).unwrap_or_else(|e| die(format!("decode challenge: {e}")));

    eprintln!("[2/4] picking option + verifying pins...");
    let chosen = challenge
        .accepts
        .iter()
        .find(|a| a.scheme == "exact" && a.atomic().is_some_and(|n| n <= PER_CALL_CAP_ATOMIC))
        .unwrap_or_else(|| {
            die(format!(
                "no acceptable option within cap {PER_CALL_CAP_ATOMIC}"
            ))
        });
    let fee_payer = chosen
        .extra
        .as_ref()
        .and_then(|e| e.fee_payer.as_deref())
        .unwrap_or_else(|| die("no extra.feePayer in chosen option"));
    if chosen.pay_to != PINNED_PAY_TO {
        die(format!("pay_to pin failed: {}", chosen.pay_to));
    }
    if fee_payer != PINNED_FEE_PAYER {
        die(format!("feePayer pin failed: {fee_payer}"));
    }
    if chosen.asset != PINNED_USDC_MINT {
        die(format!("asset pin failed: {}", chosen.asset));
    }
    let amount = chosen.atomic().unwrap();
    eprintln!("    pinned pay_to + fee_payer + asset OK, amount {amount} atomic USDC");

    eprintln!("[3/4] piping PaymentRequirements to the sidecar...");
    let requirements = json!({
        "network": CAIP2_SOLANA,
        "asset": PINNED_USDC_MINT,
        "amount": amount.to_string(),
        "amountUsdc": (amount as f64) / 1_000_000.0,
        "payTo": PINNED_PAY_TO,
        "scheme": "exact",
        "extra": { "feePayer": fee_payer },
    });
    let payload = serde_json::to_vec(&requirements).unwrap();
    let mut child = Command::new(&signer_bin)
        .env_clear()
        .env("COVENANT_X402_FUNDING_KEYPAIR", &keypair)
        .env("COVENANT_X402_RPC_URL", &rpc)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| die(format!("spawn {signer_bin}: {e}")));
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(&payload)
        .await
        .unwrap_or_else(|e| die(format!("write stdin: {e}")));
    let out = child
        .wait_with_output()
        .await
        .unwrap_or_else(|e| die(format!("await: {e}")));
    if !out.status.success() {
        die(format!(
            "sidecar exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let header = String::from_utf8(out.stdout)
        .unwrap_or_else(|e| die(format!("non-utf8 stdout: {e}")))
        .trim()
        .to_string();
    if header.is_empty() {
        die("sidecar returned empty header");
    }
    eprintln!("    signed, X-PAYMENT length={}", header.len());

    eprintln!("[4/4] re-POSTing with X-PAYMENT...");
    let final_resp = http
        .get(&endpoint)
        .header("x-payment", &header)
        .send()
        .await
        .unwrap_or_else(|e| die(format!("retry GET: {e}")));
    let final_status = final_resp.status();
    let final_body = final_resp.text().await.unwrap_or_default();
    eprintln!("    status={}", final_status.as_u16());
    let preview: String = final_body.chars().take(800).collect();
    println!("{preview}");
    if !final_status.is_success() {
        die(format!("expected 2xx, got {}", final_status.as_u16()));
    }
    eprintln!("OK: {amount} atomic USDC paid; tx is on chain.");
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| die(format!("env var {name} must be set")))
}

fn die(msg: impl Into<String>) -> ! {
    eprintln!("FAIL: {}", msg.into());
    std::process::exit(1);
}
