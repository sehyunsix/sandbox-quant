use crate::ev::{EvEstimatorConfig, EwmaYModel};
use crate::event::{AppEvent, LogDomain, LogLevel, LogRecord};
use crate::model::order::OrderSide;
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::{MarketKind, OrderHistoryStats, OrderManager};
use crate::runtime::entry_pipeline::{
    estimate_open_position_snapshot_for_signal, fallback_sigma_for_market,
    mark_ev_zero_exit_if_needed,
};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

pub fn market_kind_from_instrument_label(instrument: &str) -> MarketKind {
    if instrument.trim().to_ascii_uppercase().ends_with("(FUT)") {
        MarketKind::Futures
    } else {
        MarketKind::Spot
    }
}

pub fn derived_stop_price_for_position(position: &Position, stop_loss_pct: f64) -> Option<f64> {
    if position.qty.abs() <= f64::EPSILON || position.entry_price <= f64::EPSILON {
        return None;
    }
    let pct = stop_loss_pct.max(0.0);
    match position.side {
        Some(OrderSide::Sell) => Some(position.entry_price * (1.0 + pct)),
        _ => Some(position.entry_price * (1.0 - pct)),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn process_periodic_sync_for_instrument(
    app_tx: &mpsc::Sender<AppEvent>,
    internal_exit_tx: &mpsc::Sender<(String, String)>,
    mgr: &mut OrderManager,
    instrument: &str,
    selected_symbol: &str,
    order_history_limit: usize,
    strategy_stats_by_instrument: &mut HashMap<String, HashMap<String, OrderHistoryStats>>,
    realized_pnl_by_symbol: &mut HashMap<String, f64>,
    ev_zero_exit_enqueued: &mut HashSet<String>,
    ev_enabled: bool,
    ev_mode: &str,
    ev_cfg: &EvEstimatorConfig,
    y_model: &EwmaYModel,
    fallback_mu: f64,
    y_sigma_spot: f64,
    y_sigma_futures: f64,
    futures_multiplier: f64,
    max_holding_ms: u64,
    stop_loss_pct: f64,
) {
    if mgr.position().is_flat() {
        ev_zero_exit_enqueued.remove(instrument);
    }
    match mgr.refresh_order_history(order_history_limit).await {
        Ok(history) => {
            if instrument == selected_symbol {
                let _ = app_tx.send(AppEvent::OrderHistoryUpdate(history.clone())).await;
            }
            strategy_stats_by_instrument.insert(instrument.to_string(), history.strategy_stats.clone());
            realized_pnl_by_symbol.insert(instrument.to_string(), history.stats.realized_pnl);
        }
        Err(e) => {
            let _ = app_tx
                .send(log_event(
                    LogLevel::Warn,
                    LogDomain::Order,
                    "history.sync.fail",
                    format!("Periodic order history sync failed ({}): {}", instrument, e),
                ))
                .await;
        }
    }

    if mgr.position().qty.abs() > f64::EPSILON && mgr.position().entry_price > f64::EPSILON {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let market = market_kind_from_instrument_label(instrument);
        let fallback_sigma = fallback_sigma_for_market(market, y_sigma_spot, y_sigma_futures);
        let Some(snapshot) = estimate_open_position_snapshot_for_signal(
            ev_cfg,
            y_model,
            instrument,
            "sys",
            &Signal::Hold,
            fallback_mu,
            fallback_sigma,
            market,
            futures_multiplier,
            mgr.position().entry_price,
            mgr.position().qty,
            mgr.position().side,
            max_holding_ms,
            now_ms,
        ) else {
            return;
        };
        let ev_value = snapshot.expected_return_usdt;
        let _ = app_tx
            .send(AppEvent::EvSnapshotUpdate {
                symbol: instrument.to_string(),
                source_tag: "sys".to_string(),
                ev: snapshot.expected_return_usdt,
                entry_ev: None,
                p_win: snapshot.probability.p_win,
                gate_mode: ev_mode.to_string(),
                gate_blocked: false,
            })
            .await;
        if mark_ev_zero_exit_if_needed(ev_zero_exit_enqueued, instrument, ev_enabled, ev_value) {
            let _ = app_tx
                .send(log_event(
                    LogLevel::Warn,
                    LogDomain::Risk,
                    "ev.exit.zero",
                    format!("EV<=0 forced exit queued: {} ev={:+.4}", instrument, ev_value),
                ))
                .await;
            let _ = internal_exit_tx
                .send((instrument.to_string(), "exit.ev_non_positive".to_string()))
                .await;
        }
    }

    if let Some(stop_price) = derived_stop_price_for_position(mgr.position(), stop_loss_pct) {
        let _ = app_tx
            .send(AppEvent::ExitPolicyUpdate {
                symbol: instrument.to_string(),
                source_tag: "sys".to_string(),
                stop_price: Some(stop_price),
                expected_holding_ms: None,
                protective_stop_ok: None,
            })
            .await;
    }
}

fn log_event(level: LogLevel, domain: LogDomain, event: &'static str, msg: String) -> AppEvent {
    AppEvent::LogRecord(LogRecord::new(level, domain, event, msg))
}
