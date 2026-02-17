use std::collections::HashMap;

use sandbox_quant::order_store::{
    load_strategy_symbol_stats, persist_strategy_symbol_stats, StrategyScopedStats,
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
