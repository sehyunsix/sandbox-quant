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
