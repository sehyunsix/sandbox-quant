use crate::ev::EntryExpectancySnapshot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PredictorEntryPolicyDecision {
    pub expected_return_usdt: f64,
    pub p_win: f64,
    pub gate_blocked: bool,
    pub log_event: &'static str,
}

pub fn decide_buy_entry_policy(
    snapshot: &EntryExpectancySnapshot,
    gate_mode: &str,
    entry_gate_min_ev_usdt: f64,
    is_buy_entry_attempt: bool,
) -> PredictorEntryPolicyDecision {
    let ev = snapshot.expected_return_usdt;
    let p_win = snapshot.probability.p_win;
    let hard = gate_mode.eq_ignore_ascii_case("hard");
    let soft = gate_mode.eq_ignore_ascii_case("soft");
    let gate_blocked = is_buy_entry_attempt && hard && ev <= entry_gate_min_ev_usdt;
    let log_event = if gate_blocked {
        "ev.entry.gate.block"
    } else if !is_buy_entry_attempt {
        "ev.signal.snapshot"
    } else if soft && ev <= entry_gate_min_ev_usdt {
        "ev.entry.gate.soft_warn"
    } else {
        "ev.entry.snapshot"
    };

    PredictorEntryPolicyDecision {
        expected_return_usdt: ev,
        p_win,
        gate_blocked,
        log_event,
    }
}
