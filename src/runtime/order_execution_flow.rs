use crate::ev::{EntryExpectancySnapshot, EvEstimatorConfig, EwmaYModel};
use crate::event::{AppEvent, LogDomain, LogLevel, LogRecord};
use crate::lifecycle::PositionLifecycleEngine;
use crate::model::order::OrderSide;
use crate::model::signal::Signal;
use crate::order_manager::{MarketKind, OrderManager};
use crate::runtime::entry_pipeline::{
    estimate_entry_snapshot_for_signal, fallback_sigma_for_market,
};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

pub fn should_track_buy_entry_fill(
    ev_enabled: bool,
    side: OrderSide,
    is_buy_entry_attempt: bool,
    filled_qty: f64,
) -> bool {
    ev_enabled && matches!(side, OrderSide::Buy) && is_buy_entry_attempt && filled_qty > f64::EPSILON
}

pub fn should_track_sell_close(ev_enabled: bool, side: OrderSide, is_flat: bool) -> bool {
    ev_enabled && matches!(side, OrderSide::Sell) && is_flat
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_buy_fill_expectancy(
    pending_entry_expectancy: Option<EntryExpectancySnapshot>,
    ev_cfg: &EvEstimatorConfig,
    y_model: &EwmaYModel,
    instrument: &str,
    source_tag: &str,
    market: MarketKind,
    fallback_mu: f64,
    y_sigma_spot: f64,
    y_sigma_futures: f64,
    futures_multiplier: f64,
    order_amount_usdt: f64,
    avg_price: f64,
    max_holding_ms: u64,
    now_ms: u64,
) -> Option<EntryExpectancySnapshot> {
    pending_entry_expectancy.or_else(|| {
        let fallback_sigma = fallback_sigma_for_market(market, y_sigma_spot, y_sigma_futures);
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
            avg_price,
            max_holding_ms,
            now_ms,
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_buy_fill_followups(
    app_tx: &mpsc::Sender<AppEvent>,
    internal_exit_tx: &mpsc::Sender<(String, String)>,
    lifecycle_engine: &mut PositionLifecycleEngine,
    lifecycle_triggered_once: &mut HashSet<String>,
    pending_entry_expectancy: &mut HashMap<String, EntryExpectancySnapshot>,
    mgr: &mut OrderManager,
    instrument: &str,
    source_tag_lc: &str,
    avg_price: f64,
    filled_qty: f64,
    is_buy_entry_attempt: bool,
    ev_enabled: bool,
    ev_shadow_mode: bool,
    ev_mode: &str,
    ev_cfg: &EvEstimatorConfig,
    y_model: &EwmaYModel,
    market: MarketKind,
    fallback_mu: f64,
    y_sigma_spot: f64,
    y_sigma_futures: f64,
    futures_multiplier: f64,
    order_amount_usdt: f64,
    max_holding_ms: u64,
    enforce_protective_stop: bool,
    stop_loss_pct: f64,
) {
    if !should_track_buy_entry_fill(
        ev_enabled,
        OrderSide::Buy,
        is_buy_entry_attempt,
        filled_qty,
    ) {
        return;
    }

    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
    let expectancy = resolve_buy_fill_expectancy(
        pending_entry_expectancy.remove(instrument),
        ev_cfg,
        y_model,
        instrument,
        source_tag_lc,
        market,
        fallback_mu,
        y_sigma_spot,
        y_sigma_futures,
        futures_multiplier,
        order_amount_usdt,
        avg_price,
        max_holding_ms,
        now_ms,
    );

    if let Some(expectancy) = expectancy {
        let position_id = lifecycle_engine.on_entry_filled(
            instrument,
            source_tag_lc,
            avg_price,
            filled_qty,
            &expectancy,
            now_ms,
        );
        let _ = app_tx
            .send(AppEvent::ExitPolicyUpdate {
                symbol: instrument.to_string(),
                source_tag: source_tag_lc.to_string(),
                stop_price: None,
                expected_holding_ms: Some(expectancy.expected_holding_ms),
                protective_stop_ok: None,
            })
            .await;
        let _ = app_tx
            .send(AppEvent::EvSnapshotUpdate {
                symbol: instrument.to_string(),
                source_tag: source_tag_lc.to_string(),
                ev: expectancy.expected_return_usdt,
                entry_ev: Some(expectancy.expected_return_usdt),
                p_win: expectancy.probability.p_win,
                gate_mode: ev_mode.to_string(),
                gate_blocked: false,
            })
            .await;
        lifecycle_triggered_once.remove(instrument);
        let _ = app_tx
            .send(log_event(
                LogLevel::Info,
                "lifecycle.entry",
                format!(
                    "Lifecycle entry tracked: {} pos={} hold_ms={}",
                    instrument, position_id, expectancy.expected_holding_ms
                ),
            ))
            .await;
    }

    if !enforce_protective_stop {
        return;
    }

    let stop_price = avg_price * (1.0 - stop_loss_pct.max(0.0));
    match mgr
        .place_protective_stop_for_open_position(source_tag_lc, stop_price)
        .await
    {
        Ok(Some(stop_order_id)) => {
            lifecycle_engine.set_stop_loss_order_id(instrument, Some(stop_order_id.clone()));
            let _ = app_tx
                .send(AppEvent::ExitPolicyUpdate {
                    symbol: instrument.to_string(),
                    source_tag: source_tag_lc.to_string(),
                    stop_price: Some(stop_price),
                    expected_holding_ms: None,
                    protective_stop_ok: Some(true),
                })
                .await;
            let _ = app_tx
                .send(log_event(
                    LogLevel::Info,
                    "protective_stop.ensure",
                    format!(
                        "Protective stop ensured: {} src={} stop={:.4} order={}",
                        instrument, source_tag_lc, stop_price, stop_order_id
                    ),
                ))
                .await;
        }
        Ok(None) => {
            let _ = app_tx
                .send(AppEvent::ExitPolicyUpdate {
                    symbol: instrument.to_string(),
                    source_tag: source_tag_lc.to_string(),
                    stop_price: Some(stop_price),
                    expected_holding_ms: None,
                    protective_stop_ok: Some(false),
                })
                .await;
            let _ = app_tx
                .send(log_event(
                    LogLevel::Warn,
                    "protective_stop.missing",
                    format!(
                        "Protective stop unavailable: {} src={} stop={:.4}",
                        instrument, source_tag_lc, stop_price
                    ),
                ))
                .await;
            if !ev_shadow_mode {
                let _ = internal_exit_tx
                    .send((instrument.to_string(), "exit.stop_loss_protection".to_string()))
                    .await;
            }
        }
        Err(e) => {
            let _ = app_tx
                .send(AppEvent::ExitPolicyUpdate {
                    symbol: instrument.to_string(),
                    source_tag: source_tag_lc.to_string(),
                    stop_price: Some(stop_price),
                    expected_holding_ms: None,
                    protective_stop_ok: Some(false),
                })
                .await;
            let _ = app_tx
                .send(log_event(
                    LogLevel::Warn,
                    "protective_stop.ensure.fail",
                    format!(
                        "Protective stop ensure failed ({}|{}): {}",
                        instrument, source_tag_lc, e
                    ),
                ))
                .await;
            if !ev_shadow_mode {
                let _ = internal_exit_tx
                    .send((instrument.to_string(), "exit.stop_loss_protection".to_string()))
                    .await;
            }
        }
    }
}

fn log_event(level: LogLevel, event: &'static str, msg: String) -> AppEvent {
    AppEvent::LogRecord(LogRecord::new(level, LogDomain::Risk, event, msg))
}
