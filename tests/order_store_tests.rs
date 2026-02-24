use std::collections::HashMap;

use sandbox_quant::binance::types::{BinanceAllOrder, BinanceMyTrade};
use sandbox_quant::order_store::{
    load_recent_persisted_trades_filtered, load_strategy_symbol_stats, persist_order_snapshot,
    persist_strategy_symbol_stats, StrategyScopedStats,
};

#[test]
/// Verifies restart-recovery persistence for strategy+symbol scoped stats:
/// rows written to sqlite should be loaded back with the same values.
fn strategy_symbol_stats_round_trip_persistence() {
    let symbol = "UTEST_STATS_ROUNDTRIP";
    let mut stats = HashMap::new();
    stats.insert(
        "MA(Config)".to_string(),
        StrategyScopedStats {
            trade_count: 10,
            win_count: 6,
            lose_count: 4,
            realized_pnl: 12.5,
        },
    );
    stats.insert(
        "MANUAL".to_string(),
        StrategyScopedStats {
            trade_count: 3,
            win_count: 1,
            lose_count: 2,
            realized_pnl: -1.25,
        },
    );

    persist_strategy_symbol_stats(symbol, &stats).expect("persist should succeed");
    let loaded = load_strategy_symbol_stats(symbol).expect("load should succeed");

    assert_eq!(loaded.len(), 2);
    let cfg = loaded.get("MA(Config)").expect("MA(Config) row missing");
    assert_eq!(cfg.trade_count, 10);
    assert_eq!(cfg.win_count, 6);
    assert_eq!(cfg.lose_count, 4);
    assert!((cfg.realized_pnl - 12.5).abs() < f64::EPSILON);

    let manual = loaded.get("MANUAL").expect("MANUAL row missing");
    assert_eq!(manual.trade_count, 3);
    assert_eq!(manual.win_count, 1);
    assert_eq!(manual.lose_count, 2);
    assert!((manual.realized_pnl + 1.25).abs() < f64::EPSILON);
}

#[test]
/// Verifies missing-symbol behavior: loading stats for an unknown symbol should
/// return an empty map (not an error).
fn strategy_symbol_stats_missing_symbol_returns_empty() {
    let symbol = "UTEST_STATS_MISSING_SYMBOL";
    let loaded = load_strategy_symbol_stats(symbol).expect("load should succeed");
    assert!(loaded.is_empty());
}

#[test]
/// Verifies filtered recent trade loader:
/// symbol + source filters should return only matching persisted rows.
fn load_recent_persisted_trades_filtered_by_symbol_and_source() {
    let symbol = "UTEST_EV_SRC_FILTER";
    let orders = vec![BinanceAllOrder {
        symbol: symbol.to_string(),
        order_id: 991001,
        client_order_id: "sq-cfg-test1234".to_string(),
        price: 100.0,
        orig_qty: 1.0,
        executed_qty: 1.0,
        cummulative_quote_qty: 100.0,
        status: "FILLED".to_string(),
        r#type: "MARKET".to_string(),
        side: "BUY".to_string(),
        time: 1_700_000_000_000,
        update_time: 1_700_000_000_100,
    }];
    let trades = vec![BinanceMyTrade {
        symbol: symbol.to_string(),
        id: 991001,
        order_id: 991001,
        price: 100.0,
        qty: 1.0,
        commission: 0.0,
        commission_asset: "USDT".to_string(),
        time: 1_700_000_000_100,
        is_buyer: true,
        is_maker: false,
        realized_pnl: 2.5,
    }];
    persist_order_snapshot(symbol, &orders, &trades).expect("persist snapshot should succeed");

    let rows = load_recent_persisted_trades_filtered(Some(symbol), Some("MA(Config)"), 10)
        .expect("filtered load should succeed");
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|r| r.trade.symbol == symbol));
    assert!(rows.iter().all(|r| r.source.eq_ignore_ascii_case("MA(Config)")));
}
