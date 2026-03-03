use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::runtime::strategy_runtime::StrategyRuntime;
use sandbox_quant::strategy_catalog::StrategyProfile;

fn profile(strategy_type: &str) -> StrategyProfile {
    StrategyProfile {
        label: format!("{}(test)", strategy_type.to_ascii_uppercase()),
        source_tag: strategy_type.to_ascii_lowercase(),
        strategy_type: strategy_type.to_string(),
        symbol: "BTCUSDT".to_string(),
        created_at_ms: 0,
        cumulative_running_ms: 0,
        last_started_at_ms: None,
        fast_period: 5,
        slow_period: 20,
        min_ticks_between_signals: 1,
    }
}

#[test]
fn ma_runtime_exposes_fast_and_slow_values_after_warmup() {
    let mut runtime = StrategyRuntime::from_profile(&profile("ma"));
    for p in 100..=130 {
        let _ = runtime.on_tick(&Tick::from_price(p as f64));
    }
    assert!(runtime.fast_sma_value().is_some());
    assert!(runtime.slow_sma_value().is_some());
}

#[test]
fn atr_runtime_indicator_accessors_are_none() {
    let mut runtime = StrategyRuntime::from_profile(&profile("atr"));
    let _ = runtime.on_tick(&Tick::from_price(100.0));
    assert!(runtime.fast_sma_value().is_none());
    assert!(runtime.slow_sma_value().is_none());
}

#[test]
fn runtime_on_tick_returns_a_valid_signal_variant() {
    let mut runtime = StrategyRuntime::from_profile(&profile("ema"));
    let out = runtime.on_tick(&Tick::from_price(100.0));
    assert!(matches!(out, Signal::Buy | Signal::Sell | Signal::Hold));
}
