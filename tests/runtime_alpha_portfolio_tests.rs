use sandbox_quant::event::{MarketRegime, MarketRegimeSignal};
use sandbox_quant::model::signal::Signal;
use sandbox_quant::runtime::alpha_portfolio::{
    decide_portfolio_action_from_alpha, RegimeDecisionConfig,
};

fn trend_up_regime(confidence: f64) -> MarketRegimeSignal {
    MarketRegimeSignal {
        regime: MarketRegime::TrendUp,
        confidence,
        ema_fast: 0.0,
        ema_slow: 0.0,
        vol_ratio: 0.0,
        slope: 0.0,
        updated_at_ms: 0,
    }
}

fn down_regime(confidence: f64) -> MarketRegimeSignal {
    MarketRegimeSignal {
        regime: MarketRegime::TrendDown,
        confidence,
        ema_fast: 0.0,
        ema_slow: 0.0,
        vol_ratio: 0.0,
        slope: 0.0,
        updated_at_ms: 0,
    }
}

fn unknown_regime(confidence: f64) -> MarketRegimeSignal {
    MarketRegimeSignal {
        regime: MarketRegime::Unknown,
        confidence,
        ema_fast: 0.0,
        ema_slow: 0.0,
        vol_ratio: 0.0,
        slope: 0.0,
        updated_at_ms: 0,
    }
}

fn enabled_regime_cfg() -> RegimeDecisionConfig {
    RegimeDecisionConfig {
        enabled: true,
        confidence_min: 0.0,
        entry_multiplier_trend_up: 1.0,
        entry_multiplier_range: 0.5,
        entry_multiplier_trend_down: 0.0,
        entry_multiplier_unknown: 0.0,
        hold_multiplier_trend_up: 1.0,
        hold_multiplier_range: 0.5,
        hold_multiplier_trend_down: 0.75,
        hold_multiplier_unknown: 0.75,
    }
}

fn disabled_regime_cfg() -> RegimeDecisionConfig {
    RegimeDecisionConfig::disabled()
}

#[test]
fn alpha_portfolio_decision_maps_to_execution_intent() {
    let decision = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        42,
        true,
        0.5,
        100.0,
        trend_up_regime(1.0),
        enabled_regime_cfg(),
    );
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
    let decision = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        100,
        true,
        0.3,
        50.0,
        trend_up_regime(1.0),
        enabled_regime_cfg(),
    );
    let intent = decision.to_intent("cfg", 50.0, 0.0);
    assert_eq!(intent.effective_signal(0.05), Signal::Buy);
}

#[test]
fn sizing_scales_target_ratio_by_strength_bucket() {
    let low = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        1,
        true,
        0.05,
        100.0,
        trend_up_regime(1.0),
        enabled_regime_cfg(),
    );
    let high = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        1,
        true,
        0.9,
        100.0,
        trend_up_regime(1.0),
        enabled_regime_cfg(),
    );
    assert_eq!(low.target_position_ratio, 0.0);
    assert_eq!(high.target_position_ratio, 1.0);
    assert!(high.target_position_ratio > low.target_position_ratio);
}

#[test]
fn effective_signal_holds_when_delta_within_threshold() {
    let decision = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        1,
        true,
        0.5,
        100.0,
        trend_up_regime(1.0),
        enabled_regime_cfg(),
    );
    // target is 0.5 at this strength; current is 0.47 => delta 0.03 < threshold.
    let intent = decision.to_intent("cfg", 100.0, 0.47);
    assert!((intent.position_delta_ratio - 0.03).abs() < 1e-9);
    assert_eq!(intent.effective_signal(0.05), Signal::Hold);
}

#[test]
fn alpha_only_decision_enters_when_alpha_positive() {
    let decision = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        1,
        true,
        0.4,
        100.0,
        trend_up_regime(1.0),
        enabled_regime_cfg(),
    );
    assert_eq!(decision.execution_signal, Signal::Buy);
    assert_eq!(decision.target_position_ratio, 0.5);
}

#[test]
fn alpha_only_decision_exits_when_alpha_non_positive_and_open() {
    let decision = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        1,
        false,
        0.0,
        100.0,
        trend_up_regime(1.0),
        enabled_regime_cfg(),
    );
    assert_eq!(decision.execution_signal, Signal::Sell);
    assert_eq!(decision.target_position_ratio, 0.0);
}

#[test]
fn trend_down_regime_blocks_new_entries() {
    let decision = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        1,
        true,
        0.8,
        100.0,
        down_regime(1.0),
        enabled_regime_cfg(),
    );
    assert_eq!(decision.execution_signal, Signal::Hold);
    assert_eq!(decision.target_position_ratio, 0.0);
}

#[test]
fn unknown_regime_blocks_new_entries() {
    let decision = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        1,
        true,
        0.8,
        100.0,
        unknown_regime(0.9),
        enabled_regime_cfg(),
    );
    assert_eq!(decision.execution_signal, Signal::Hold);
    assert_eq!(decision.target_position_ratio, 0.0);
}

#[test]
fn regime_confidence_gate_blocks_low_confidence_entry() {
    let mut cfg = enabled_regime_cfg();
    cfg.confidence_min = 0.75;
    let decision = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        1,
        true,
        0.9,
        100.0,
        trend_up_regime(0.5),
        cfg,
    );
    assert_eq!(decision.execution_signal, Signal::Hold);
    assert_eq!(decision.target_position_ratio, 0.0);
}

#[test]
fn regime_gate_can_be_disabled_for_rollback() {
    let decision = decide_portfolio_action_from_alpha(
        "BTCUSDT",
        1,
        true,
        0.4,
        100.0,
        down_regime(0.1),
        disabled_regime_cfg(),
    );
    assert_eq!(decision.execution_signal, Signal::Buy);
    assert!(decision.target_position_ratio > 0.0);
}
