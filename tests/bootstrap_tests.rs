use sandbox_quant::app::bootstrap::{AppBootstrap, BinanceEnvConfig, BinanceMode};
use sandbox_quant::error::exchange_error::ExchangeError;
use sandbox_quant::portfolio::store::PortfolioStateStore;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn clear_binance_env() {
    unsafe {
        std::env::remove_var("BINANCE_API_KEY");
        std::env::remove_var("BINANCE_SECRET_KEY");
        std::env::remove_var("BINANCE_DEMO_API_KEY");
        std::env::remove_var("BINANCE_DEMO_SECRET_KEY");
        std::env::remove_var("BINANCE_REAL_API_KEY");
        std::env::remove_var("BINANCE_REAL_SECRET_KEY");
        std::env::remove_var("BINANCE_MODE");
        std::env::remove_var("BINANCE_SPOT_BASE_URL");
        std::env::remove_var("BINANCE_FUTURES_BASE_URL");
        std::env::remove_var("BINANCE_OPTIONS_BASE_URL");
    }
}

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("sandbox-quant-{name}-{nanos}"))
}

fn with_isolated_cwd<T>(name: &str, run: impl FnOnce() -> T) -> T {
    let original_dir = std::env::current_dir().expect("current dir");
    let temp_dir = unique_test_dir(name);
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    std::env::set_current_dir(&temp_dir).expect("set current dir");
    let result = run();
    std::env::set_current_dir(&original_dir).expect("restore current dir");
    fs::remove_dir_all(&temp_dir).expect("remove temp dir");
    result
}

fn with_dotenv_disabled<T>(run: impl FnOnce() -> T) -> T {
    unsafe {
        std::env::set_var("SANDBOX_QUANT_DISABLE_DOTENV", "1");
    }
    let result = run();
    unsafe {
        std::env::remove_var("SANDBOX_QUANT_DISABLE_DOTENV");
    }
    result
}

#[test]
fn app_bootstrap_from_env_requires_api_key() {
    let _guard = env_lock().lock().unwrap_or_else(|error| error.into_inner());
    with_isolated_cwd("missing-config", || {
        with_dotenv_disabled(|| {
            clear_binance_env();

            let error = match AppBootstrap::from_env(PortfolioStateStore::default()) {
                Ok(_) => panic!("missing configuration should fail"),
                Err(error) => error,
            };

            assert_eq!(
                error,
                ExchangeError::MissingConfiguration("BINANCE_DEMO_API_KEY")
            );
        });
    });
}

#[test]
fn binance_env_config_reads_demo_mode() {
    let _guard = env_lock().lock().unwrap_or_else(|error| error.into_inner());
    with_isolated_cwd("demo-mode", || {
        with_dotenv_disabled(|| {
            clear_binance_env();
            unsafe {
                std::env::set_var("BINANCE_DEMO_API_KEY", "demo-key");
                std::env::set_var("BINANCE_DEMO_SECRET_KEY", "demo-secret");
                std::env::set_var("BINANCE_MODE", "demo");
            }

            let config = BinanceEnvConfig::from_env().expect("demo config should parse");

            assert_eq!(config.mode, BinanceMode::Demo);
            assert_eq!(config.api_key, "demo-key");
            assert_eq!(config.secret_key, "demo-secret");
        });
    });
}

#[test]
fn binance_env_config_defaults_to_demo_mode() {
    let _guard = env_lock().lock().unwrap_or_else(|error| error.into_inner());
    with_isolated_cwd("default-demo", || {
        with_dotenv_disabled(|| {
            clear_binance_env();
            unsafe {
                std::env::set_var("BINANCE_DEMO_API_KEY", "demo-key");
                std::env::set_var("BINANCE_DEMO_SECRET_KEY", "demo-secret");
            }

            let config = BinanceEnvConfig::from_env().expect("default config should parse");

            assert_eq!(config.mode, BinanceMode::Demo);
        });
    });
}

#[test]
fn binance_env_config_reads_real_mode_credentials() {
    let _guard = env_lock().lock().unwrap_or_else(|error| error.into_inner());
    with_isolated_cwd("real-mode", || {
        with_dotenv_disabled(|| {
            clear_binance_env();
            unsafe {
                std::env::set_var("BINANCE_DEMO_API_KEY", "demo-key");
                std::env::set_var("BINANCE_DEMO_SECRET_KEY", "demo-secret");
                std::env::set_var("BINANCE_REAL_API_KEY", "real-key");
                std::env::set_var("BINANCE_REAL_SECRET_KEY", "real-secret");
                std::env::set_var("BINANCE_MODE", "real");
            }

            let config = BinanceEnvConfig::from_env().expect("real config should parse");

            assert_eq!(config.mode, BinanceMode::Real);
            assert_eq!(config.api_key, "real-key");
            assert_eq!(config.secret_key, "real-secret");
        });
    });
}

#[test]
fn binance_env_config_falls_back_to_legacy_credentials() {
    let _guard = env_lock().lock().unwrap_or_else(|error| error.into_inner());
    with_isolated_cwd("legacy-fallback", || {
        with_dotenv_disabled(|| {
            clear_binance_env();
            unsafe {
                std::env::set_var("BINANCE_API_KEY", "legacy-key");
                std::env::set_var("BINANCE_SECRET_KEY", "legacy-secret");
                std::env::set_var("BINANCE_MODE", "real");
            }

            let config = BinanceEnvConfig::from_env().expect("legacy config should parse");

            assert_eq!(config.mode, BinanceMode::Real);
            assert_eq!(config.api_key, "legacy-key");
            assert_eq!(config.secret_key, "legacy-secret");
        });
    });
}

#[test]
fn binance_env_config_reads_dotenv_file() {
    let _guard = env_lock().lock().unwrap_or_else(|error| error.into_inner());
    with_isolated_cwd("dotenv-load", || {
        clear_binance_env();
        fs::write(
            ".env",
            "BINANCE_MODE=real\nBINANCE_REAL_API_KEY=dotenv-real-key\nBINANCE_REAL_SECRET_KEY=dotenv-real-secret\n",
        )
        .expect("write .env");

        let config = BinanceEnvConfig::from_env().expect("dotenv config should parse");

        assert_eq!(config.mode, BinanceMode::Real);
        assert_eq!(config.api_key, "dotenv-real-key");
        assert_eq!(config.secret_key, "dotenv-real-secret");
    });
}
