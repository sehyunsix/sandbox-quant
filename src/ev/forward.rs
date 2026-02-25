use crate::ev::estimator::EvEstimatorConfig;
use crate::ev::types::{ConfidenceLevel, EntryExpectancySnapshot, ProbabilitySnapshot};

pub fn estimate_forward_expectancy(
    cfg: &EvEstimatorConfig,
    entry_price: f64,
    qty: f64,
    stop_loss_pct: f64,
    target_rr: f64,
    p_win: f64,
    max_holding_ms: u64,
    now_ms: u64,
) -> EntryExpectancySnapshot {
    let entry_price = entry_price.max(0.0);
    let qty = qty.abs();
    let stop_loss_pct = stop_loss_pct.max(0.0);
    let p_win = p_win.clamp(0.0, 1.0);
    let target_rr = target_rr.max(0.0);

    let risk_usdt = entry_price * stop_loss_pct * qty;
    let reward_usdt = risk_usdt * target_rr;
    let ev_raw = p_win * reward_usdt - (1.0 - p_win) * risk_usdt;
    let ev = ev_raw - cfg.fee_slippage_penalty_usdt;

    EntryExpectancySnapshot {
        expected_return_usdt: ev,
        expected_holding_ms: max_holding_ms.max(1),
        worst_case_loss_usdt: risk_usdt,
        fee_slippage_penalty_usdt: cfg.fee_slippage_penalty_usdt,
        probability: ProbabilitySnapshot {
            p_win,
            p_tail_loss: 1.0 - p_win,
            p_timeout_exit: 0.5,
            n_eff: 0.0,
            confidence: ConfidenceLevel::Low,
            prob_model_version: "forward-static-v1".to_string(),
        },
        ev_model_version: "forward-rr-v1".to_string(),
        computed_at_ms: now_ms,
    }
}
