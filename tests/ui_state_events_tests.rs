use sandbox_quant::event::{AppEvent, AssetPnlEntry, LogDomain, LogLevel, LogRecord, WsConnectionStatus};
use sandbox_quant::model::order::OrderSide;
use sandbox_quant::model::signal::Signal;
use sandbox_quant::order_manager::OrderHistoryStats;
use sandbox_quant::order_manager::OrderUpdate;
use sandbox_quant::risk_module::RateBudgetSnapshot;
use sandbox_quant::ui::AppState;

#[test]
/// Verifies projection is refreshed on legacy state events:
/// applying strategy signal/history inputs should keep UiProjection focus and portfolio in sync.
fn app_state_rebuilds_projection_after_events() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    s.current_equity_usdt = Some(1000.0);
    s.apply(AppEvent::StrategySignal {
        signal: Signal::Buy,
        source_tag: "cfg".to_string(),
        price: Some(100.0),
        timestamp_ms: 1,
    });

    assert_eq!(s.ui_projection.focus.symbol.as_deref(), Some("BTCUSDT"));
    assert_eq!(s.ui_projection.focus.strategy_id.as_deref(), Some("MA(Config)"));
    assert_eq!(s.ui_projection.portfolio.total_equity_usdt, Some(1000.0));
}

#[test]
/// Verifies risk/rate snapshot event propagation:
/// UI state should store the latest global/endpoint budget snapshots for heatmap rendering.
fn app_state_updates_risk_rate_snapshots() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    s.apply(AppEvent::RiskRateSnapshot {
        global: RateBudgetSnapshot {
            used: 10,
            limit: 100,
            reset_in_ms: 30_000,
        },
        orders: RateBudgetSnapshot {
            used: 4,
            limit: 20,
            reset_in_ms: 30_000,
        },
        account: RateBudgetSnapshot {
            used: 1,
            limit: 10,
            reset_in_ms: 30_000,
        },
        market_data: RateBudgetSnapshot {
            used: 7,
            limit: 40,
            reset_in_ms: 30_000,
        },
    });

    assert_eq!(s.rate_budget_global.used, 10);
    assert_eq!(s.rate_budget_orders.limit, 20);
    assert_eq!(s.rate_budget_account.used, 1);
    assert_eq!(s.rate_budget_market_data.limit, 40);
}

#[test]
/// Verifies focus drill-down state is sticky across event redraws:
/// once focus is selected, unrelated events must not reset symbol/strategy focus.
fn app_state_preserves_focus_state_across_events() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    s.ui_projection.focus.symbol = Some("ETHUSDT".to_string());
    s.ui_projection.focus.strategy_id = Some("MA(Fast 5/20)".to_string());

    s.apply(AppEvent::LogMessage("heartbeat".to_string()));

    assert_eq!(s.ui_projection.focus.symbol.as_deref(), Some("ETHUSDT"));
    assert_eq!(
        s.ui_projection.focus.strategy_id.as_deref(),
        Some("MA(Fast 5/20)")
    );
}

#[test]
/// Verifies fallback focus reconstruction:
/// if focus values are cleared, the next event-driven rebuild should restore
/// current legacy symbol/strategy as drill-down focus defaults.
fn app_state_restores_default_focus_when_missing() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    s.ui_projection.focus.symbol = None;
    s.ui_projection.focus.strategy_id = None;

    s.apply(AppEvent::LogMessage("refresh".to_string()));

    assert_eq!(s.ui_projection.focus.symbol.as_deref(), Some("BTCUSDT"));
    assert_eq!(s.ui_projection.focus.strategy_id.as_deref(), Some("MA(Config)"));
}

#[test]
/// Verifies RFC-0012 strategy-row telemetry backing state:
/// strategy signal events should update per-source last side/price/timestamp.
fn app_state_tracks_last_strategy_signal_by_source_tag() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    s.apply(AppEvent::StrategySignal {
        signal: Signal::Sell,
        source_tag: "cfg".to_string(),
        price: Some(43210.5),
        timestamp_ms: 777,
    });

    let e = s
        .strategy_last_event_by_tag
        .get("cfg")
        .expect("expected cfg signal telemetry");
    assert_eq!(e.side, OrderSide::Sell);
    assert_eq!(e.price, Some(43210.5));
    assert_eq!(e.timestamp_ms, 777);
    assert!(!e.is_filled);
}

#[test]
/// Verifies RFC-0013 network observability counters:
/// reconnect and dropped-tick events should increment dedicated counters.
fn app_state_tracks_network_reconnect_and_tick_drop_counters() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    s.apply(AppEvent::WsStatus(WsConnectionStatus::Reconnecting {
        attempt: 1,
        delay_ms: 500,
    }));
    s.apply(AppEvent::TickDropped);

    assert_eq!(s.network_reconnect_count, 1);
    assert_eq!(s.network_tick_drop_count, 1);
}

#[test]
/// Verifies fill-latency fallback path:
/// if Filled arrives without Submitted, latency should still be sampled from last signal timestamp.
fn app_state_tracks_fill_latency_without_submitted_event() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    let signal_ts = (chrono::Utc::now().timestamp_millis() as u64).saturating_sub(150);
    s.apply(AppEvent::StrategySignal {
        signal: Signal::Buy,
        source_tag: "cfg".to_string(),
        price: Some(43000.0),
        timestamp_ms: signal_ts,
    });
    s.apply(AppEvent::OrderUpdate(OrderUpdate::Filled {
        intent_id: "intent-test".to_string(),
        client_order_id: "sq-cfg-abcd1234".to_string(),
        side: OrderSide::Buy,
        fills: vec![],
        avg_price: 43010.0,
    }));

    assert!(
        !s.network_fill_latencies_ms.is_empty(),
        "fill latency should be sampled even when Submitted is absent"
    );
}

#[test]
/// Verifies aggregated strategy stats propagation:
/// periodic multi-symbol sync should update grid strategy pnl map via StrategyStatsUpdate.
fn app_state_applies_strategy_stats_update_event() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    let mut stats = std::collections::HashMap::new();
    stats.insert(
        "cfg".to_string(),
        OrderHistoryStats {
            trade_count: 4,
            win_count: 3,
            lose_count: 1,
            realized_pnl: 12.5,
        },
    );
    s.apply(AppEvent::StrategyStatsUpdate {
        strategy_stats: stats,
    });
    let cfg = s.strategy_stats.get("cfg").expect("cfg stats should be present");
    assert_eq!(cfg.trade_count, 4);
    assert!((cfg.realized_pnl - 12.5).abs() < f64::EPSILON);
}

#[test]
/// Verifies asset pnl event propagation:
/// asset table backing map should update on AssetPnlUpdate events.
fn app_state_applies_asset_pnl_update_event() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    let mut by_symbol = std::collections::HashMap::new();
    by_symbol.insert(
        "ETHUSDT".to_string(),
        AssetPnlEntry {
            position_qty: 0.4,
            realized_pnl_usdt: 3.0,
            unrealized_pnl_usdt: 0.9,
        },
    );
    s.apply(AppEvent::AssetPnlUpdate { by_symbol });
    let eth = s
        .asset_pnl_by_symbol
        .get("ETHUSDT")
        .expect("ETHUSDT pnl should be present");
    assert!((eth.realized_pnl_usdt - 3.0).abs() < f64::EPSILON);
}

#[test]
/// Verifies structured log event compatibility:
/// LogRecord should be accepted and rendered into legacy system log lines.
fn app_state_accepts_log_record_event() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    let mut r = LogRecord::new(
        LogLevel::Warn,
        LogDomain::Ws,
        "connect.fail",
        "attempt=3 timeout",
    );
    r.symbol = Some("BTCUSDT".to_string());
    s.apply(AppEvent::LogRecord(r));
    let last = s.log_messages.last().expect("expected formatted log message");
    assert!(last.contains("[WARN]"));
    assert!(last.contains("ws.connect.fail"));
    assert!(last.contains("BTCUSDT"));
}
