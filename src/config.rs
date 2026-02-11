use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub binance: BinanceConfig,
    pub strategy: StrategyConfig,
    pub ui: UiConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceConfig {
    pub rest_base_url: String,
    pub ws_base_url: String,
    pub symbol: String,
    pub recv_window: u64,
    #[serde(skip)]
    pub api_key: String,
    #[serde(skip)]
    pub api_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub fast_period: usize,
    pub slow_period: usize,
    pub order_qty: f64,
    pub min_ticks_between_signals: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiConfig {
    pub refresh_rate_ms: u64,
    pub price_history_len: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        dotenvy::dotenv().ok();

        let config_path = Path::new("config/default.toml");
        let config_str = std::fs::read_to_string(config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;

        let mut config: Config =
            toml::from_str(&config_str).context("failed to parse config/default.toml")?;

        config.binance.api_key = std::env::var("BINANCE_API_KEY")
            .context("BINANCE_API_KEY not set in .env or environment")?;
        config.binance.api_secret = std::env::var("BINANCE_API_SECRET")
            .context("BINANCE_API_SECRET not set in .env or environment")?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_toml() {
        let toml_str = r#"
[binance]
rest_base_url = "https://testnet.binance.vision"
ws_base_url = "wss://testnet.binance.vision/ws"
symbol = "BTCUSDT"
recv_window = 5000

[strategy]
fast_period = 10
slow_period = 30
order_qty = 0.001
min_ticks_between_signals = 50

[ui]
refresh_rate_ms = 100
price_history_len = 120

[logging]
level = "debug"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.binance.symbol, "BTCUSDT");
        assert_eq!(config.strategy.fast_period, 10);
        assert_eq!(config.strategy.slow_period, 30);
        assert!((config.strategy.order_qty - 0.001).abs() < f64::EPSILON);
        assert_eq!(config.ui.price_history_len, 120);
    }
}
