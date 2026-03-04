use std::path::PathBuf;

use sandbox_quant::backtest::{
    build_walk_forward_windows, parse_backtest_args, parse_candle_csv, BacktestConfig,
};

#[test]
fn walk_windows_respect_train_test_embargo() {
    let cfg = BacktestConfig {
        train_window: 5,
        test_window: 3,
        embargo_window: 2,
        max_folds: 3,
        ..BacktestConfig::default()
    };
    let windows = build_walk_forward_windows(30, &cfg);
    assert_eq!(windows.len(), 3);
    assert_eq!(windows[0].train_start, 0);
    assert_eq!(windows[0].train_end, 5);
    assert_eq!(windows[0].test_start, 7);
    assert_eq!(windows[0].test_end, 10);
    assert_eq!(windows[1].train_start, 10);
    assert_eq!(windows[1].test_start, 17);
    assert_eq!(windows[2].train_start, 20);
}

#[test]
fn parse_args_requires_bars_file() {
    let args = vec![
        "--symbol".to_string(),
        "BTCUSDT".to_string(),
        "--bars".to_string(),
        "tests/fixtures/backtest-bars.csv".to_string(),
    ];
    assert!(parse_backtest_args(&args).is_err());
}

#[test]
fn parse_csv_with_headers_and_run_smoke() {
    let csv = "\
open_time,open,high,low,close
0,100,101,99,100
60000,101,102,100,101
120000,101,103,100,102
180000,102,104,101,103
240000,103,105,102,104
300000,104,105,103,104
360000,105,106,104,105
420000,105,106,105,106
480000,106,108,105,107
540000,107,108,106,108
600000,109,109,107,108
660000,108,109,106,107
720000,108,109,107,108
780000,109,110,108,109
840000,110,111,109,110
900000,110,112,110,111
960000,111,112,111,112
1020000,112,113,111,112
1080000,113,114,113,113
1140000,114,115,113,114
1200000,115,116,114,115
1260000,116,116,115,116
1320000,117,118,116,117
1380000,117,117,116,116
1440000,116,117,115,116
1500000,117,118,116,117
1560000,118,119,117,118
1620000,118,119,117,117
1680000,117,118,116,116
1740000,117,117,116,116
1800000,116,117,115,115
";

    let path = PathBuf::from("/tmp/sq_backtest_smoke.csv");
    std::fs::write(&path, csv).expect("write smoke csv");
    let feed = parse_candle_csv("BTCUSDT", &path).expect("parse");
    assert_eq!(feed.bars.len(), 31);
    assert_eq!(feed.symbol, "BTCUSDT");
    assert_eq!(feed.interval_ms, 60_000);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn parse_csv_rejects_non_ohlcv_headers() {
    let csv = "\
exchange,environment,asset_class
Alpaca,Paper,Equity
Tradier,Sandbox,Equity
";
    let path = PathBuf::from("/tmp/sq_backtest_non_ohlcv.csv");
    std::fs::write(&path, csv).expect("write non-ohlcv csv");
    let err = parse_candle_csv("BTCUSDT", &path).expect_err("non-ohlcv file should be rejected");
    let msg = err.to_string().to_ascii_lowercase();
    assert!(msg.contains("invalid candle csv header") || msg.contains("need at least 2 valid rows"));
    let _ = std::fs::remove_file(&path);
}
