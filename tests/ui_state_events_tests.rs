use sandbox_quant::event::{AppEvent, WsConnectionStatus};
use sandbox_quant::model::order::OrderSide;
use sandbox_quant::model::signal::Signal;
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
