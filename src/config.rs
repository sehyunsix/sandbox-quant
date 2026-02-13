use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub broker: Broker,
    pub binance: BinanceConfig,
    #[serde(default)]
    pub alpaca: AlpacaConfig,
    pub strategy: StrategyConfig,
    pub ui: UiConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Broker {
    #[default]
    Binance,
    Alpaca,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceConfig {
    #[serde(default)]
    pub product: TradingProduct,
    pub rest_base_url: String,
    pub ws_base_url: String,
    pub recv_window: u64,
    pub kline_interval: String,
    #[serde(skip)]
    pub api_key: String,
    #[serde(skip)]
    pub api_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlpacaConfig {
    #[serde(default)]
    pub asset_class: AlpacaAssetClass,
    #[serde(default = "default_alpaca_symbol")]
    pub symbol: String,
    #[serde(default = "default_alpaca_symbols")]
    pub symbols: Vec<String>,
    #[serde(default = "default_alpaca_kline_interval")]
    pub kline_interval: String,
    #[serde(default = "default_alpaca_trading_base_url")]
    pub trading_base_url: String,
    #[serde(default = "default_alpaca_data_base_url")]
    pub data_base_url: String,
    #[serde(default = "default_alpaca_option_snapshot_feeds")]
    pub option_snapshot_feeds: Vec<String>,
    #[serde(skip)]
    pub api_key: String,
    #[serde(skip)]
    pub api_secret: String,
}

impl Default for AlpacaConfig {
    fn default() -> Self {
        Self {
            asset_class: AlpacaAssetClass::default(),
            symbol: default_alpaca_symbol(),
            symbols: default_alpaca_symbols(),
            kline_interval: default_alpaca_kline_interval(),
            trading_base_url: default_alpaca_trading_base_url(),
            data_base_url: default_alpaca_data_base_url(),
            option_snapshot_feeds: default_alpaca_option_snapshot_feeds(),
            api_key: String::new(),
            api_secret: String::new(),
        }
    }
}

fn default_alpaca_symbol() -> String {
    "AAPL".to_string()
}
fn default_alpaca_symbols() -> Vec<String> {
    vec!["AAPL".to_string(), "GLD".to_string(), "SLV".to_string()]
}
fn default_alpaca_kline_interval() -> String {
    "1m".to_string()
}
fn default_alpaca_trading_base_url() -> String {
    "https://paper-api.alpaca.markets".to_string()
}
fn default_alpaca_data_base_url() -> String {
    "https://data.alpaca.markets".to_string()
}
fn default_alpaca_option_snapshot_feeds() -> Vec<String> {
    vec!["indicative".to_string(), "opra".to_string()]
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AlpacaAssetClass {
    #[default]
    UsEquity,
    UsOption,
    UsFuture,
}

impl AlpacaAssetClass {
    pub fn label(self) -> &'static str {
        match self {
            AlpacaAssetClass::UsEquity => "US EQUITY",
            AlpacaAssetClass::UsOption => "US OPTION",
            AlpacaAssetClass::UsFuture => "US FUTURE",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TradingProduct {
    #[default]
    BtcSpot,
    BtcFuture,
    EthSpot,
    EthFuture,
}

impl TradingProduct {
    pub const ALL: [TradingProduct; 4] = [
        TradingProduct::BtcSpot,
        TradingProduct::BtcFuture,
        TradingProduct::EthSpot,
        TradingProduct::EthFuture,
    ];

    pub fn symbol(self) -> &'static str {
        match self {
            TradingProduct::BtcSpot | TradingProduct::BtcFuture => "BTCUSDT",
            TradingProduct::EthSpot | TradingProduct::EthFuture => "ETHUSDT",
        }
    }

    pub fn market_label(self) -> &'static str {
        match self {
            TradingProduct::BtcSpot | TradingProduct::EthSpot => "SPOT",
            TradingProduct::BtcFuture | TradingProduct::EthFuture => "FUTURE",
        }
    }

    pub fn product_label(self) -> &'static str {
        match self {
            TradingProduct::BtcSpot => "BTC/USDT SPOT",
            TradingProduct::BtcFuture => "BTC/USDT FUTURE",
            TradingProduct::EthSpot => "ETH/USDT SPOT",
            TradingProduct::EthFuture => "ETH/USDT FUTURE",
        }
    }

    pub fn selector_label(self) -> &'static str {
        match self {
            TradingProduct::BtcSpot => "BTC SPOT",
            TradingProduct::BtcFuture => "BTC FUTURE",
            TradingProduct::EthSpot => "ETH SPOT",
            TradingProduct::EthFuture => "ETH FUTURE",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub fast_period: usize,
    pub slow_period: usize,
    pub order_amount_usdt: f64,
    pub min_ticks_between_signals: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategyPreset {
    #[default]
    ConfigMa,
    FastMa,
    SlowMa,
}

impl StrategyPreset {
    pub const ALL: [StrategyPreset; 3] = [
        StrategyPreset::ConfigMa,
        StrategyPreset::FastMa,
        StrategyPreset::SlowMa,
    ];

    pub fn selector_label(self) -> &'static str {
        match self {
            StrategyPreset::ConfigMa => "CONFIG MA",
            StrategyPreset::FastMa => "FAST MA",
            StrategyPreset::SlowMa => "SLOW MA",
        }
    }

    pub fn display_label(self) -> &'static str {
        match self {
            StrategyPreset::ConfigMa => "MA(Config)",
            StrategyPreset::FastMa => "MA(Fast 5/20)",
            StrategyPreset::SlowMa => "MA(Slow 20/60)",
        }
    }

    pub fn periods(self, config: &StrategyConfig) -> (usize, usize, u64) {
        match self {
            StrategyPreset::ConfigMa => (
                config.fast_period,
                config.slow_period,
                config.min_ticks_between_signals,
            ),
            StrategyPreset::FastMa => (5, 20, 10),
            StrategyPreset::SlowMa => (20, 60, 100),
        }
    }
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
        bail!(
            "kline_interval must include a positive number and unit, got '{}'",
            s
        );
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
    pub fn selected_symbol(&self) -> &'static str {
        self.product.symbol()
    }

    pub fn market_label(&self) -> &'static str {
        self.product.market_label()
    }

    pub fn product_label(&self) -> &'static str {
        self.product.product_label()
    }

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

        // Load keys opportunistically; actual requirement is validated for the selected broker at runtime.
        config.binance.api_key = std::env::var("BINANCE_API_KEY").unwrap_or_default();
        config.binance.api_secret = std::env::var("BINANCE_API_SECRET").unwrap_or_default();
        config.alpaca.api_key = std::env::var("APCA_API_KEY_ID").unwrap_or_default();
        config.alpaca.api_secret = std::env::var("APCA_API_SECRET_KEY").unwrap_or_default();

        config
            .binance
            .kline_interval_ms()
            .context("invalid binance.kline_interval in config/default.toml")?;
        config
            .alpaca
            .kline_interval_ms()
            .context("invalid alpaca.kline_interval in config/default.toml")?;

        Ok(config)
    }

    pub fn validate_for_broker(&self, broker: Broker) -> Result<()> {
        match broker {
            Broker::Binance => {
                if self.binance.api_key.is_empty() {
                    bail!("BINANCE_API_KEY not set in .env or environment");
                }
                if self.binance.api_secret.is_empty() {
                    bail!("BINANCE_API_SECRET not set in .env or environment");
                }
            }
            Broker::Alpaca => {
                if self.alpaca.api_key.is_empty() {
                    bail!("APCA_API_KEY_ID not set in .env or environment");
                }
                if self.alpaca.api_secret.is_empty() {
                    bail!("APCA_API_SECRET_KEY not set in .env or environment");
                }
            }
        }
        Ok(())
    }
}

impl AlpacaConfig {
    pub fn kline_interval_ms(&self) -> Result<u64> {
        parse_interval_ms(&self.kline_interval)
    }

    pub fn tradable_symbols(&self) -> Vec<String> {
        let mut symbols = Vec::new();
        let push_unique = |target: &mut Vec<String>, raw: &str| {
            let normalized = raw.trim().to_ascii_uppercase();
            if !normalized.is_empty() && !target.iter().any(|s| s == &normalized) {
                target.push(normalized);
            }
        };

        push_unique(&mut symbols, &self.symbol);
        for symbol in &self.symbols {
            push_unique(&mut symbols, symbol);
        }

        if symbols.is_empty() {
            return default_alpaca_symbols();
        }
        symbols
    }

    pub fn normalized_option_snapshot_feeds(&self) -> Vec<String> {
        let mut feeds = Vec::new();
        for raw in &self.option_snapshot_feeds {
            let normalized = raw.trim().to_ascii_lowercase();
            if !normalized.is_empty() && !feeds.iter().any(|s| s == &normalized) {
                feeds.push(normalized);
            }
        }
        if feeds.is_empty() {
            return default_alpaca_option_snapshot_feeds();
        }
        feeds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_toml() {
        let toml_str = r#"
[binance]
product = "btc_spot"
rest_base_url = "https://demo-api.binance.com"
ws_base_url = "wss://demo-stream.binance.com/ws"
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
        assert_eq!(config.binance.selected_symbol(), "BTCUSDT");
        assert_eq!(config.binance.product_label(), "BTC/USDT SPOT");
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
