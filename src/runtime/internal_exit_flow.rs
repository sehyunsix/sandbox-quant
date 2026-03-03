use std::collections::HashMap;

use crate::event::{AppEvent, LogDomain, LogLevel, LogRecord};
use crate::lifecycle::PositionLifecycleEngine;
use crate::order_manager::{OrderHistoryStats, OrderManager, OrderUpdate};
use std::collections::HashSet;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseAllUpdate {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub finished: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InternalExitProcessResult {
    pub emit_asset_snapshot: bool,
    pub emit_rate_snapshot: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloseAttemptOutcome {
    pub close_failed_reason: Option<String>,
    pub close_reject_code: Option<String>,
}

pub fn classify_close_update(update: &OrderUpdate) -> CloseAttemptOutcome {
    match update {
        OrderUpdate::Rejected {
            reason_code,
            reason,
            ..
        } => CloseAttemptOutcome {
            close_failed_reason: Some(reason.clone()),
            close_reject_code: Some(reason_code.clone()),
        },
        _ => CloseAttemptOutcome::default(),
    }
}

pub fn advance_close_all_job(
    close_all_jobs: &mut HashMap<u64, (usize, usize, usize)>,
    job_id: u64,
    close_failed_reason: Option<&str>,
    close_reject_code: Option<&str>,
    is_soft_skip_reason: fn(&str) -> bool,
) -> Option<CloseAllUpdate> {
    let (total, completed, failed) = if let Some(state) = close_all_jobs.get_mut(&job_id) {
        state.1 = state.1.saturating_add(1);
        let is_soft_skip = close_reject_code.map(is_soft_skip_reason).unwrap_or(false);
        if close_failed_reason.is_some() && !is_soft_skip {
            state.2 = state.2.saturating_add(1);
        }
        (state.0, state.1, state.2)
    } else {
        (0, 0, 0)
    };

    if total == 0 && completed == 0 {
        return None;
    }
    let finished = completed >= total;
    if finished {
        close_all_jobs.remove(&job_id);
    }
    Some(CloseAllUpdate {
        total,
        completed,
        failed,
        finished,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn process_internal_exit_for_instrument(
    app_tx: &mpsc::Sender<AppEvent>,
    mgr: &mut OrderManager,
    instrument: &str,
    source_tag_lc: &str,
    reason_code: &str,
    selected_symbol: &str,
    order_history_limit: usize,
    close_all_job_id: Option<u64>,
    close_all_jobs: &mut HashMap<u64, (usize, usize, usize)>,
    strategy_stats_by_instrument: &mut HashMap<String, HashMap<String, OrderHistoryStats>>,
    realized_pnl_by_symbol: &mut HashMap<String, f64>,
    lifecycle_triggered_once: &mut HashSet<String>,
    lifecycle_engine: &mut PositionLifecycleEngine,
    close_all_soft_skip_reason: fn(&str) -> bool,
    build_scoped_stats: fn(
        &HashMap<String, HashMap<String, OrderHistoryStats>>,
    ) -> HashMap<String, OrderHistoryStats>,
) -> InternalExitProcessResult {
    let mut close_failed_reason: Option<String> = None;
    let mut close_reject_code: Option<String> = None;
    let mut result = InternalExitProcessResult::default();

    match mgr
        .emergency_close_position(source_tag_lc, reason_code)
        .await
    {
        Ok(Some(ref update)) => {
            let outcome = classify_close_update(update);
            close_reject_code = outcome.close_reject_code;
            close_failed_reason = outcome.close_failed_reason;
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
            if let OrderUpdate::Filled { .. } = update {
                lifecycle_triggered_once.remove(instrument);
                if let Some(state) = lifecycle_engine.on_position_closed(instrument) {
                    let _ = app_tx
                        .send(log_event(
                            LogLevel::Info,
                            LogDomain::Risk,
                            "lifecycle.close.internal",
                            format!(
                                "Lifecycle internal close: {} pos={} reason={} mfe={:+.4} mae={:+.4}",
                                instrument, state.position_id, reason_code, state.mfe_usdt, state.mae_usdt
                            ),
                        ))
                        .await;
                }
                if let Ok(balances) = mgr.refresh_balances().await {
                    if instrument == selected_symbol {
                        let _ = app_tx.send(AppEvent::BalanceUpdate(balances)).await;
                    }
                }
            }
            result.emit_asset_snapshot = true;
            result.emit_rate_snapshot = true;
        }
        Ok(None) => {}
        Err(e) => {
            close_failed_reason = Some(e.to_string());
            let _ = app_tx.send(AppEvent::Error(e.to_string())).await;
        }
    }

    if let Some(job_id) = close_all_job_id {
        if let Some(update) = advance_close_all_job(
            close_all_jobs,
            job_id,
            close_failed_reason.as_deref(),
            close_reject_code.as_deref(),
            close_all_soft_skip_reason,
        ) {
            let _ = app_tx
                .send(AppEvent::CloseAllProgress {
                    job_id,
                    symbol: instrument.to_string(),
                    completed: update.completed,
                    total: update.total,
                    failed: update.failed,
                    reason: close_failed_reason.clone(),
                })
                .await;
            if update.finished {
                let _ = app_tx
                    .send(AppEvent::CloseAllFinished {
                        job_id,
                        completed: update.completed,
                        total: update.total,
                        failed: update.failed,
                    })
                    .await;
            }
        }
    }

    result
}

fn log_event(level: LogLevel, domain: LogDomain, event: &'static str, msg: String) -> AppEvent {
    AppEvent::LogRecord(LogRecord::new(level, domain, event, msg))
}
