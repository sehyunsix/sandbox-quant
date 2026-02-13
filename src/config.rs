use anyhow::{Context, Result, bail};
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
    pub kline_interval: String,
    #[serde(skip)]
    pub api_key: String,
    #[serde(skip)]
    pub api_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub fast_period: usize,
    pub slow_period: usize,
    pub order_amount_usdt: f64,
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

/// Parse a Binance kline interval string (e.g. "1s", "1m", "1h", "1d", "1w", "1M") into milliseconds.
pub fn parse_interval_ms(s: &str) -> Result<u64> {
    if s.len() < 2 {
        bail!("kline_interval must include a positive number and unit, got '{}'", s);
    }

    let (num_str, suffix) = s.split_at(s.len() - 1);
    let n: u64 = num_str
        .parse()
        .with_context(|| format!("invalid kline interval number '{}'", num_str))?;
    if n == 0 {
        bail!("kline_interval value must be > 0, got '{}'", s);
    }

    let ms = match suffix {
        "s" => n * 1_000,
        "m" => n * 60_000,
        "h" => n * 3_600_000,
        "d" => n * 86_400_000,
        "w" => n * 7 * 86_400_000,
        "M" => n * 30 * 86_400_000,
        _ => bail!("unsupported kline_interval unit '{}'", suffix),
    };

    Ok(ms)
}

impl BinanceConfig {
    pub fn kline_interval_ms(&self) -> Result<u64> {
        parse_interval_ms(&self.kline_interval)
    }
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
        config
            .binance
            .kline_interval_ms()
            .context("invalid binance.kline_interval in config/default.toml")?;

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
rest_base_url = "https://demo-api.binance.com"
ws_base_url = "wss://demo-stream.binance.com/ws"
symbol = "BTCUSDT"
recv_window = 5000
kline_interval = "1m"

[strategy]
fast_period = 10
slow_period = 30
order_amount_usdt = 10.0
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
        assert!((config.strategy.order_amount_usdt - 10.0).abs() < f64::EPSILON);
        assert_eq!(config.ui.price_history_len, 120);
    }

    #[test]
    fn parse_interval_valid() {
        assert_eq!(parse_interval_ms("1m").unwrap(), 60_000);
        assert_eq!(parse_interval_ms("2h").unwrap(), 7_200_000);
    }

    #[test]
    fn parse_interval_rejects_invalid_inputs() {
        assert!(parse_interval_ms("").is_err());
        assert!(parse_interval_ms("0m").is_err());
        assert!(parse_interval_ms("xm").is_err());
        assert!(parse_interval_ms("1x").is_err());
    }
}
