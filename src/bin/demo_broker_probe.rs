use anyhow::{bail, Context, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION};
use serde::Deserialize;

#[derive(Debug)]
struct ProbeResult {
    broker: &'static str,
    status: ProbeStatus,
    detail: String,
}

#[derive(Debug, PartialEq, Eq)]
enum ProbeStatus {
    Ok,
    Skipped,
    Failed,
}

#[derive(Debug, Deserialize)]
struct AlpacaAccount {
    id: String,
    status: String,
    account_number: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let results = vec![probe_alpaca().await, probe_tradier().await];

    println!("demo broker probe results");
    println!("=========================");

    let mut has_failure = false;
    for result in &results {
        let status = match result.status {
            ProbeStatus::Ok => "OK",
            ProbeStatus::Skipped => "SKIPPED",
            ProbeStatus::Failed => {
                has_failure = true;
                "FAILED"
            }
        };
        println!("- {:<16} {:<7} {}", result.broker, status, result.detail);
    }

    if has_failure {
        bail!("one or more broker probes failed");
    }

    Ok(())
}

async fn probe_alpaca() -> ProbeResult {
    let key = match std::env::var("ALPACA_PAPER_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return ProbeResult {
                broker: "alpaca-paper",
                status: ProbeStatus::Skipped,
                detail: "set ALPACA_PAPER_API_KEY and ALPACA_PAPER_API_SECRET".to_string(),
            }
        }
    };
    let secret = match std::env::var("ALPACA_PAPER_API_SECRET") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return ProbeResult {
                broker: "alpaca-paper",
                status: ProbeStatus::Skipped,
                detail: "set ALPACA_PAPER_API_KEY and ALPACA_PAPER_API_SECRET".to_string(),
            }
        }
    };

    let base = std::env::var("ALPACA_PAPER_BASE_URL")
        .unwrap_or_else(|_| "https://paper-api.alpaca.markets".to_string());
    let url = format!("{}/v2/account", base.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let response = match client
        .get(&url)
        .header("APCA-API-KEY-ID", &key)
        .header("APCA-API-SECRET-KEY", &secret)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return ProbeResult {
                broker: "alpaca-paper",
                status: ProbeStatus::Failed,
                detail: format!("request error: {}", e),
            }
        }
    };

    if !response.status().is_success() {
        let code = response.status();
        let body = response.text().await.unwrap_or_default();
        return ProbeResult {
            broker: "alpaca-paper",
            status: ProbeStatus::Failed,
            detail: format!("HTTP {} {}", code, compact_body(&body)),
        };
    }

    let account: AlpacaAccount = match response.json().await.context("invalid account response") {
        Ok(v) => v,
        Err(e) => {
            return ProbeResult {
                broker: "alpaca-paper",
                status: ProbeStatus::Failed,
                detail: format!("parse error: {}", e),
            }
        }
    };

    ProbeResult {
        broker: "alpaca-paper",
        status: ProbeStatus::Ok,
        detail: format!(
            "account={} status={} id={}",
            mask_account(&account.account_number),
            account.status,
            short_id(&account.id)
        ),
    }
}

async fn probe_tradier() -> ProbeResult {
    let token = match std::env::var("TRADIER_SANDBOX_TOKEN") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return ProbeResult {
                broker: "tradier-sandbox",
                status: ProbeStatus::Skipped,
                detail: "set TRADIER_SANDBOX_TOKEN".to_string(),
            }
        }
    };

    let base = std::env::var("TRADIER_SANDBOX_BASE_URL")
        .unwrap_or_else(|_| "https://sandbox.tradier.com/v1".to_string());
    let url = format!("{}/user/profile", base.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let response = match client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header(ACCEPT, "application/json")
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return ProbeResult {
                broker: "tradier-sandbox",
                status: ProbeStatus::Failed,
                detail: format!("request error: {}", e),
            }
        }
    };

    if !response.status().is_success() {
        let code = response.status();
        let body = response.text().await.unwrap_or_default();
        return ProbeResult {
            broker: "tradier-sandbox",
            status: ProbeStatus::Failed,
            detail: format!("HTTP {} {}", code, compact_body(&body)),
        };
    }

    let json: serde_json::Value = match response.json().await.context("invalid profile response") {
        Ok(v) => v,
        Err(e) => {
            return ProbeResult {
                broker: "tradier-sandbox",
                status: ProbeStatus::Failed,
                detail: format!("parse error: {}", e),
            }
        }
    };

    match tradier_account_count(&json) {
        Ok(count) => ProbeResult {
            broker: "tradier-sandbox",
            status: ProbeStatus::Ok,
            detail: format!("profile reachable; accounts={}", count),
        },
        Err(e) => ProbeResult {
            broker: "tradier-sandbox",
            status: ProbeStatus::Failed,
            detail: format!("shape error: {}", e),
        },
    }
}

fn tradier_account_count(v: &serde_json::Value) -> Result<usize> {
    let profile = v
        .get("profile")
        .context("missing profile object")?
        .as_object()
        .context("profile is not an object")?;

    let account_value = profile.get("account").context("missing profile.account")?;
    if let Some(arr) = account_value.as_array() {
        return Ok(arr.len());
    }
    if account_value.is_object() {
        return Ok(1);
    }

    bail!("profile.account is neither object nor array")
}

fn compact_body(body: &str) -> String {
    body.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn mask_account(account_number: &str) -> String {
    if account_number.len() <= 4 {
        return account_number.to_string();
    }
    let suffix = &account_number[account_number.len() - 4..];
    format!("***{}", suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_masking_keeps_last_4() {
        assert_eq!(mask_account("12345678"), "***5678");
        assert_eq!(mask_account("1234"), "1234");
    }

    #[test]
    fn tradier_account_count_accepts_object() {
        let v: serde_json::Value = serde_json::json!({
            "profile": {
                "account": { "account_number": "ABC" }
            }
        });
        assert_eq!(tradier_account_count(&v).unwrap(), 1);
    }

    #[test]
    fn tradier_account_count_accepts_array() {
        let v: serde_json::Value = serde_json::json!({
            "profile": {
                "account": [
                    { "account_number": "ABC" },
                    { "account_number": "DEF" }
                ]
            }
        });
        assert_eq!(tradier_account_count(&v).unwrap(), 2);
    }
}
