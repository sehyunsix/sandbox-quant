use crate::backtest_app::runner::BacktestReport;
use crate::dataset::types::BacktestRunSummaryRow;

pub fn render_backtest_run(report: &BacktestReport) -> String {
    let realized_trade_count = report
        .trades
        .iter()
        .filter(|trade| trade.net_pnl.is_some())
        .count();
    let mut lines = vec![
        "backtest run".to_string(),
        format!(
            "run_id={}",
            report
                .run_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!("mode={}", report.mode.as_str()),
        format!("template={}", report.template.slug()),
        format!("instrument={}", report.instrument),
        format!("from={}", report.from),
        format!("to={}", report.to),
        format!("db_path={}", report.db_path.display()),
        format!("liquidation_events={}", report.dataset.liquidation_events),
        format!("book_ticker_events={}", report.dataset.book_ticker_events),
        format!("agg_trade_events={}", report.dataset.agg_trade_events),
        format!("derived_kline_1s_bars={}", report.dataset.derived_kline_1s_bars),
        format!("trigger_count={}", report.trigger_count),
        format!("closed_trades={}", realized_trade_count),
        format!("open_trades={}", report.open_trades),
        format!("wins={}", report.wins),
        format!("losses={}", report.losses),
        format!("skipped_triggers={}", report.skipped_triggers),
        format!("starting_equity={:.2}", report.starting_equity),
        format!("ending_equity={:.2}", report.ending_equity),
        format!("net_pnl={:.2}", report.net_pnl),
        format!("observed_win_rate={:.4}", report.observed_win_rate),
        format!("average_net_pnl={:.2}", report.average_net_pnl),
        format!(
            "configured_expected_value={:.2}",
            report.configured_expected_value
        ),
        format!("risk_pct={}", report.config.risk_pct),
        format!("win_rate_assumption={}", report.config.win_rate_assumption),
        format!("r_multiple={}", report.config.r_multiple),
        format!(
            "max_entry_slippage_pct={}",
            report.config.max_entry_slippage_pct
        ),
        format!("stop_distance_pct={}", report.config.stop_distance_pct),
    ];

    if report.trades.is_empty() {
        lines.push("trades=none".to_string());
    } else {
        for trade in report.trades.iter().take(5) {
            lines.push(format!(
                "trade id={} entry_time={} entry_price={:.4} stop={:.4} tp={:.4} exit_reason={} net_pnl={}",
                trade.trade_id,
                trade.entry_time.to_rfc3339(),
                trade.entry_price,
                trade.stop_price,
                trade.take_profit_price,
                trade
                    .exit_reason
                    .as_ref()
                    .map(|reason| reason.as_str())
                    .unwrap_or("open"),
                trade
                    .net_pnl
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| "open".to_string())
            ));
        }
    }

    lines.join("\n")
}

pub fn render_backtest_run_list(runs: &[BacktestRunSummaryRow]) -> String {
    let mut lines = vec!["backtest runs".to_string(), format!("count={}", runs.len())];
    if runs.is_empty() {
        lines.push("runs=none".to_string());
    } else {
        lines.extend(runs.iter().map(|run| {
            format!(
                "run_id={} created_at={} mode={} template={} instrument={} from={} to={} triggers={} closed_trades={} wins={} losses={} net_pnl={:.2} ending_equity={:.2}",
                run.run_id,
                run.created_at,
                run.mode.as_str(),
                run.template,
                run.instrument,
                run.from,
                run.to,
                run.trigger_count,
                run.closed_trades,
                run.wins,
                run.losses,
                run.net_pnl,
                run.ending_equity
            )
        }));
    }
    lines.join("\n")
}
