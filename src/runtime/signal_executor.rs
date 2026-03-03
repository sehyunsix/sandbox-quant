use crate::ev::{EntryExpectancySnapshot, EvEstimatorConfig, EwmaYModel};
use crate::model::order::OrderSide;
use crate::model::signal::Signal;
use crate::order_manager::MarketKind;
use crate::runtime::entry_pipeline::{
    estimate_entry_snapshot_for_signal, estimate_open_position_snapshot_for_signal,
};
use crate::runtime::predictor_policy::{decide_buy_entry_policy, PredictorEntryPolicyDecision};

#[derive(Debug, Clone)]
pub struct BuyEvEvaluation {
    pub snapshot: EntryExpectancySnapshot,
    pub decision: PredictorEntryPolicyDecision,
}

pub fn evaluate_prequeue_buy_entry(
    ev_cfg: &EvEstimatorConfig,
    y_model: &EwmaYModel,
    instrument: &str,
    source_tag: &str,
    market: MarketKind,
    fallback_mu: f64,
    fallback_sigma: f64,
    futures_multiplier: f64,
    order_amount_usdt: f64,
    entry_price: f64,
    max_holding_ms: u64,
    now_ms: u64,
    gate_mode: &str,
    entry_gate_min_ev_usdt: f64,
    is_buy_entry_attempt: bool,
) -> Option<BuyEvEvaluation> {
    let snapshot = estimate_entry_snapshot_for_signal(
        ev_cfg,
        y_model,
        instrument,
        source_tag,
        &Signal::Buy,
        fallback_mu,
        fallback_sigma,
        market,
        futures_multiplier,
        order_amount_usdt,
        entry_price,
        max_holding_ms,
        now_ms,
    )?;
    let decision = decide_buy_entry_policy(
        &snapshot,
        gate_mode,
        entry_gate_min_ev_usdt,
        is_buy_entry_attempt,
    );
    Some(BuyEvEvaluation { snapshot, decision })
}

#[allow(clippy::too_many_arguments)]
pub fn evaluate_risk_buy_signal(
    ev_cfg: &EvEstimatorConfig,
    y_model: &EwmaYModel,
    instrument: &str,
    source_tag: &str,
    signal: &Signal,
    market: MarketKind,
    fallback_mu: f64,
    fallback_sigma: f64,
    futures_multiplier: f64,
    order_amount_usdt: f64,
    max_holding_ms: u64,
    now_ms: u64,
    gate_mode: &str,
    entry_gate_min_ev_usdt: f64,
    is_buy_entry_attempt: bool,
    pending_entry_expectancy: Option<EntryExpectancySnapshot>,
    last_price: Option<f64>,
    position_entry_price: f64,
    position_qty: f64,
    position_side: Option<OrderSide>,
) -> Option<BuyEvEvaluation> {
    let snapshot = if is_buy_entry_attempt {
        pending_entry_expectancy.or_else(|| {
            let price = last_price?;
            estimate_entry_snapshot_for_signal(
                ev_cfg,
                y_model,
                instrument,
                source_tag,
                &Signal::Buy,
                fallback_mu,
                fallback_sigma,
                market,
                futures_multiplier,
                order_amount_usdt,
                price,
                max_holding_ms,
                now_ms,
            )
        })
    } else if position_qty.abs() > f64::EPSILON && position_entry_price > f64::EPSILON {
        estimate_open_position_snapshot_for_signal(
            ev_cfg,
            y_model,
            instrument,
            source_tag,
            signal,
            fallback_mu,
            fallback_sigma,
            market,
            futures_multiplier,
            position_entry_price,
            position_qty,
            position_side,
            max_holding_ms,
            now_ms,
        )
    } else {
        None
    }?;

    let decision = decide_buy_entry_policy(
        &snapshot,
        gate_mode,
        entry_gate_min_ev_usdt,
        is_buy_entry_attempt,
    );
    Some(BuyEvEvaluation { snapshot, decision })
}
