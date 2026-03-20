use crate::backtest_app::runner::BacktestReport;
use crate::dataset::types::BacktestRunSummaryRow;
use crate::strategy::model::StrategyTemplate;

pub fn render_backtest_run(report: &BacktestReport) -> String {
    let realized_trade_count = report
        .trades
        .iter()
        .filter(|trade| trade.net_pnl.is_some())
        .count();
    let has_dataset_rows = report.dataset.liquidation_events > 0
        || report.dataset.book_ticker_events > 0
        || report.dataset.agg_trade_events > 0
        || report.dataset.derived_kline_1s_bars > 0;
    let mut lines = vec![
        "backtest run".to_string(),
        "[identity]".to_string(),
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
        "[dataset]".to_string(),
        format!("liquidation_events={}", report.dataset.liquidation_events),
        format!("book_ticker_events={}", report.dataset.book_ticker_events),
        format!("agg_trade_events={}", report.dataset.agg_trade_events),
        format!(
            "derived_kline_1s_bars={}",
            report.dataset.derived_kline_1s_bars
        ),
        format!("state={}", report_state(report, has_dataset_rows)),
        "[results]".to_string(),
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
            "summary=state:{} trades:{}/{} pnl:{:.2} equity:{:.2}->{:.2}",
            report_state(report, has_dataset_rows),
            realized_trade_count,
            report.trigger_count,
            report.net_pnl,
            report.starting_equity,
            report.ending_equity
        ),
        "[config]".to_string(),
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
        lines.push("[trades]".to_string());
        lines.push("trades=none".to_string());
    } else {
        lines.push("[trades]".to_string());
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

fn report_state(report: &BacktestReport, has_dataset_rows: bool) -> &'static str {
    if !report.dataset.symbol_found {
        return "symbol_not_found";
    }
    match report.template {
        StrategyTemplate::LiquidationBreakdownShort => {
            if !has_dataset_rows {
                "empty_dataset"
            } else if report.trades.is_empty() {
                "no_trades"
            } else {
                "ok"
            }
        }
        StrategyTemplate::PriceSmaCrossLong => {
            if report.trades.is_empty() {
                "no_trades"
            } else {
                "ok"
            }
        }
        StrategyTemplate::PriceSmaCrossShort => {
            if report.trades.is_empty() {
                "no_trades"
            } else {
                "ok"
            }
        }
        StrategyTemplate::PriceSmaCrossLongFast => {
            if report.trades.is_empty() {
                "no_trades"
            } else {
                "ok"
            }
        }
        StrategyTemplate::PriceSmaCrossShortFast => {
            if report.trades.is_empty() {
                "no_trades"
            } else {
                "ok"
            }
        }
    }
}

pub fn render_backtest_run_list(runs: &[BacktestRunSummaryRow]) -> String {
    let mut lines = vec!["backtest runs".to_string(), format!("count={}", runs.len())];
    if runs.is_empty() {
        lines.push("runs=none".to_string());
    } else {
        lines.extend(runs.iter().map(|run| {
            let state = summarize_run_state(run);
            format!(
                "run_id={} state={} created_at={} mode={} template={} instrument={} from={} to={} triggers={} closed_trades={} open_trades={} wins={} losses={} net_pnl={:.2} ending_equity={:.2}",
                run.run_id,
                state,
                run.created_at,
                run.mode.as_str(),
                run.template,
                run.instrument,
                run.from,
                run.to,
                run.trigger_count,
                run.closed_trades,
                run.open_trades,
                run.wins,
                run.losses,
                run.net_pnl,
                run.ending_equity
            )
        }));
    }
    lines.join("\n")
}

fn summarize_run_state(run: &BacktestRunSummaryRow) -> &'static str {
    if run.closed_trades > 0 {
        "closed_trades"
    } else if run.open_trades > 0 {
        "open_trades"
    } else if run.trigger_count > 0 {
        "triggers_only"
    } else {
        "no_trades"
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{NaiveDate, TimeZone, Utc};

    use super::*;
    use crate::app::bootstrap::BinanceMode;
    use crate::backtest_app::runner::{
        BacktestConfig, BacktestExitReason, BacktestReport, BacktestTrade,
    };
    use crate::dataset::types::BacktestDatasetSummary;
    use crate::strategy::model::StrategyTemplate;

    #[test]
    fn render_backtest_run_marks_empty_dataset() {
        let output = render_backtest_run(&sample_report(
            Vec::new(),
            BacktestDatasetSummary {
                mode: BinanceMode::Demo,
                symbol: "BTCUSDT".to_string(),
                symbol_found: true,
                from: "2026-03-13".to_string(),
                to: "2026-03-13".to_string(),
                liquidation_events: 0,
                book_ticker_events: 0,
                agg_trade_events: 0,
                derived_kline_1s_bars: 0,
            },
        ));

        assert!(output.contains("state=empty_dataset"));
        assert!(output.contains("[dataset]"));
        assert!(output.contains("[results]"));
        assert!(output.contains("[trades]"));
        assert!(output.contains("trades=none"));
    }

    #[test]
    fn render_backtest_run_marks_no_trades_when_dataset_exists() {
        let output = render_backtest_run(&sample_report(
            Vec::new(),
            BacktestDatasetSummary {
                mode: BinanceMode::Demo,
                symbol: "BTCUSDT".to_string(),
                symbol_found: true,
                from: "2026-03-13".to_string(),
                to: "2026-03-13".to_string(),
                liquidation_events: 1,
                book_ticker_events: 10,
                agg_trade_events: 0,
                derived_kline_1s_bars: 5,
            },
        ));

        assert!(output.contains("state=no_trades"));
    }

    #[test]
    fn render_backtest_run_marks_ok_when_trades_exist() {
        let output = render_backtest_run(&sample_report(
            vec![BacktestTrade {
                trade_id: 1,
                trigger_time: Utc.timestamp_millis_opt(1_000).single().expect("timestamp"),
                entry_time: Utc.timestamp_millis_opt(2_000).single().expect("timestamp"),
                entry_price: 100.0,
                stop_price: 101.0,
                take_profit_price: 98.0,
                qty: 1.0,
                exit_time: Some(Utc.timestamp_millis_opt(3_000).single().expect("timestamp")),
                exit_price: Some(98.0),
                exit_reason: Some(BacktestExitReason::TakeProfit),
                gross_pnl: Some(2.0),
                fees: Some(0.2),
                net_pnl: Some(1.8),
            }],
            BacktestDatasetSummary {
                mode: BinanceMode::Demo,
                symbol: "BTCUSDT".to_string(),
                symbol_found: true,
                from: "2026-03-13".to_string(),
                to: "2026-03-13".to_string(),
                liquidation_events: 1,
                book_ticker_events: 10,
                agg_trade_events: 0,
                derived_kline_1s_bars: 5,
            },
        ));

        assert!(output.contains("state=ok"));
        assert!(output.contains("summary=state:ok"));
        assert!(output.contains("trade id=1"));
    }

    #[test]
    fn render_backtest_run_marks_symbol_not_found() {
        let output = render_backtest_run(&sample_report(
            Vec::new(),
            BacktestDatasetSummary {
                mode: BinanceMode::Demo,
                symbol: "DOESNOTEXISTUSDT".to_string(),
                symbol_found: false,
                from: "2026-03-13".to_string(),
                to: "2026-03-13".to_string(),
                liquidation_events: 0,
                book_ticker_events: 0,
                agg_trade_events: 0,
                derived_kline_1s_bars: 0,
            },
        ));

        assert!(output.contains("state=symbol_not_found"));
    }

    fn sample_report(
        trades: Vec<BacktestTrade>,
        dataset: BacktestDatasetSummary,
    ) -> BacktestReport {
        BacktestReport {
            run_id: Some(7),
            template: StrategyTemplate::LiquidationBreakdownShort,
            instrument: "BTCUSDT".to_string(),
            mode: BinanceMode::Demo,
            from: NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
            to: NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
            db_path: PathBuf::from("var/demo.duckdb"),
            dataset,
            config: BacktestConfig::default(),
            trigger_count: trades.len(),
            wins: trades
                .iter()
                .filter(|trade| trade.net_pnl.unwrap_or_default() > 0.0)
                .count(),
            losses: trades
                .iter()
                .filter(|trade| trade.net_pnl.unwrap_or_default() < 0.0)
                .count(),
            open_trades: trades
                .iter()
                .filter(|trade| trade.net_pnl.is_none())
                .count(),
            skipped_triggers: 0,
            starting_equity: 10_000.0,
            ending_equity: 10_001.8,
            net_pnl: 1.8,
            observed_win_rate: 1.0,
            average_net_pnl: 1.8,
            configured_expected_value: 1.0,
            trades,
        }
    }

    #[test]
    fn render_backtest_run_list_includes_state_and_open_trade_counts() {
        let output = render_backtest_run_list(&[
            BacktestRunSummaryRow {
                run_id: 1,
                created_at: "2026-03-13 10:00:00".to_string(),
                mode: BinanceMode::Demo,
                template: "liquidation-breakdown-short".to_string(),
                instrument: "BTCUSDT".to_string(),
                from: "2026-03-13".to_string(),
                to: "2026-03-13".to_string(),
                trigger_count: 0,
                closed_trades: 0,
                open_trades: 0,
                wins: 0,
                losses: 0,
                net_pnl: 0.0,
                ending_equity: 10_000.0,
            },
            BacktestRunSummaryRow {
                run_id: 2,
                created_at: "2026-03-13 11:00:00".to_string(),
                mode: BinanceMode::Demo,
                template: "liquidation-breakdown-short".to_string(),
                instrument: "ETHUSDT".to_string(),
                from: "2026-03-13".to_string(),
                to: "2026-03-13".to_string(),
                trigger_count: 1,
                closed_trades: 0,
                open_trades: 1,
                wins: 0,
                losses: 0,
                net_pnl: -5.0,
                ending_equity: 9_995.0,
            },
        ]);

        assert!(output.contains("run_id=1 state=no_trades"));
        assert!(output.contains("run_id=2 state=open_trades"));
        assert!(output.contains("open_trades=1"));
    }
}
