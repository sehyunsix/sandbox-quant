use sandbox_quant::ev::{
    ConfidenceLevel, EntryExpectancySnapshot, EvEstimatorConfig, EwmaYModel, EwmaYModelConfig,
    ProbabilitySnapshot,
};
use sandbox_quant::model::signal::Signal;
use sandbox_quant::order_manager::MarketKind;
use sandbox_quant::runtime::signal_executor::{
    evaluate_prequeue_buy_entry, evaluate_risk_buy_signal,
};

fn pending_snapshot(ev: f64) -> EntryExpectancySnapshot {
    EntryExpectancySnapshot {
        expected_return_usdt: ev,
        expected_holding_ms: 60_000,
        worst_case_loss_usdt: 1.0,
        fee_slippage_penalty_usdt: 0.0,
        probability: ProbabilitySnapshot {
            p_win: 0.4,
            p_tail_loss: 0.6,
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
fn risk_buy_signal_uses_pending_snapshot_for_entry_attempt() {
    let y_model = EwmaYModel::new(EwmaYModelConfig {
        alpha_mean: 0.1,
        alpha_var: 0.1,
        min_sigma: 0.001,
    });
    let eval = evaluate_risk_buy_signal(
        &EvEstimatorConfig::default(),
        &y_model,
        "BTCUSDT",
        "cfg",
        &Signal::Buy,
        MarketKind::Spot,
        0.0,
        0.01,
        1.0,
        10.0,
        60_000,
        1,
        "hard",
        0.0,
        true,
        Some(pending_snapshot(-0.10)),
        None,
        0.0,
        0.0,
        None,
    )
    .expect("pending snapshot should be used");

    assert!(eval.decision.gate_blocked);
    assert_eq!(eval.decision.log_event, "ev.entry.gate.block");
}

#[test]
fn risk_buy_signal_returns_none_without_price_or_pending_snapshot() {
    let y_model = EwmaYModel::new(EwmaYModelConfig {
        alpha_mean: 0.1,
        alpha_var: 0.1,
        min_sigma: 0.001,
    });
    let eval = evaluate_risk_buy_signal(
        &EvEstimatorConfig::default(),
        &y_model,
        "BTCUSDT",
        "cfg",
        &Signal::Buy,
        MarketKind::Spot,
        0.0,
        0.01,
        1.0,
        10.0,
        60_000,
        1,
        "hard",
        0.0,
        true,
        None,
        None,
        0.0,
        0.0,
        None,
    );
    assert!(eval.is_none());
}

#[test]
fn prequeue_buy_entry_returns_snapshot_and_decision() {
    let y_model = EwmaYModel::new(EwmaYModelConfig {
        alpha_mean: 0.1,
        alpha_var: 0.1,
        min_sigma: 0.001,
    });
    let eval = evaluate_prequeue_buy_entry(
        &EvEstimatorConfig::default(),
        &y_model,
        "BTCUSDT",
        "cfg",
        MarketKind::Spot,
        0.0,
        0.01,
        1.0,
        10.0,
        100.0,
        60_000,
        1,
        "soft",
        0.0,
        true,
    );
    assert!(eval.is_some());
}
