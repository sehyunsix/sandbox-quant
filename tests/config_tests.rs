use sandbox_quant::config::{parse_interval_ms, BinanceConfig, Config};

#[test]
fn parse_default_toml() {
    let toml_str = r#"
[binance]
rest_base_url = "https://demo-api.binance.com"
ws_base_url = "wss://demo-stream.binance.com/ws"
futures_rest_base_url = "https://demo-fapi.binance.com"
futures_ws_base_url = "wss://fstream.binancefuture.com/ws"
symbol = "BTCUSDT"
symbols = ["ETHUSDT", "BNBUSDT"]
futures_symbols = ["BTCUSDT", "ETHUSDT", "BNBUSDT", "SOLUSDT"]
recv_window = 5000
kline_interval = "1m"

[strategy]
fast_period = 10
slow_period = 30
order_amount_usdt = 10.0
min_ticks_between_signals = 50

[risk]
global_rate_limit_per_minute = 600
default_strategy_cooldown_ms = 3000
default_strategy_max_active_orders = 1
default_symbol_max_exposure_usdt = 200.0

[[risk.strategy_limits]]
source_tag = "mnl"
cooldown_ms = 0
max_active_orders = 2

[[risk.symbol_exposure_limits]]
symbol = "BTCUSDT"
market = "spot"
max_exposure_usdt = 300.0

[risk.endpoint_rate_limits]
orders_per_minute = 240
account_per_minute = 180
market_data_per_minute = 360

[ui]
refresh_rate_ms = 100
price_history_len = 120

[logging]
level = "debug"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.binance.symbol, "BTCUSDT");
    assert_eq!(config.binance.symbols.len(), 2);
    assert_eq!(config.binance.futures_symbols.len(), 4);
    assert_eq!(config.strategy.fast_period, 10);
    assert_eq!(config.strategy.slow_period, 30);
    assert!((config.strategy.order_amount_usdt - 10.0).abs() < f64::EPSILON);
    assert_eq!(config.risk.global_rate_limit_per_minute, 600);
    assert_eq!(config.risk.default_strategy_cooldown_ms, 3000);
    assert_eq!(config.risk.default_strategy_max_active_orders, 1);
    assert!((config.risk.default_symbol_max_exposure_usdt - 200.0).abs() < f64::EPSILON);
    assert_eq!(config.risk.strategy_limits.len(), 1);
    assert_eq!(config.risk.strategy_limits[0].source_tag, "mnl");
    assert_eq!(config.risk.strategy_limits[0].cooldown_ms, Some(0));
    assert_eq!(config.risk.strategy_limits[0].max_active_orders, Some(2));
    assert_eq!(config.risk.symbol_exposure_limits.len(), 1);
    assert_eq!(config.risk.symbol_exposure_limits[0].symbol, "BTCUSDT");
    assert_eq!(
        config.risk.symbol_exposure_limits[0].market.as_deref(),
        Some("spot")
    );
    assert!((config.risk.symbol_exposure_limits[0].max_exposure_usdt - 300.0).abs() < f64::EPSILON);
    assert_eq!(config.risk.endpoint_rate_limits.orders_per_minute, 240);
    assert_eq!(config.risk.endpoint_rate_limits.account_per_minute, 180);
    assert_eq!(config.risk.endpoint_rate_limits.market_data_per_minute, 360);
    assert!(config.ev.enabled);
    assert_eq!(config.ev.mode, "shadow");
    assert_eq!(config.ev.lookback_trades, 200);
    assert!((config.ev.forward_p_win - 0.5).abs() < f64::EPSILON);
    assert!((config.ev.forward_target_rr - 1.5).abs() < f64::EPSILON);
    assert!((config.ev.y_mu - 0.0).abs() < f64::EPSILON);
    assert!((config.ev.y_sigma_spot - 0.01).abs() < f64::EPSILON);
    assert!((config.ev.y_sigma_futures - 0.015).abs() < f64::EPSILON);
    assert!((config.ev.futures_multiplier - 1.0).abs() < f64::EPSILON);
    assert!((config.ev.y_ewma_alpha_mean - 0.08).abs() < f64::EPSILON);
    assert!((config.ev.y_ewma_alpha_var - 0.08).abs() < f64::EPSILON);
    assert!((config.ev.y_min_sigma - 0.001).abs() < f64::EPSILON);
    assert_eq!(config.exit.max_holding_ms, 1_800_000);
    assert!((config.exit.stop_loss_pct - 0.015).abs() < f64::EPSILON);
    assert!(config.exit.enforce_protective_stop);
    assert_eq!(config.ui.price_history_len, 120);
}

#[test]
fn tradable_symbols_dedup_and_include_primary() {
    let cfg = BinanceConfig {
        rest_base_url: "x".to_string(),
        ws_base_url: "y".to_string(),
        futures_rest_base_url: "z".to_string(),
        futures_ws_base_url: "w".to_string(),
        symbol: "btcusdt".to_string(),
        symbols: vec![
            "ETHUSDT".to_string(),
            "BTCUSDT".to_string(),
            "  ".to_string(),
        ],
        futures_symbols: vec![],
        recv_window: 5000,
        kline_interval: "1m".to_string(),
        api_key: String::new(),
        api_secret: String::new(),
        futures_api_key: String::new(),
        futures_api_secret: String::new(),
    };
    assert_eq!(
        cfg.tradable_symbols(),
        vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]
    );
}

#[test]
fn tradable_instruments_include_futures_labels() {
    let cfg = BinanceConfig {
        rest_base_url: "x".to_string(),
        ws_base_url: "y".to_string(),
        futures_rest_base_url: "z".to_string(),
        futures_ws_base_url: "w".to_string(),
        symbol: "BTCUSDT".to_string(),
        symbols: vec!["ETHUSDT".to_string()],
        futures_symbols: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
        recv_window: 5000,
        kline_interval: "1m".to_string(),
        api_key: String::new(),
        api_secret: String::new(),
        futures_api_key: String::new(),
        futures_api_secret: String::new(),
    };
    assert_eq!(
        cfg.tradable_instruments(),
        vec![
            "BTCUSDT".to_string(),
            "ETHUSDT".to_string(),
            "BTCUSDT (FUT)".to_string(),
            "ETHUSDT (FUT)".to_string()
        ]
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
