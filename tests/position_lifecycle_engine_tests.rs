use sandbox_quant::lifecycle::{ExitTrigger, PositionLifecycleEngine};

#[test]
fn engine_tracks_mfe_and_mae() {
    let mut engine = PositionLifecycleEngine::default();
    engine.on_entry_filled("BTCUSDT", "cfg", 100.0, 2.0, 10_000, 1_000);

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
    engine.on_entry_filled("ETHUSDT", "cfg", 100.0, 1.0, 1_000, 1_000);

    let trigger = engine.on_tick("ETHUSDT", 100.5, 2_500);
    assert_eq!(trigger, Some(ExitTrigger::MaxHoldingTime));
}
