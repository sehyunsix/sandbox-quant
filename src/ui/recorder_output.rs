use crate::record::manager::{format_mode, RecordRuntimeStatus};
use crate::recorder_app::runtime::RecorderStatus;

pub fn render_recorder_status(header: &str, status: &RecordRuntimeStatus) -> String {
    [
        header.to_string(),
        format!("mode={}", format_mode(status.mode)),
        format!("state={}", status.state),
        format!("desired_running={}", status.desired_running),
        format!("process_alive={}", status.process_alive),
        format!("worker_alive={}", status.worker_alive),
        format!("status_stale={}", status.status_stale),
        format!(
            "heartbeat_age_sec={}",
            status
                .heartbeat_age_sec
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "pid={}",
            status
                .pid
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!("binary_version={}", status.binary_version),
        format!("db_path={}", status.db_path.display()),
        format!(
            "started_at={}",
            status
                .started_at
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!("updated_at={}", status.updated_at.to_rfc3339()),
        format!("manual_symbols={}", status.manual_symbols.len()),
        format!("strategy_symbols={}", status.strategy_symbols.len()),
        format!("watched_symbols={}", status.watched_symbols.len()),
        format!("liquidation_events={}", status.metrics.liquidation_events),
        format!("book_ticker_events={}", status.metrics.book_ticker_events),
        format!("agg_trade_events={}", status.metrics.agg_trade_events),
        format!(
            "derived_kline_1s_bars={}",
            status.metrics.derived_kline_1s_bars
        ),
    ]
    .join("\n")
}

pub fn render_live_recorder_status(header: &str, status: &RecorderStatus) -> String {
    [
        header.to_string(),
        format!("mode={}", format_mode(status.mode)),
        format!("state={}", status.state.as_str()),
        "desired_running=true".to_string(),
        format!("process_alive={}", status.worker_alive),
        format!("worker_alive={}", status.worker_alive),
        "status_stale=false".to_string(),
        "heartbeat_age_sec=0".to_string(),
        "pid=in-process".to_string(),
        format!("binary_version={}", env!("CARGO_PKG_VERSION")),
        format!("db_path={}", status.db_path.display()),
        format!(
            "started_at={}",
            status
                .started_at
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!("updated_at={}", status.updated_at.to_rfc3339()),
        format!("manual_symbols={}", status.manual_symbols.len()),
        format!("strategy_symbols={}", status.strategy_symbols.len()),
        format!("watched_symbols={}", status.watched_symbols.len()),
        format!("liquidation_events={}", status.metrics.liquidation_events),
        format!("book_ticker_events={}", status.metrics.book_ticker_events),
        format!("agg_trade_events={}", status.metrics.agg_trade_events),
        format!(
            "derived_kline_1s_bars={}",
            status.metrics.derived_kline_1s_bars
        ),
    ]
    .join("\n")
}
