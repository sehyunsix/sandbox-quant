use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::event::{AppEvent, LogDomain, LogLevel, LogRecord};
use crate::order_manager::{OrderHistoryStats, OrderManager};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutionIntentProcessResult {
    pub emit_asset_snapshot: bool,
    pub emit_rate_snapshot: bool,
}

#[allow(clippy::too_many_arguments)]
pub async fn process_execution_intent_for_instrument(
    app_tx: &mpsc::Sender<AppEvent>,
    mgr: &mut OrderManager,
    instrument: &str,
    source_tag: &str,
    signal: crate::model::signal::Signal,
    selected_symbol: &str,
    order_history_limit: usize,
    strategy_stats_by_instrument: &mut HashMap<String, HashMap<String, OrderHistoryStats>>,
    realized_pnl_by_symbol: &mut HashMap<String, f64>,
    build_scoped_stats: fn(
        &HashMap<String, HashMap<String, OrderHistoryStats>>,
    ) -> HashMap<String, OrderHistoryStats>,
) -> ExecutionIntentProcessResult {
    let mut result = ExecutionIntentProcessResult::default();
    let source_tag_lc = source_tag.to_ascii_lowercase();

    match mgr.submit_order(signal, &source_tag_lc).await {
        Ok(Some(ref update)) => {
            if instrument == selected_symbol {
                let _ = app_tx.send(AppEvent::OrderUpdate(update.clone())).await;
            }
            match mgr.refresh_order_history(order_history_limit).await {
                Ok(history) => {
                    strategy_stats_by_instrument
                        .insert(instrument.to_string(), history.strategy_stats.clone());
                    realized_pnl_by_symbol
                        .insert(instrument.to_string(), history.stats.realized_pnl);
                    if instrument == selected_symbol {
                        let _ = app_tx.send(AppEvent::OrderHistoryUpdate(history)).await;
                    }
                    let _ = app_tx
                        .send(AppEvent::StrategyStatsUpdate {
                            strategy_stats: build_scoped_stats(strategy_stats_by_instrument),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = app_tx
                        .send(log_event(
                            LogLevel::Warn,
                            LogDomain::Order,
                            "history.refresh.fail",
                            format!("Order history refresh failed: {}", e),
                        ))
                        .await;
                }
            }
            if let Ok(balances) = mgr.refresh_balances().await {
                if instrument == selected_symbol {
                    let _ = app_tx.send(AppEvent::BalanceUpdate(balances)).await;
                }
            }
            result.emit_asset_snapshot = true;
            result.emit_rate_snapshot = true;
        }
        Ok(None) => {}
        Err(e) => {
            let _ = app_tx.send(AppEvent::Error(e.to_string())).await;
        }
    }

    result
}

fn log_event(level: LogLevel, domain: LogDomain, event: &'static str, msg: String) -> AppEvent {
    AppEvent::LogRecord(LogRecord::new(level, domain, event, msg))
}
