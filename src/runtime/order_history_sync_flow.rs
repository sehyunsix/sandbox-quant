use crate::event::{AppEvent, LogDomain, LogLevel, LogRecord};
use crate::model::position::Position;
use crate::model::order::OrderSide;
use crate::order_manager::{MarketKind, OrderHistoryStats, OrderManager};
use std::collections::HashMap;
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

pub async fn process_periodic_sync_basic_for_instrument(
    app_tx: &mpsc::Sender<AppEvent>,
    mgr: &mut OrderManager,
    instrument: &str,
    selected_symbol: &str,
    order_history_limit: usize,
    strategy_stats_by_instrument: &mut HashMap<String, HashMap<String, OrderHistoryStats>>,
    realized_pnl_by_symbol: &mut HashMap<String, f64>,
    stop_loss_pct: f64,
) {
    match mgr.refresh_order_history(order_history_limit).await {
        Ok(history) => {
            if instrument == selected_symbol {
                let _ = app_tx
                    .send(AppEvent::OrderHistoryUpdate(history.clone()))
                    .await;
            }
            strategy_stats_by_instrument
                .insert(instrument.to_string(), history.strategy_stats.clone());
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
