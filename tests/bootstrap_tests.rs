use sandbox_quant::app::bootstrap::{AppBootstrap, BinanceEnvConfig, BinanceMode};
use sandbox_quant::error::exchange_error::ExchangeError;
use sandbox_quant::portfolio::store::PortfolioStateStore;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn app_bootstrap_from_env_requires_api_key() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe {
        std::env::remove_var("BINANCE_API_KEY");
        std::env::remove_var("BINANCE_SECRET_KEY");
        std::env::remove_var("BINANCE_MODE");
        std::env::remove_var("BINANCE_SPOT_BASE_URL");
        std::env::remove_var("BINANCE_FUTURES_BASE_URL");
    }

    let error = match AppBootstrap::from_env(PortfolioStateStore::default()) {
        Ok(_) => panic!("missing configuration should fail"),
        Err(error) => error,
    };

    assert_eq!(error, ExchangeError::MissingConfiguration("BINANCE_API_KEY"));
}

#[test]
fn binance_env_config_reads_demo_mode() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe {
        std::env::set_var("BINANCE_API_KEY", "demo-key");
        std::env::set_var("BINANCE_SECRET_KEY", "demo-secret");
        std::env::set_var("BINANCE_MODE", "demo");
        std::env::remove_var("BINANCE_SPOT_BASE_URL");
        std::env::remove_var("BINANCE_FUTURES_BASE_URL");
    }

    let config = BinanceEnvConfig::from_env().expect("demo config should parse");

    assert_eq!(config.mode, BinanceMode::Demo);
}
