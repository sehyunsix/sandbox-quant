use std::path::Path;

use crate::app::bootstrap::BinanceMode;
use crate::backtest_app::runner::{run_backtest_for_path, BacktestExitReason, BacktestTrade};
use crate::dataset::query::{
    backtest_summary_for_path, latest_market_data_day_for_path, load_backtest_report,
    load_backtest_run_summaries, load_book_ticker_rows_for_path, load_derived_kline_rows_for_path,
    load_liquidation_events_for_path, load_raw_kline_rows_for_path, load_recorded_symbols_for_path,
    metrics_for_path, persist_backtest_report,
};
use crate::dataset::schema::init_schema_for_path;
use crate::dataset::types::BacktestDatasetSummary;
use crate::error::storage_error::StorageError;
use crate::record::coordination::RecorderCoordination;
use crate::visualization::types::{
    BacktestRunRequest, DashboardQuery, DashboardSnapshot, EquityPoint, MarketSeries, PricePoint,
    SignalKind, SignalMarker,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct VisualizationService;

impl VisualizationService {
    pub fn load_dashboard(&self, query: DashboardQuery) -> Result<DashboardSnapshot, StorageError> {
        self.load_dashboard_inner(query, None)
    }

    pub fn run_backtest(
        &self,
        request: BacktestRunRequest,
    ) -> Result<DashboardSnapshot, StorageError> {
        let db_path = RecorderCoordination::new(request.base_dir.clone()).db_path(request.mode);
        init_schema_for_path(&db_path)?;
        let report = run_backtest_for_path(
            &db_path,
            request.mode,
            request.template,
            &request.symbol,
            request.from,
            request.to,
            request.config,
        )?;
        let run_id = persist_backtest_report(&db_path, &report)?;
        self.load_dashboard_inner(
            DashboardQuery {
                mode: request.mode,
                base_dir: request.base_dir,
                symbol: request.symbol,
                from: request.from,
                to: request.to,
                selected_run_id: Some(run_id),
                run_limit: request.run_limit,
            },
            None,
        )
    }

    pub fn latest_market_data_day(
        &self,
        mode: BinanceMode,
        base_dir: std::path::PathBuf,
        symbol: &str,
    ) -> Result<Option<chrono::NaiveDate>, StorageError> {
        let db_path = RecorderCoordination::new(base_dir).db_path(mode);
        init_schema_for_path(&db_path)?;
        latest_market_data_day_for_path(&db_path, symbol)
    }

    pub fn price_points(series: &MarketSeries) -> Vec<PricePoint> {
        if !series.klines.is_empty() {
            return series
                .klines
                .iter()
                .map(|row| PricePoint {
                    time_ms: row.close_time_ms,
                    price: row.close,
                })
                .collect();
        }
        series
            .book_tickers
            .iter()
            .map(|row| PricePoint {
                time_ms: row.event_time_ms,
                price: (row.bid + row.ask) * 0.5,
            })
            .collect()
    }

    pub fn equity_curve(starting_equity: f64, trades: &[BacktestTrade]) -> Vec<EquityPoint> {
        let mut equity = starting_equity;
        let mut points = Vec::new();
        for trade in trades {
            if let (Some(exit_time), Some(net_pnl)) = (trade.exit_time, trade.net_pnl) {
                equity += net_pnl;
                points.push(EquityPoint {
                    time_ms: exit_time.timestamp_millis(),
                    equity,
                });
            }
        }
        points
    }

    pub fn signal_markers(trades: &[BacktestTrade]) -> Vec<SignalMarker> {
        let mut markers = Vec::new();
        for trade in trades {
            markers.push(SignalMarker {
                time_ms: trade.entry_time.timestamp_millis(),
                price: trade.entry_price,
                label: format!("entry #{}", trade.trade_id),
                kind: SignalKind::Entry,
            });
            if let (Some(exit_time), Some(exit_price), Some(exit_reason)) = (
                trade.exit_time,
                trade.exit_price,
                trade.exit_reason.as_ref(),
            ) {
                markers.push(SignalMarker {
                    time_ms: exit_time.timestamp_millis(),
                    price: exit_price,
                    label: format!("exit #{}", trade.trade_id),
                    kind: match exit_reason {
                        BacktestExitReason::TakeProfit => SignalKind::TakeProfit,
                        BacktestExitReason::StopLoss => SignalKind::StopLoss,
                        BacktestExitReason::OpenAtEnd => SignalKind::OpenAtEnd,
                        BacktestExitReason::SignalExit => SignalKind::SignalExit,
                    },
                });
            }
        }
        markers
    }

    fn load_dashboard_inner(
        &self,
        query: DashboardQuery,
        selected_report_override: Option<crate::backtest_app::runner::BacktestReport>,
    ) -> Result<DashboardSnapshot, StorageError> {
        let db_path = RecorderCoordination::new(query.base_dir.clone()).db_path(query.mode);
        init_schema_for_path(&db_path)?;
        let recorder_metrics = metrics_for_path(&db_path)?;
        let available_symbols = load_recorded_symbols_for_path(&db_path, 256)?;
        let symbol = resolve_symbol(&query.symbol, &available_symbols);
        let dataset_summary =
            load_dataset_summary(&db_path, query.mode, &symbol, query.from, query.to)?;
        let market_series = load_market_series(&db_path, &symbol, query.from, query.to)?;
        let recent_runs = load_backtest_run_summaries(&db_path, query.run_limit)?;
        let selected_run_id = query.selected_run_id.or_else(|| {
            recent_runs
                .iter()
                .find(|row| row.instrument == symbol)
                .map(|row| row.run_id)
        });
        let selected_report = match selected_report_override {
            Some(report) => Some(report),
            None => match selected_run_id {
                Some(run_id) => load_backtest_report(&db_path, Some(run_id))?,
                None => None,
            },
        };

        Ok(DashboardSnapshot {
            mode: query.mode,
            base_dir: query.base_dir,
            db_path,
            symbol,
            from: query.from,
            to: query.to,
            available_symbols,
            recorder_metrics,
            dataset_summary,
            market_series,
            recent_runs,
            selected_report,
            selected_run_id,
        })
    }
}

fn load_dataset_summary(
    db_path: &Path,
    mode: BinanceMode,
    symbol: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<BacktestDatasetSummary, StorageError> {
    if symbol.is_empty() {
        return Ok(BacktestDatasetSummary {
            mode,
            symbol: String::new(),
            symbol_found: false,
            from: from.to_string(),
            to: to.to_string(),
            liquidation_events: 0,
            book_ticker_events: 0,
            agg_trade_events: 0,
            derived_kline_1s_bars: 0,
        });
    }
    backtest_summary_for_path(db_path, mode, symbol, from, to)
}

fn load_market_series(
    db_path: &Path,
    symbol: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<MarketSeries, StorageError> {
    if symbol.is_empty() {
        return Ok(MarketSeries {
            symbol: String::new(),
            liquidations: Vec::new(),
            book_tickers: Vec::new(),
            klines: Vec::new(),
            kline_interval: None,
        });
    }
    let derived_klines = load_derived_kline_rows_for_path(db_path, symbol, from, to)?;
    let (klines, kline_interval) = if derived_klines.is_empty() {
        match load_raw_kline_rows_for_path(db_path, symbol, from, to)? {
            Some((interval, rows)) => (rows, Some(interval)),
            None => (Vec::new(), None),
        }
    } else {
        (derived_klines, Some("1s".to_string()))
    };
    Ok(MarketSeries {
        symbol: symbol.to_string(),
        liquidations: load_liquidation_events_for_path(db_path, symbol, from, to)?,
        book_tickers: load_book_ticker_rows_for_path(db_path, symbol, from, to)?,
        klines,
        kline_interval,
    })
}

fn resolve_symbol(selected: &str, available_symbols: &[String]) -> String {
    if !selected.trim().is_empty() {
        return selected.trim().to_ascii_uppercase();
    }
    available_symbols.first().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::app::bootstrap::BinanceMode;
    use crate::dataset::schema::init_schema_for_path;
    use chrono::{TimeZone, Utc};
    use duckdb::Connection;

    #[test]
    fn equity_curve_accumulates_realized_trade_pnl() {
        let trades = vec![
            BacktestTrade {
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
            },
            BacktestTrade {
                trade_id: 2,
                trigger_time: Utc.timestamp_millis_opt(4_000).single().expect("timestamp"),
                entry_time: Utc.timestamp_millis_opt(5_000).single().expect("timestamp"),
                entry_price: 99.0,
                stop_price: 100.0,
                take_profit_price: 97.0,
                qty: 1.0,
                exit_time: Some(Utc.timestamp_millis_opt(6_000).single().expect("timestamp")),
                exit_price: Some(100.0),
                exit_reason: Some(BacktestExitReason::StopLoss),
                gross_pnl: Some(-1.0),
                fees: Some(0.2),
                net_pnl: Some(-1.2),
            },
        ];

        let points = VisualizationService::equity_curve(10_000.0, &trades);

        assert_eq!(points.len(), 2);
        assert!((points[0].equity - 10_001.8).abs() < 1e-9);
        assert!((points[1].equity - 10_000.6).abs() < 1e-9);
    }

    #[test]
    fn resolve_symbol_prefers_selected_value() {
        let symbol = resolve_symbol("ethusdt", &["BTCUSDT".to_string()]);

        assert_eq!(symbol, "ETHUSDT");
    }

    #[test]
    fn empty_symbol_summary_uses_requested_range() {
        let from = chrono::NaiveDate::from_ymd_opt(2026, 3, 13).expect("valid date");
        let to = chrono::NaiveDate::from_ymd_opt(2026, 3, 14).expect("valid date");
        let summary = load_dataset_summary(
            &PathBuf::from("/tmp/missing.duckdb"),
            BinanceMode::Demo,
            "",
            from,
            to,
        )
        .expect("summary");

        assert_eq!(summary.mode, BinanceMode::Demo);
        assert_eq!(summary.from, "2026-03-13");
        assert_eq!(summary.to, "2026-03-14");
        assert_eq!(summary.symbol, "");
    }

    #[test]
    fn load_dashboard_falls_back_to_raw_klines_when_derived_klines_are_absent() {
        let mut base_dir = std::env::temp_dir();
        base_dir.push(format!(
            "sandbox_quant_gui_raw_kline_fallback_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&base_dir).expect("create temp dir");
        let db_path = base_dir.join("market-v2-demo.duckdb");
        init_schema_for_path(&db_path).expect("init schema");
        let connection = Connection::open(&db_path).expect("open db");
        connection
            .execute(
                "INSERT INTO raw_klines (
                kline_id, mode, product, symbol, interval, open_time, close_time,
                open, high, low, close, volume, quote_volume, trade_count, raw_payload
             ) VALUES (
                1, 'demo', 'um', 'BTCUSDT', '1m',
                CAST('2026-03-13 00:00:00' AS TIMESTAMP),
                CAST('2026-03-13 00:00:59' AS TIMESTAMP),
                100.0, 101.0, 99.5, 100.5, 10.0, 1005.0, 5, '{}'
             )",
                [],
            )
            .expect("insert raw kline");

        let service = VisualizationService;
        let snapshot = service
            .load_dashboard(DashboardQuery {
                mode: BinanceMode::Demo,
                base_dir: base_dir.clone(),
                symbol: "BTCUSDT".to_string(),
                from: chrono::NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
                to: chrono::NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
                selected_run_id: None,
                run_limit: 10,
            })
            .expect("load dashboard");

        assert_eq!(snapshot.market_series.kline_interval.as_deref(), Some("1m"));
        assert_eq!(snapshot.market_series.klines.len(), 1);

        std::fs::remove_file(db_path).ok();
        std::fs::remove_dir_all(base_dir).ok();
    }
}
