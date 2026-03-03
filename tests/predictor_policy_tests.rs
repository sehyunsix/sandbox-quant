use sandbox_quant::ev::{ConfidenceLevel, EntryExpectancySnapshot, ProbabilitySnapshot};
use sandbox_quant::runtime::predictor_policy::decide_buy_entry_policy;

fn snapshot(ev: f64, p_win: f64) -> EntryExpectancySnapshot {
    EntryExpectancySnapshot {
        expected_return_usdt: ev,
        expected_holding_ms: 60_000,
        worst_case_loss_usdt: 1.0,
        fee_slippage_penalty_usdt: 0.0,
        probability: ProbabilitySnapshot {
            p_win,
            p_tail_loss: 1.0 - p_win,
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
fn hard_mode_blocks_low_ev_entry_attempt() {
    let out = decide_buy_entry_policy(&snapshot(-0.10, 0.45), "hard", 0.0, true);
    assert!(out.gate_blocked);
    assert_eq!(out.log_event, "ev.entry.gate.block");
}

#[test]
fn soft_mode_warns_but_does_not_block_entry_attempt() {
    let out = decide_buy_entry_policy(&snapshot(-0.10, 0.45), "soft", 0.0, true);
    assert!(!out.gate_blocked);
    assert_eq!(out.log_event, "ev.entry.gate.soft_warn");
}

#[test]
fn non_entry_signal_uses_signal_snapshot_event() {
    let out = decide_buy_entry_policy(&snapshot(0.20, 0.60), "hard", 0.0, false);
    assert!(!out.gate_blocked);
    assert_eq!(out.log_event, "ev.signal.snapshot");
}

#[test]
fn positive_entry_ev_uses_entry_snapshot_event() {
    let out = decide_buy_entry_policy(&snapshot(0.25, 0.61), "hard", 0.0, true);
    assert!(!out.gate_blocked);
    assert_eq!(out.log_event, "ev.entry.snapshot");
}
