use sandbox_quant::order_manager::OrderHistoryStats;
use sandbox_quant::ui::ui_projection::UiProjection;
use sandbox_quant::ui::AppState;

fn sample_app_state() -> AppState {
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.ws_connected = true;
    state.current_equity_usdt = Some(1200.0);
    state.history_realized_pnl = 42.5;
    state.position.qty = 0.2;
    state.position.unrealized_pnl = 8.0;
    state.strategy_stats.insert(
        "MA(Config)".to_string(),
        OrderHistoryStats {
            trade_count: 5,
            win_count: 3,
            lose_count: 2,
            realized_pnl: 12.0,
        },
    );
    state.strategy_stats.insert(
        "MA(Fast 5/20)".to_string(),
        OrderHistoryStats {
            trade_count: 4,
            win_count: 2,
            lose_count: 2,
            realized_pnl: -1.5,
        },
    );
    state
}

#[test]
/// Verifies UiProjection baseline shape:
/// new() should initialize all top-level buckets for portfolio/grid/focus rendering.
fn ui_projection_new_starts_empty() {
    let v2 = UiProjection::new();
    assert!(v2.assets.is_empty());
    assert!(v2.strategies.is_empty());
    assert!(v2.matrix.is_empty());
    assert!(v2.focus.symbol.is_none());
    assert!(v2.focus.strategy_id.is_none());
}

#[test]
/// Verifies legacy-to-v2 projection:
/// existing AppState stats should populate portfolio/assets/strategies/matrix/focus fields.
fn ui_projection_from_legacy_maps_core_fields() {
    let legacy = sample_app_state();
    let v2 = UiProjection::from_legacy(&legacy);

    assert_eq!(v2.portfolio.total_equity_usdt, Some(1200.0));
    assert_eq!(v2.portfolio.total_realized_pnl_usdt, 42.5);
    assert_eq!(v2.portfolio.total_unrealized_pnl_usdt, 8.0);
    assert!(v2.portfolio.ws_connected);

    assert_eq!(v2.assets.len(), 1);
    assert_eq!(v2.assets[0].symbol, "BTCUSDT");
    assert!((v2.assets[0].position_qty - 0.2).abs() < f64::EPSILON);

    assert_eq!(v2.strategies.len(), 2);
    assert_eq!(v2.matrix.len(), 2);
    assert_eq!(v2.focus.symbol.as_deref(), Some("BTCUSDT"));
    assert_eq!(v2.focus.strategy_id.as_deref(), Some("MA(Config)"));
}

#[test]
/// Verifies strategy lookup helper:
/// strategy ids should be queryable in O(1)-style map form for table/grid rendering.
fn ui_projection_strategy_lookup_returns_indexed_map() {
    let legacy = sample_app_state();
    let v2 = UiProjection::from_legacy(&legacy);
    let lookup = v2.strategy_lookup();

    let cfg = lookup.get("MA(Config)").expect("missing MA(Config)");
    assert_eq!(cfg.trade_count, 5);
    assert_eq!(cfg.win_count, 3);
    assert_eq!(cfg.lose_count, 2);

    let fast = lookup.get("MA(Fast 5/20)").expect("missing MA(Fast 5/20)");
    assert!((fast.realized_pnl_usdt + 1.5).abs() < f64::EPSILON);
}
