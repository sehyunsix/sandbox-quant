use sandbox_quant::app::bootstrap::{BinanceEnvConfig, BinanceMode};
use sandbox_quant::exchange::binance::auth::BinanceAuth;
use sandbox_quant::exchange::binance::client::{BinanceExchange, BinanceHttpTransport};
use sandbox_quant::exchange::binance::demo::BinanceDemoHttpTransport;
use sandbox_quant::exchange::facade::ExchangeFacade;
use std::sync::Arc;

fn integration_auth() -> Option<BinanceAuth> {
    let api_key = std::env::var("BINANCE_API_KEY").ok()?;
    let secret_key = std::env::var("BINANCE_SECRET_KEY").ok()?;
    Some(BinanceAuth::new(api_key, secret_key))
}

#[test]
#[ignore = "requires live network access and valid Binance credentials"]
fn live_or_demo_refresh_hits_real_signed_account_endpoint() {
    let Some(auth) = integration_auth() else {
        return;
    };

    let mode = match std::env::var("BINANCE_MODE")
        .unwrap_or_else(|_| "real".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "demo" => BinanceMode::Demo,
        _ => BinanceMode::Real,
    };

    let exchange = match mode {
        BinanceMode::Real => BinanceExchange::new(Arc::new(BinanceHttpTransport::new(auth))),
        BinanceMode::Demo => BinanceExchange::new(Arc::new(BinanceDemoHttpTransport::new(auth))),
    };

    let snapshot = exchange
        .load_authoritative_snapshot()
        .expect("signed account request should succeed");

    assert!(
        !snapshot.balances.is_empty() || !snapshot.positions.is_empty(),
        "expected balances or positions from live endpoint"
    );
}

#[test]
fn env_config_parses_mode_for_live_network_integration() {
    unsafe {
        std::env::set_var("BINANCE_API_KEY", "k");
        std::env::set_var("BINANCE_SECRET_KEY", "s");
        std::env::set_var("BINANCE_MODE", "demo");
        std::env::remove_var("BINANCE_SPOT_BASE_URL");
        std::env::remove_var("BINANCE_FUTURES_BASE_URL");
    }

    let config = BinanceEnvConfig::from_env().expect("env config should parse");
    assert_eq!(config.mode, BinanceMode::Demo);
}
