use sandbox_quant::ev::{
    ConfidenceLevel, EntryExpectancySnapshot, EvEstimatorConfig, EwmaYModel, EwmaYModelConfig,
    ProbabilitySnapshot,
};
use sandbox_quant::model::order::OrderSide;
use sandbox_quant::order_manager::MarketKind;
use sandbox_quant::runtime::order_execution_flow::{
    handle_buy_fill_followups, resolve_buy_fill_expectancy, should_track_buy_entry_fill,
    should_track_sell_close,
};
use sandbox_quant::event::AppEvent;
use sandbox_quant::lifecycle::PositionLifecycleEngine;
use sandbox_quant::order_manager::OrderManager;
use sandbox_quant::binance::rest::BinanceRestClient;
use sandbox_quant::config::RiskConfig;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;

fn pending_snapshot(ev: f64) -> EntryExpectancySnapshot {
    EntryExpectancySnapshot {
        expected_return_usdt: ev,
        expected_holding_ms: 60_000,
        worst_case_loss_usdt: 1.0,
        fee_slippage_penalty_usdt: 0.0,
        probability: ProbabilitySnapshot {
            p_win: 0.5,
            p_tail_loss: 0.5,
            p_timeout_exit: 0.5,
            n_eff: 0.0,
            confidence: ConfidenceLevel::Low,
            prob_model_version: "test".to_string(),
        },
        ev_model_version: "test".to_string(),
        computed_at_ms: 1,
    }
}

#[test]
fn track_buy_entry_fill_requires_buy_entry_and_positive_qty() {
    assert!(should_track_buy_entry_fill(true, OrderSide::Buy, true, 0.1));
    assert!(!should_track_buy_entry_fill(true, OrderSide::Sell, true, 0.1));
    assert!(!should_track_buy_entry_fill(true, OrderSide::Buy, false, 0.1));
    assert!(!should_track_buy_entry_fill(true, OrderSide::Buy, true, 0.0));
}

#[test]
fn track_sell_close_requires_sell_and_flat_position() {
    assert!(should_track_sell_close(true, OrderSide::Sell, true));
    assert!(!should_track_sell_close(true, OrderSide::Sell, false));
    assert!(!should_track_sell_close(true, OrderSide::Buy, true));
}

#[test]
fn resolve_buy_fill_prefers_pending_snapshot() {
    let y_model = EwmaYModel::new(EwmaYModelConfig {
        alpha_mean: 0.1,
        alpha_var: 0.1,
        min_sigma: 0.001,
    });
    let out = resolve_buy_fill_expectancy(
        Some(pending_snapshot(0.42)),
        &EvEstimatorConfig::default(),
        &y_model,
        "BTCUSDT",
        "cfg",
        MarketKind::Spot,
        0.0,
        0.01,
        0.02,
        1.0,
        10.0,
        100.0,
        60_000,
        1,
    )
    .expect("pending snapshot should be returned");
    assert!((out.expected_return_usdt - 0.42).abs() < 1e-12);
}

#[tokio::test]
async fn handle_buy_fill_followups_ignores_non_entry_case() {
    let y_model = EwmaYModel::new(EwmaYModelConfig {
        alpha_mean: 0.1,
        alpha_var: 0.1,
        min_sigma: 0.001,
    });
    let (app_tx, mut app_rx) = mpsc::channel::<AppEvent>(8);
    let (exit_tx, mut exit_rx) = mpsc::channel::<(String, String)>(8);
    let mut lifecycle_engine = PositionLifecycleEngine::default();
    let mut triggered = HashSet::new();
    let mut pending = HashMap::new();
    let rest = Arc::new(BinanceRestClient::new(
        "https://api.binance.com",
        "https://fapi.binance.com",
        "k",
        "s",
        "fk",
        "fs",
        5_000,
    ));
    let mut mgr = OrderManager::new(rest, "BTCUSDT", MarketKind::Spot, 10.0, &RiskConfig::default());

    handle_buy_fill_followups(
        &app_tx,
        &exit_tx,
        &mut lifecycle_engine,
        &mut triggered,
        &mut pending,
        &mut mgr,
        "BTCUSDT",
        "cfg",
        100.0,
        0.0,
        false,
        true,
        true,
        "hard",
        &EvEstimatorConfig::default(),
        &y_model,
        MarketKind::Spot,
        0.0,
        0.01,
        0.02,
        1.0,
        10.0,
        60_000,
        false,
        0.01,
    )
    .await;

    assert!(app_rx.try_recv().is_err());
    assert!(exit_rx.try_recv().is_err());
}
