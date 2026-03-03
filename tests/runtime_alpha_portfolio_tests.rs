use sandbox_quant::model::signal::Signal;
use sandbox_quant::runtime::alpha_portfolio::decide_portfolio_action_from_alpha;

#[test]
fn alpha_portfolio_decision_maps_to_execution_intent() {
    let decision = decide_portfolio_action_from_alpha("BTCUSDT", 42, true, 0.5, 100.0);
    let intent = decision.to_intent("cfg", 100.0, 0.0);
    assert_eq!(intent.symbol, "BTCUSDT");
    assert_eq!(intent.source_tag, "cfg");
    assert_eq!(intent.target_position_ratio, 0.5);
    assert_eq!(intent.position_delta_ratio, 0.5);
    assert_eq!(intent.desired_notional_usdt, 50.0);
    assert_eq!(intent.reason, "portfolio.alpha.entry");
    assert_eq!(intent.timestamp_ms, 42);
}

#[test]
fn intent_effective_signal_uses_target_ratio_first() {
    let decision = decide_portfolio_action_from_alpha("BTCUSDT", 100, true, 0.3, 50.0);
    let intent = decision.to_intent("cfg", 50.0, 0.0);
    assert_eq!(intent.effective_signal(0.05), Signal::Buy);
}

#[test]
fn sizing_scales_target_ratio_by_strength_bucket() {
    let low = decide_portfolio_action_from_alpha("BTCUSDT", 1, true, 0.05, 100.0);
    let high = decide_portfolio_action_from_alpha("BTCUSDT", 1, true, 0.9, 100.0);
    assert_eq!(low.target_position_ratio, 0.1);
    assert_eq!(high.target_position_ratio, 1.0);
}

#[test]
fn effective_signal_holds_when_delta_within_threshold() {
    let decision = decide_portfolio_action_from_alpha("BTCUSDT", 1, true, 0.5, 100.0);
    // target is 0.5 at this strength; current is 0.47 => delta 0.03 < threshold.
    let intent = decision.to_intent("cfg", 100.0, 0.47);
    assert!((intent.position_delta_ratio - 0.03).abs() < 1e-9);
    assert_eq!(intent.effective_signal(0.05), Signal::Hold);
}

#[test]
fn alpha_only_decision_enters_when_alpha_positive() {
    let decision = decide_portfolio_action_from_alpha("BTCUSDT", 1, true, 0.4, 100.0);
    assert_eq!(decision.execution_signal, Signal::Buy);
    assert_eq!(decision.target_position_ratio, 0.5);
}

#[test]
fn alpha_only_decision_exits_when_alpha_non_positive_and_open() {
    let decision = decide_portfolio_action_from_alpha("BTCUSDT", 1, false, 0.0, 100.0);
    assert_eq!(decision.execution_signal, Signal::Sell);
    assert_eq!(decision.target_position_ratio, 0.0);
}
