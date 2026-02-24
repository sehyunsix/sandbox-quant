use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub binance: BinanceConfig,
    pub strategy: StrategyConfig,
    #[serde(default)]
    pub risk: RiskConfig,
    #[serde(default)]
    pub ev: EvConfig,
    #[serde(default)]
    pub exit: ExitConfig,
    pub ui: UiConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceConfig {
    pub rest_base_url: String,
    pub ws_base_url: String,
    #[serde(default = "default_futures_rest_base_url")]
    pub futures_rest_base_url: String,
    #[serde(default = "default_futures_ws_base_url")]
    pub futures_ws_base_url: String,
    pub symbol: String,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub futures_symbols: Vec<String>,
    pub recv_window: u64,
    pub kline_interval: String,
    #[serde(skip)]
    pub api_key: String,
    #[serde(skip)]
    pub api_secret: String,
    #[serde(skip)]
    pub futures_api_key: String,
    #[serde(skip)]
    pub futures_api_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub fast_period: usize,
    pub slow_period: usize,
    pub order_amount_usdt: f64,
    pub min_ticks_between_signals: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    #[serde(default = "default_global_rate_limit_per_minute")]
    pub global_rate_limit_per_minute: u32,
    #[serde(default = "default_strategy_cooldown_ms")]
    pub default_strategy_cooldown_ms: u64,
    #[serde(default = "default_strategy_max_active_orders")]
    pub default_strategy_max_active_orders: u32,
    #[serde(default = "default_symbol_max_exposure_usdt")]
    pub default_symbol_max_exposure_usdt: f64,
    #[serde(default)]
    pub strategy_limits: Vec<StrategyLimitConfig>,
    #[serde(default)]
    pub symbol_exposure_limits: Vec<SymbolExposureLimitConfig>,
    #[serde(default)]
    pub endpoint_rate_limits: EndpointRateLimitConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyLimitConfig {
    pub source_tag: String,
    pub cooldown_ms: Option<u64>,
    pub max_active_orders: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SymbolExposureLimitConfig {
    pub symbol: String,
    pub market: Option<String>,
    pub max_exposure_usdt: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EndpointRateLimitConfig {
    #[serde(default = "default_endpoint_orders_limit_per_minute")]
    pub orders_per_minute: u32,
    #[serde(default = "default_endpoint_account_limit_per_minute")]
    pub account_per_minute: u32,
    #[serde(default = "default_endpoint_market_data_limit_per_minute")]
    pub market_data_per_minute: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvConfig {
    #[serde(default = "default_ev_enabled")]
    pub enabled: bool,
    #[serde(default = "default_ev_mode")]
    pub mode: String,
    #[serde(default = "default_ev_lookback_trades")]
    pub lookback_trades: usize,
    #[serde(default = "default_ev_prior_a")]
    pub prior_a: f64,
    #[serde(default = "default_ev_prior_b")]
    pub prior_b: f64,
    #[serde(default = "default_ev_tail_prior_a")]
    pub tail_prior_a: f64,
    #[serde(default = "default_ev_tail_prior_b")]
    pub tail_prior_b: f64,
    #[serde(default = "default_ev_recency_lambda")]
    pub recency_lambda: f64,
    #[serde(default = "default_ev_shrink_k")]
    pub shrink_k: f64,
    #[serde(default = "default_ev_loss_threshold_usdt")]
    pub loss_threshold_usdt: f64,
    #[serde(default = "default_ev_gamma_tail_penalty")]
    pub gamma_tail_penalty: f64,
    #[serde(default = "default_ev_fee_slippage_penalty_usdt")]
    pub fee_slippage_penalty_usdt: f64,
    #[serde(default = "default_ev_entry_gate_min_ev_usdt")]
    pub entry_gate_min_ev_usdt: f64,
}

impl Default for EvConfig {
    fn default() -> Self {
        Self {
            enabled: default_ev_enabled(),
            mode: default_ev_mode(),
            lookback_trades: default_ev_lookback_trades(),
            prior_a: default_ev_prior_a(),
            prior_b: default_ev_prior_b(),
            tail_prior_a: default_ev_tail_prior_a(),
            tail_prior_b: default_ev_tail_prior_b(),
            recency_lambda: default_ev_recency_lambda(),
            shrink_k: default_ev_shrink_k(),
            loss_threshold_usdt: default_ev_loss_threshold_usdt(),
            gamma_tail_penalty: default_ev_gamma_tail_penalty(),
            fee_slippage_penalty_usdt: default_ev_fee_slippage_penalty_usdt(),
            entry_gate_min_ev_usdt: default_ev_entry_gate_min_ev_usdt(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExitConfig {
    #[serde(default = "default_exit_max_holding_ms")]
    pub max_holding_ms: u64,
    #[serde(default = "default_exit_stop_loss_pct")]
    pub stop_loss_pct: f64,
    #[serde(default = "default_exit_enforce_protective_stop")]
    pub enforce_protective_stop: bool,
}

impl Default for ExitConfig {
    fn default() -> Self {
        Self {
            max_holding_ms: default_exit_max_holding_ms(),
            stop_loss_pct: default_exit_stop_loss_pct(),
            enforce_protective_stop: default_exit_enforce_protective_stop(),
        }
    }
}

impl Default for EndpointRateLimitConfig {
    fn default() -> Self {
        Self {
            orders_per_minute: default_endpoint_orders_limit_per_minute(),
            account_per_minute: default_endpoint_account_limit_per_minute(),
            market_data_per_minute: default_endpoint_market_data_limit_per_minute(),
        }
    }
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            global_rate_limit_per_minute: default_global_rate_limit_per_minute(),
            default_strategy_cooldown_ms: default_strategy_cooldown_ms(),
            default_strategy_max_active_orders: default_strategy_max_active_orders(),
            default_symbol_max_exposure_usdt: default_symbol_max_exposure_usdt(),
            strategy_limits: Vec::new(),
            symbol_exposure_limits: Vec::new(),
            endpoint_rate_limits: EndpointRateLimitConfig::default(),
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

fn default_futures_rest_base_url() -> String {
    "https://demo-fapi.binance.com".to_string()
}

fn default_futures_ws_base_url() -> String {
    "wss://fstream.binancefuture.com/ws".to_string()
}

fn default_global_rate_limit_per_minute() -> u32 {
    600
}

fn default_strategy_cooldown_ms() -> u64 {
    3_000
}

fn default_strategy_max_active_orders() -> u32 {
    1
}

fn default_symbol_max_exposure_usdt() -> f64 {
    200.0
}

fn default_endpoint_orders_limit_per_minute() -> u32 {
    240
}

fn default_endpoint_account_limit_per_minute() -> u32 {
    180
}

fn default_endpoint_market_data_limit_per_minute() -> u32 {
    360
}

fn default_ev_enabled() -> bool {
    true
}

fn default_ev_mode() -> String {
    "shadow".to_string()
}

fn default_ev_lookback_trades() -> usize {
    200
}

fn default_ev_prior_a() -> f64 {
    6.0
}

fn default_ev_prior_b() -> f64 {
    6.0
}

fn default_ev_tail_prior_a() -> f64 {
    3.0
}

fn default_ev_tail_prior_b() -> f64 {
    7.0
}

fn default_ev_recency_lambda() -> f64 {
    0.08
}

fn default_ev_shrink_k() -> f64 {
    40.0
}

fn default_ev_loss_threshold_usdt() -> f64 {
    15.0
}

fn default_ev_gamma_tail_penalty() -> f64 {
    0.8
}

fn default_ev_fee_slippage_penalty_usdt() -> f64 {
    0.0
}

fn default_ev_entry_gate_min_ev_usdt() -> f64 {
    0.0
}

fn default_exit_max_holding_ms() -> u64 {
    1_800_000
}

fn default_exit_stop_loss_pct() -> f64 {
    0.015
}

fn default_exit_enforce_protective_stop() -> bool {
    true
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

    pub fn tradable_instruments(&self) -> Vec<String> {
        let mut out = self.tradable_symbols();
        for sym in &self.futures_symbols {
            let s = sym.trim().to_ascii_uppercase();
            if !s.is_empty() {
                let label = format!("{} (FUT)", s);
                if !out.iter().any(|v| v == &label) {
                    out.push(label);
                }
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
        config.binance.futures_api_key = std::env::var("BINANCE_FUTURES_API_KEY")
            .unwrap_or_else(|_| config.binance.api_key.clone());
        config.binance.futures_api_secret = std::env::var("BINANCE_FUTURES_API_SECRET")
            .unwrap_or_else(|_| config.binance.api_secret.clone());

        config
            .binance
            .kline_interval_ms()
            .context("binance.kline_interval is invalid")?;

        Ok(config)
    }
}
