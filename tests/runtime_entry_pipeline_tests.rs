use sandbox_quant::ev::{EvEstimatorConfig, EwmaYModel, EwmaYModelConfig};
use sandbox_quant::model::order::OrderSide;
use sandbox_quant::model::signal::Signal;
use sandbox_quant::order_manager::MarketKind;
use sandbox_quant::runtime::entry_pipeline::{
    estimate_entry_snapshot_for_signal, estimate_open_position_snapshot_for_signal,
    fallback_sigma_for_market, mark_ev_zero_exit_if_needed,
};
use std::collections::HashSet;

#[test]
fn fallback_sigma_uses_market_specific_value() {
    assert!((fallback_sigma_for_market(MarketKind::Spot, 0.01, 0.02) - 0.01).abs() < 1e-12);
    assert!((fallback_sigma_for_market(MarketKind::Futures, 0.01, 0.02) - 0.02).abs() < 1e-12);
}

#[test]
fn entry_snapshot_returns_none_for_invalid_entry_price() {
    let model = EwmaYModel::new(EwmaYModelConfig {
        alpha_mean: 0.1,
        alpha_var: 0.1,
        min_sigma: 0.001,
    });
    let out = estimate_entry_snapshot_for_signal(
        &EvEstimatorConfig::default(),
        &model,
        "BTCUSDT",
        "cfg",
        &Signal::Buy,
        0.0,
        0.01,
        MarketKind::Spot,
        1.0,
        10.0,
        0.0,
        60_000,
        1,
    );
    assert!(out.is_none());
}

#[test]
fn open_position_snapshot_returns_some_for_valid_inputs() {
    let model = EwmaYModel::new(EwmaYModelConfig {
        alpha_mean: 0.1,
        alpha_var: 0.1,
        min_sigma: 0.001,
    });
    let out = estimate_open_position_snapshot_for_signal(
        &EvEstimatorConfig::default(),
        &model,
        "BTCUSDT",
        "cfg",
        &Signal::Sell,
        0.0,
        0.01,
        MarketKind::Futures,
        1.0,
        100.0,
        0.1,
        Some(OrderSide::Buy),
        60_000,
        1,
    );
    assert!(out.is_some());
}

#[test]
fn mark_ev_zero_exit_only_once_per_instrument() {
    let mut enqueued = HashSet::new();
    assert!(mark_ev_zero_exit_if_needed(&mut enqueued, "BTCUSDT", true, -0.1));
    assert!(!mark_ev_zero_exit_if_needed(&mut enqueued, "BTCUSDT", true, -0.2));
    assert!(!mark_ev_zero_exit_if_needed(&mut enqueued, "ETHUSDT", false, -0.1));
}
