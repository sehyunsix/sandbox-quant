use crate::recorder_app::runtime::RecorderStatus;

pub fn render_live_recorder_status(header: &str, status: &RecorderStatus) -> String {
    let mut lines = vec![
        header.to_string(),
        format!("mode={}", status.mode.as_str()),
        format!("state={}", status.state.as_str()),
        format!("desired_running={}", status.state.is_running()),
        format!("process_alive={}", status.worker_alive),
        format!("worker_alive={}", status.worker_alive),
        format!("status_stale={}", status.heartbeat_age_sec > 5),
        format!("heartbeat_age_sec={}", status.heartbeat_age_sec),
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
        format!(
            "last_liquidation_event_time={}",
            status
                .metrics
                .last_liquidation_event_time
                .clone()
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "last_book_ticker_event_time={}",
            status
                .metrics
                .last_book_ticker_event_time
                .clone()
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "last_agg_trade_event_time={}",
            status
                .metrics
                .last_agg_trade_event_time
                .clone()
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "top_liquidation_symbols={}",
            join_symbols(&status.metrics.top_liquidation_symbols)
        ),
        format!(
            "top_book_ticker_symbols={}",
            join_symbols(&status.metrics.top_book_ticker_symbols)
        ),
        format!(
            "top_agg_trade_symbols={}",
            join_symbols(&status.metrics.top_agg_trade_symbols)
        ),
    ];
    if let Some(error) = &status.last_error {
        lines.push(format!("last_error={error}"));
    }
    lines.join("\n")
}

fn join_symbols(symbols: &[String]) -> String {
    if symbols.is_empty() {
        "none".to_string()
    } else {
        symbols.join(", ")
    }
}
