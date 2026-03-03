use crate::ev::{
    futures_ev_from_y_normal, spot_ev_from_y_normal, EntryExpectancySnapshot, EvEstimatorConfig,
    EwmaYModel, FuturesEvInputs, PositionSide, SpotEvInputs,
};
use crate::model::order::OrderSide;
use crate::model::signal::Signal;
use crate::order_manager::MarketKind;
use std::collections::HashSet;

pub fn fallback_sigma_for_market(
    market: MarketKind,
    sigma_spot: f64,
    sigma_futures: f64,
) -> f64 {
    if market == MarketKind::Futures {
        sigma_futures
    } else {
        sigma_spot
    }
}

pub fn mark_ev_zero_exit_if_needed(
    ev_zero_exit_enqueued: &mut HashSet<String>,
    instrument: &str,
    ev_enabled: bool,
    ev_value: f64,
) -> bool {
    if ev_enabled && ev_value <= 0.0 && !ev_zero_exit_enqueued.contains(instrument) {
        ev_zero_exit_enqueued.insert(instrument.to_string());
        true
    } else {
        false
    }
}

pub fn estimate_entry_snapshot_for_signal(
    ev_cfg: &EvEstimatorConfig,
    y_model: &EwmaYModel,
    instrument: &str,
    source_tag: &str,
    signal: &Signal,
    fallback_mu: f64,
    fallback_sigma: f64,
    market: MarketKind,
    futures_multiplier: f64,
    order_amount_usdt: f64,
    entry_price: f64,
    max_holding_ms: u64,
    now_ms: u64,
) -> Option<EntryExpectancySnapshot> {
    let y = y_model.estimate_for_signal(
        instrument,
        source_tag,
        signal,
        fallback_mu,
        fallback_sigma,
    );
    snapshot_for_entry_from_y(
        ev_cfg,
        market,
        futures_multiplier,
        y.mu,
        y.sigma,
        order_amount_usdt,
        entry_price,
        max_holding_ms,
        now_ms,
    )
}

pub fn estimate_open_position_snapshot_for_signal(
    ev_cfg: &EvEstimatorConfig,
    y_model: &EwmaYModel,
    instrument: &str,
    source_tag: &str,
    signal: &Signal,
    fallback_mu: f64,
    fallback_sigma: f64,
    market: MarketKind,
    futures_multiplier: f64,
    entry_price: f64,
    qty: f64,
    side: Option<OrderSide>,
    max_holding_ms: u64,
    now_ms: u64,
) -> Option<EntryExpectancySnapshot> {
    let y = y_model.estimate_for_signal(
        instrument,
        source_tag,
        signal,
        fallback_mu,
        fallback_sigma,
    );
    snapshot_for_open_position_from_y(
        ev_cfg,
        market,
        futures_multiplier,
        y.mu,
        y.sigma,
        entry_price,
        qty,
        side,
        max_holding_ms,
        now_ms,
    )
}

fn snapshot_for_entry_from_y(
    ev_cfg: &EvEstimatorConfig,
    market: MarketKind,
    futures_multiplier: f64,
    mu: f64,
    sigma: f64,
    order_amount_usdt: f64,
    entry_price: f64,
    max_holding_ms: u64,
    now_ms: u64,
) -> Option<EntryExpectancySnapshot> {
    if entry_price <= f64::EPSILON || order_amount_usdt <= f64::EPSILON {
        return None;
    }
    let qty = order_amount_usdt / entry_price;
    if qty <= f64::EPSILON {
        return None;
    }
    let stats = if market == MarketKind::Futures {
        futures_ev_from_y_normal(
            crate::ev::YNormal { mu, sigma },
            FuturesEvInputs {
                p0: entry_price,
                qty,
                multiplier: futures_multiplier,
                side: PositionSide::Long,
                fee: 0.0,
                slippage: ev_cfg.fee_slippage_penalty_usdt,
                funding: 0.0,
                liq_risk: 0.0,
            },
        )
    } else {
        spot_ev_from_y_normal(
            crate::ev::YNormal { mu, sigma },
            SpotEvInputs {
                p0: entry_price,
                qty,
                side: PositionSide::Long,
                fee: 0.0,
                slippage: ev_cfg.fee_slippage_penalty_usdt,
                borrow: 0.0,
            },
        )
    };

    Some(EntryExpectancySnapshot {
        expected_return_usdt: stats.ev,
        expected_holding_ms: max_holding_ms.max(1),
        worst_case_loss_usdt: stats.ev_std,
        fee_slippage_penalty_usdt: ev_cfg.fee_slippage_penalty_usdt,
        probability: crate::ev::ProbabilitySnapshot {
            p_win: stats.p_win,
            p_tail_loss: 1.0 - stats.p_win,
            p_timeout_exit: 0.5,
            n_eff: 0.0,
            confidence: crate::ev::ConfidenceLevel::Low,
            prob_model_version: "y-normal-v1".to_string(),
        },
        ev_model_version: "y-normal-spot-fut-v1".to_string(),
        computed_at_ms: now_ms,
    })
}

fn snapshot_for_open_position_from_y(
    ev_cfg: &EvEstimatorConfig,
    market: MarketKind,
    futures_multiplier: f64,
    mu: f64,
    sigma: f64,
    entry_price: f64,
    qty: f64,
    side: Option<OrderSide>,
    max_holding_ms: u64,
    now_ms: u64,
) -> Option<EntryExpectancySnapshot> {
    if entry_price <= f64::EPSILON || qty.abs() <= f64::EPSILON {
        return None;
    }
    let side = match side {
        Some(OrderSide::Sell) => PositionSide::Short,
        _ => PositionSide::Long,
    };
    let stats = if market == MarketKind::Futures {
        futures_ev_from_y_normal(
            crate::ev::YNormal { mu, sigma },
            FuturesEvInputs {
                p0: entry_price,
                qty: qty.abs(),
                multiplier: futures_multiplier,
                side,
                fee: 0.0,
                slippage: ev_cfg.fee_slippage_penalty_usdt,
                funding: 0.0,
                liq_risk: 0.0,
            },
        )
    } else {
        spot_ev_from_y_normal(
            crate::ev::YNormal { mu, sigma },
            SpotEvInputs {
                p0: entry_price,
                qty: qty.abs(),
                side,
                fee: 0.0,
                slippage: ev_cfg.fee_slippage_penalty_usdt,
                borrow: 0.0,
            },
        )
    };

    Some(EntryExpectancySnapshot {
        expected_return_usdt: stats.ev,
        expected_holding_ms: max_holding_ms.max(1),
        worst_case_loss_usdt: stats.ev_std,
        fee_slippage_penalty_usdt: ev_cfg.fee_slippage_penalty_usdt,
        probability: crate::ev::ProbabilitySnapshot {
            p_win: stats.p_win,
            p_tail_loss: 1.0 - stats.p_win,
            p_timeout_exit: 0.5,
            n_eff: 0.0,
            confidence: crate::ev::ConfidenceLevel::Low,
            prob_model_version: "y-normal-v1".to_string(),
        },
        ev_model_version: "y-normal-spot-fut-v1".to_string(),
        computed_at_ms: now_ms,
    })
}
