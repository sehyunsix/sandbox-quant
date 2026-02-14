use anyhow::{bail, Context, Result};
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
    #[serde(default)]
    pub symbols: Vec<String>,
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
        bail!("invalid interval '{}': expected format like '1m'", s);
    }

    let (num_str, suffix) = s.split_at(s.len() - 1);
    let n: u64 = num_str.parse().with_context(|| {
        format!(
            "invalid interval '{}': quantity must be a positive integer",
            s
        )
    })?;
    if n == 0 {
        bail!("invalid interval '{}': quantity must be > 0", s);
    }

    let unit_ms = match suffix {
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        "w" => 7 * 86_400_000,
        "M" => 30 * 86_400_000,
        _ => bail!(
            "invalid interval '{}': unsupported suffix '{}', expected one of s/m/h/d/w/M",
            s,
            suffix
        ),
    };

    n.checked_mul(unit_ms)
        .with_context(|| format!("invalid interval '{}': value is too large", s))
}

impl BinanceConfig {
    pub fn kline_interval_ms(&self) -> Result<u64> {
        parse_interval_ms(&self.kline_interval)
    }

    pub fn tradable_symbols(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.symbol.trim().is_empty() {
            out.push(self.symbol.trim().to_ascii_uppercase());
        }
        for sym in &self.symbols {
            let s = sym.trim().to_ascii_uppercase();
            if !s.is_empty() && !out.iter().any(|v| v == &s) {
                out.push(s);
            }
        }
        out
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
            .context("binance.kline_interval is invalid")?;

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
symbols = ["ETHUSDT", "BNBUSDT"]
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
        assert_eq!(config.binance.symbols.len(), 2);
        assert_eq!(config.strategy.fast_period, 10);
        assert_eq!(config.strategy.slow_period, 30);
        assert!((config.strategy.order_amount_usdt - 10.0).abs() < f64::EPSILON);
        assert_eq!(config.ui.price_history_len, 120);
    }

    #[test]
    fn tradable_symbols_dedup_and_include_primary() {
        let cfg = BinanceConfig {
            rest_base_url: "x".to_string(),
            ws_base_url: "y".to_string(),
            symbol: "btcusdt".to_string(),
            symbols: vec![
                "ETHUSDT".to_string(),
                "BTCUSDT".to_string(),
                "  ".to_string(),
            ],
            recv_window: 5000,
            kline_interval: "1m".to_string(),
            api_key: String::new(),
            api_secret: String::new(),
        };
        assert_eq!(
            cfg.tradable_symbols(),
            vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]
        );
    }

    #[test]
    fn parse_interval_valid() {
        assert_eq!(parse_interval_ms("1m").unwrap(), 60_000);
        assert_eq!(parse_interval_ms("2h").unwrap(), 7_200_000);
        assert_eq!(parse_interval_ms("1M").unwrap(), 2_592_000_000);
    }

    #[test]
    fn parse_interval_rejects_invalid_inputs() {
        assert!(parse_interval_ms("").is_err());
        assert!(parse_interval_ms("m").is_err());
        assert!(parse_interval_ms("0m").is_err());
        assert!(parse_interval_ms("1x").is_err());
    }
}
