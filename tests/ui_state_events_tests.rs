use sandbox_quant::event::AppEvent;
use sandbox_quant::model::signal::Signal;
use sandbox_quant::risk_module::RateBudgetSnapshot;
use sandbox_quant::ui::AppState;

#[test]
/// Verifies V2 projection is refreshed on legacy state events:
/// applying strategy signal/history inputs should keep AppStateV2 focus and portfolio in sync.
fn app_state_rebuilds_v2_after_events() {
    let mut s = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    s.current_equity_usdt = Some(1000.0);
    s.apply(AppEvent::StrategySignal(Signal::Buy));

    assert_eq!(s.v2_state.focus.symbol.as_deref(), Some("BTCUSDT"));
    assert_eq!(s.v2_state.focus.strategy_id.as_deref(), Some("MA(Config)"));
    assert_eq!(s.v2_state.portfolio.total_equity_usdt, Some(1000.0));
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
    s.v2_state.focus.symbol = Some("ETHUSDT".to_string());
    s.v2_state.focus.strategy_id = Some("MA(Fast 5/20)".to_string());

    s.apply(AppEvent::LogMessage("heartbeat".to_string()));

    assert_eq!(s.v2_state.focus.symbol.as_deref(), Some("ETHUSDT"));
    assert_eq!(
        s.v2_state.focus.strategy_id.as_deref(),
        Some("MA(Fast 5/20)")
    );
}
