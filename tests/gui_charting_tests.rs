#![cfg(feature = "gui")]

use std::path::PathBuf;

use chrono::{NaiveDate, TimeZone, Utc};
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::backtest_app::runner::{
    BacktestConfig, BacktestExitReason, BacktestReport, BacktestTrade,
};
use sandbox_quant::charting::adapters::sandbox::{
    equity_scene_from_report, market_scene_from_snapshot,
    market_scene_from_snapshot_with_timeframe, MarketTimeframe,
};
use sandbox_quant::charting::scene::Series;
use sandbox_quant::dataset::types::{
    BacktestDatasetSummary, BacktestRunSummaryRow, BookTickerRow, DerivedKlineRow,
    LiquidationEventRow, RecorderMetrics,
};
use sandbox_quant::strategy::model::StrategyTemplate;
use sandbox_quant::visualization::types::{DashboardSnapshot, MarketSeries};

fn sample_report(symbol: &str) -> BacktestReport {
    let from = NaiveDate::from_ymd_opt(2026, 3, 10).expect("date");
    let to = NaiveDate::from_ymd_opt(2026, 3, 11).expect("date");
    BacktestReport {
        run_id: Some(7),
        template: StrategyTemplate::LiquidationBreakdownShort,
        instrument: symbol.to_string(),
        mode: BinanceMode::Demo,
        from,
        to,
        db_path: PathBuf::from("var/demo.duckdb"),
        dataset: BacktestDatasetSummary {
            mode: BinanceMode::Demo,
            symbol: symbol.to_string(),
            symbol_found: true,
            from: from.to_string(),
            to: to.to_string(),
            liquidation_events: 2,
            book_ticker_events: 3,
            agg_trade_events: 0,
            derived_kline_1s_bars: 2,
        },
        config: BacktestConfig::default(),
        trigger_count: 1,
        trades: vec![BacktestTrade {
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
        wins: 1,
        losses: 0,
        open_trades: 0,
        skipped_triggers: 0,
        starting_equity: 10_000.0,
        ending_equity: 10_001.8,
        net_pnl: 1.8,
        observed_win_rate: 1.0,
        average_net_pnl: 1.8,
        configured_expected_value: 1.0,
    }
}

fn sample_snapshot(
    symbol: &str,
    book_tickers: Vec<BookTickerRow>,
    klines: Vec<DerivedKlineRow>,
    selected_report: Option<BacktestReport>,
) -> DashboardSnapshot {
    let from = NaiveDate::from_ymd_opt(2026, 3, 10).expect("date");
    let to = NaiveDate::from_ymd_opt(2026, 3, 11).expect("date");
    DashboardSnapshot {
        mode: BinanceMode::Demo,
        base_dir: PathBuf::from("var"),
        db_path: PathBuf::from("var/demo.duckdb"),
        symbol: symbol.to_string(),
        from,
        to,
        available_symbols: vec![symbol.to_string()],
        recorder_metrics: RecorderMetrics::default(),
        dataset_summary: BacktestDatasetSummary {
            mode: BinanceMode::Demo,
            symbol: symbol.to_string(),
            symbol_found: true,
            from: from.to_string(),
            to: to.to_string(),
            liquidation_events: 2,
            book_ticker_events: book_tickers.len() as u64,
            agg_trade_events: 0,
            derived_kline_1s_bars: klines.len() as u64,
        },
        market_series: MarketSeries {
            symbol: symbol.to_string(),
            liquidations: vec![LiquidationEventRow {
                event_time_ms: 1_500,
                force_side: "BUY".to_string(),
                price: 101.0,
                qty: 2.0,
                notional: 202.0,
            }],
            book_tickers,
            klines,
            kline_interval: Some("1s".to_string()),
        },
        recent_runs: vec![BacktestRunSummaryRow {
            run_id: 7,
            created_at: "2026-03-11T00:00:00Z".to_string(),
            mode: BinanceMode::Demo,
            template: StrategyTemplate::LiquidationBreakdownShort
                .slug()
                .to_string(),
            instrument: symbol.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            trigger_count: 1,
            closed_trades: 1,
            open_trades: 0,
            wins: 1,
            losses: 0,
            net_pnl: 1.8,
            ending_equity: 10_001.8,
        }],
        selected_run_id: selected_report.as_ref().and_then(|report| report.run_id),
        selected_report,
    }
}

#[test]
fn market_scene_uses_line_series_when_only_book_tickers_exist() {
    let snapshot = sample_snapshot(
        "BTCUSDT",
        vec![
            BookTickerRow {
                event_time_ms: 1_000,
                bid: 99.0,
                ask: 101.0,
            },
            BookTickerRow {
                event_time_ms: 2_000,
                bid: 100.0,
                ask: 102.0,
            },
        ],
        Vec::new(),
        None,
    );

    let scene = market_scene_from_snapshot(&snapshot);

    assert_eq!(scene.panes.len(), 1);
    assert!(matches!(scene.panes[0].series[0], Series::Line(_)));
}

#[test]
fn market_scene_uses_candles_and_volume_when_klines_exist() {
    let snapshot = sample_snapshot(
        "BTCUSDT",
        vec![BookTickerRow {
            event_time_ms: 1_500,
            bid: 101.0,
            ask: 101.5,
        }],
        vec![
            DerivedKlineRow {
                open_time_ms: 1_000,
                close_time_ms: 1_999,
                open: 100.0,
                high: 103.0,
                low: 99.0,
                close: 102.0,
                volume: 25.0,
                quote_volume: 2_550.0,
                trade_count: 10,
            },
            DerivedKlineRow {
                open_time_ms: 2_000,
                close_time_ms: 2_999,
                open: 102.0,
                high: 104.0,
                low: 101.0,
                close: 101.5,
                volume: 20.0,
                quote_volume: 2_035.0,
                trade_count: 8,
            },
        ],
        Some(sample_report("BTCUSDT")),
    );

    let scene = market_scene_from_snapshot(&snapshot);

    assert_eq!(scene.panes.len(), 2);
    assert!(scene.panes[0]
        .series
        .iter()
        .any(|series| matches!(series, Series::Line(_))));
    assert!(scene.panes[0]
        .series
        .iter()
        .any(|series| matches!(series, Series::Candles(_))));
    assert!(scene.panes[0]
        .series
        .iter()
        .any(|series| matches!(series, Series::Markers(_))));
    assert!(scene.panes[1]
        .series
        .iter()
        .any(|series| matches!(series, Series::Bars(_))));
}

#[test]
fn market_scene_can_aggregate_to_minute_timeframe() {
    let snapshot = sample_snapshot(
        "BTCUSDT",
        Vec::new(),
        vec![
            DerivedKlineRow {
                open_time_ms: 1_000,
                close_time_ms: 1_999,
                open: 100.0,
                high: 103.0,
                low: 99.0,
                close: 102.0,
                volume: 25.0,
                quote_volume: 2_550.0,
                trade_count: 10,
            },
            DerivedKlineRow {
                open_time_ms: 15_000,
                close_time_ms: 15_999,
                open: 102.0,
                high: 104.0,
                low: 101.0,
                close: 101.5,
                volume: 20.0,
                quote_volume: 2_035.0,
                trade_count: 8,
            },
        ],
        None,
    );

    let scene = market_scene_from_snapshot_with_timeframe(&snapshot, MarketTimeframe::Minute1m);

    let candle_count = scene.panes[0]
        .series
        .iter()
        .find_map(|series| match series {
            Series::Candles(candles) => Some(candles.candles.len()),
            _ => None,
        })
        .expect("candles present");

    assert_eq!(candle_count, 1);
}

#[test]
fn market_scene_can_aggregate_to_five_minute_timeframe() {
    let snapshot = sample_snapshot(
        "BTCUSDT",
        Vec::new(),
        vec![
            DerivedKlineRow {
                open_time_ms: 60_000,
                close_time_ms: 60_999,
                open: 100.0,
                high: 103.0,
                low: 99.0,
                close: 102.0,
                volume: 25.0,
                quote_volume: 2_550.0,
                trade_count: 10,
            },
            DerivedKlineRow {
                open_time_ms: 180_000,
                close_time_ms: 180_999,
                open: 102.0,
                high: 104.0,
                low: 101.0,
                close: 101.5,
                volume: 20.0,
                quote_volume: 2_035.0,
                trade_count: 8,
            },
        ],
        None,
    );

    let scene = market_scene_from_snapshot_with_timeframe(&snapshot, MarketTimeframe::Minute5m);

    let candle_count = scene.panes[0]
        .series
        .iter()
        .find_map(|series| match series {
            Series::Candles(candles) => Some(candles.candles.len()),
            _ => None,
        })
        .expect("candles present");

    assert_eq!(candle_count, 1);
}

#[test]
fn market_scene_can_aggregate_to_week_timeframe() {
    let snapshot = sample_snapshot(
        "BTCUSDT",
        Vec::new(),
        vec![
            DerivedKlineRow {
                open_time_ms: 1_710_000_000_000,
                close_time_ms: 1_710_000_059_999,
                open: 100.0,
                high: 103.0,
                low: 99.0,
                close: 102.0,
                volume: 25.0,
                quote_volume: 2_550.0,
                trade_count: 10,
            },
            DerivedKlineRow {
                open_time_ms: 1_710_086_400_000,
                close_time_ms: 1_710_086_459_999,
                open: 102.0,
                high: 104.0,
                low: 101.0,
                close: 101.5,
                volume: 20.0,
                quote_volume: 2_035.0,
                trade_count: 8,
            },
        ],
        None,
    );

    let scene = market_scene_from_snapshot_with_timeframe(&snapshot, MarketTimeframe::Week1w);

    let candle_count = scene.panes[0]
        .series
        .iter()
        .find_map(|series| match series {
            Series::Candles(candles) => Some(candles.candles.len()),
            _ => None,
        })
        .expect("candles present");

    assert_eq!(candle_count, 1);
}

#[test]
fn market_scene_splits_mid_price_line_across_large_gaps() {
    let snapshot = sample_snapshot(
        "BTCUSDT",
        vec![
            BookTickerRow {
                event_time_ms: 1_000,
                bid: 99.0,
                ask: 101.0,
            },
            BookTickerRow {
                event_time_ms: 2_000,
                bid: 100.0,
                ask: 102.0,
            },
            BookTickerRow {
                event_time_ms: 120_000,
                bid: 103.0,
                ask: 105.0,
            },
            BookTickerRow {
                event_time_ms: 121_000,
                bid: 104.0,
                ask: 106.0,
            },
        ],
        Vec::new(),
        None,
    );

    let scene = market_scene_from_snapshot(&snapshot);
    let line_count = scene.panes[0]
        .series
        .iter()
        .filter(|series| matches!(series, Series::Line(_)))
        .count();

    assert!(line_count >= 2);
}

#[test]
fn market_scene_ignores_selected_report_for_different_symbol() {
    let snapshot = sample_snapshot(
        "BTCUSDT",
        vec![BookTickerRow {
            event_time_ms: 1_000,
            bid: 99.0,
            ask: 101.0,
        }],
        Vec::new(),
        Some(sample_report("ETHUSDT")),
    );

    let scene = market_scene_from_snapshot(&snapshot);

    let marker_count = scene.panes[0]
        .series
        .iter()
        .find_map(|series| match series {
            Series::Markers(markers) => Some(markers.markers.len()),
            _ => None,
        })
        .expect("liquidation markers exist");
    assert_eq!(marker_count, 1);
}

#[test]
fn market_scene_focuses_viewport_when_selected_report_matches_symbol() {
    let snapshot = sample_snapshot(
        "BTCUSDT",
        Vec::new(),
        vec![
            DerivedKlineRow {
                open_time_ms: 1_000,
                close_time_ms: 1_999,
                open: 100.0,
                high: 103.0,
                low: 99.0,
                close: 102.0,
                volume: 25.0,
                quote_volume: 2_550.0,
                trade_count: 10,
            },
            DerivedKlineRow {
                open_time_ms: 30_000,
                close_time_ms: 30_999,
                open: 102.0,
                high: 104.0,
                low: 101.0,
                close: 101.5,
                volume: 20.0,
                quote_volume: 2_035.0,
                trade_count: 8,
            },
        ],
        Some(sample_report("BTCUSDT")),
    );

    let scene = market_scene_from_snapshot(&snapshot);

    assert!(scene.viewport.x_range.is_some());
}

#[test]
fn market_scene_caps_default_viewport_for_long_selected_run() {
    let mut report = sample_report("BTCUSDT");
    report.trades[0].exit_time = Some(Utc.timestamp_millis_opt(4_000_000).single().expect("ts"));

    let snapshot = sample_snapshot(
        "BTCUSDT",
        Vec::new(),
        vec![DerivedKlineRow {
            open_time_ms: 1_000,
            close_time_ms: 1_999,
            open: 100.0,
            high: 103.0,
            low: 99.0,
            close: 102.0,
            volume: 25.0,
            quote_volume: 2_550.0,
            trade_count: 10,
        }],
        Some(report),
    );

    let scene = market_scene_from_snapshot(&snapshot);
    let (min_x, max_x) = scene.viewport.x_range.expect("viewport");

    assert!((max_x.as_i64() - min_x.as_i64()) <= 27 * 60 * 1_000);
}

#[test]
fn equity_scene_prepends_starting_equity_point() {
    let report = sample_report("BTCUSDT");

    let scene = equity_scene_from_report(&report);

    let Series::Line(line) = &scene.panes[0].series[0] else {
        panic!("expected line series");
    };
    assert_eq!(line.points.len(), 2);
    assert_eq!(line.points[0].value, report.starting_equity);
    assert_eq!(
        line.points[0].time_ms.as_i64(),
        line.points[1].time_ms.as_i64()
    );
    assert_eq!(line.points[1].value, report.ending_equity);
}

#[test]
fn render_real_btcusdt_market_scene_does_not_panic() {
    use sandbox_quant::charting::plotters::PlottersRenderer;
    use sandbox_quant::charting::render::ChartRenderer;
    use sandbox_quant::charting::scene::RenderRequest;
    use sandbox_quant::visualization::service::VisualizationService;
    use sandbox_quant::visualization::types::DashboardQuery;

    let service = VisualizationService;
    let query = DashboardQuery {
        mode: BinanceMode::Demo,
        base_dir: PathBuf::from("var"),
        symbol: "BTCUSDT".to_string(),
        from: NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
        to: NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
        selected_run_id: None,
        run_limit: 24,
    };
    let snapshot = match service.load_dashboard(query) {
        Ok(snapshot) => snapshot,
        Err(sandbox_quant::error::storage_error::StorageError::DatabaseInitFailed {
            message,
            ..
        }) if message.contains("Conflicting lock") => {
            eprintln!(
                "skipping render_real_btcusdt_market_scene_does_not_panic: duckdb lock conflict"
            );
            return;
        }
        Err(error) => panic!("load dashboard: {error:?}"),
    };
    if snapshot.market_series.klines.is_empty() && snapshot.market_series.book_tickers.is_empty() {
        eprintln!(
            "skipping render_real_btcusdt_market_scene_does_not_panic: no local BTCUSDT data"
        );
        return;
    }
    let scene = market_scene_from_snapshot(&snapshot);
    let renderer = PlottersRenderer;
    let frame = renderer
        .render(
            &scene,
            &RenderRequest {
                width_px: 1280,
                height_px: 720,
                pixel_ratio: 1.0,
                oversample: 1,
            },
        )
        .expect("render frame");
    assert_eq!(frame.width_px, 1280);
    assert_eq!(frame.height_px, 720);
    assert!(!frame.rgb.is_empty());
}
