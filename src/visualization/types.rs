use std::path::PathBuf;

use crate::app::bootstrap::BinanceMode;
use crate::backtest_app::runner::{BacktestConfig, BacktestReport};
use crate::dataset::types::{
    BacktestDatasetSummary, BacktestRunSummaryRow, BookTickerRow, DerivedKlineRow,
    LiquidationEventRow, RecorderMetrics,
};
use crate::strategy::model::StrategyTemplate;

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardQuery {
    pub mode: BinanceMode,
    pub base_dir: PathBuf,
    pub symbol: String,
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub selected_run_id: Option<i64>,
    pub run_limit: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BacktestRunRequest {
    pub mode: BinanceMode,
    pub base_dir: PathBuf,
    pub symbol: String,
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub template: StrategyTemplate,
    pub config: BacktestConfig,
    pub run_limit: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketSeries {
    pub symbol: String,
    pub liquidations: Vec<LiquidationEventRow>,
    pub book_tickers: Vec<BookTickerRow>,
    pub klines: Vec<DerivedKlineRow>,
    pub kline_interval: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PricePoint {
    pub time_ms: i64,
    pub price: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquityPoint {
    pub time_ms: i64,
    pub equity: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalKind {
    Entry,
    TakeProfit,
    StopLoss,
    OpenAtEnd,
    SignalExit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalMarker {
    pub time_ms: i64,
    pub price: f64,
    pub label: String,
    pub kind: SignalKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardSnapshot {
    pub mode: BinanceMode,
    pub base_dir: PathBuf,
    pub db_path: PathBuf,
    pub symbol: String,
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub available_symbols: Vec<String>,
    pub recorder_metrics: RecorderMetrics,
    pub dataset_summary: BacktestDatasetSummary,
    pub market_series: MarketSeries,
    pub recent_runs: Vec<BacktestRunSummaryRow>,
    pub selected_report: Option<BacktestReport>,
    pub selected_run_id: Option<i64>,
}
