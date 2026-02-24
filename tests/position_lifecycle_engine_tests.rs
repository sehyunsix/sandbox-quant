use sandbox_quant::ev::{ConfidenceLevel, EntryExpectancySnapshot, ProbabilitySnapshot};
use sandbox_quant::lifecycle::{ExitTrigger, PositionLifecycleEngine};

fn expectancy(holding_ms: u64) -> EntryExpectancySnapshot {
    EntryExpectancySnapshot {
        expected_return_usdt: 1.0,
        expected_holding_ms: holding_ms,
        worst_case_loss_usdt: 2.0,
        fee_slippage_penalty_usdt: 0.1,
        probability: ProbabilitySnapshot {
            p_win: 0.55,
            p_tail_loss: 0.08,
            p_timeout_exit: 0.3,
            n_eff: 25.0,
            confidence: ConfidenceLevel::Medium,
            prob_model_version: "test".to_string(),
        },
        ev_model_version: "test".to_string(),
        computed_at_ms: 100,
    }
}

#[test]
fn engine_tracks_mfe_and_mae() {
    let mut engine = PositionLifecycleEngine::default();
    let exp = expectancy(10_000);
    engine.on_entry_filled("BTCUSDT", "cfg", 100.0, 2.0, &exp, 1_000);

    assert_eq!(engine.on_tick("BTCUSDT", 101.0, 2_000), None);
    assert_eq!(engine.on_tick("BTCUSDT", 97.0, 3_000), None);

    let state = engine
        .on_position_closed("BTCUSDT")
        .expect("state should exist");
    assert!(state.mfe_usdt > 0.0);
    assert!(state.mae_usdt < 0.0);
}

#[test]
fn engine_emits_max_holding_trigger() {
    let mut engine = PositionLifecycleEngine::default();
    let exp = expectancy(1_000);
    engine.on_entry_filled("ETHUSDT", "cfg", 100.0, 1.0, &exp, 1_000);

    let trigger = engine.on_tick("ETHUSDT", 100.5, 2_500);
    assert_eq!(trigger, Some(ExitTrigger::MaxHoldingTime));
}
