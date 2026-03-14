use crate::app::bootstrap::BinanceMode;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecorderMetrics {
    pub liquidation_events: u64,
    pub book_ticker_events: u64,
    pub agg_trade_events: u64,
    pub derived_kline_1s_bars: u64,
    pub schema_version: Option<String>,
    pub last_liquidation_event_time: Option<String>,
    pub last_book_ticker_event_time: Option<String>,
    pub last_agg_trade_event_time: Option<String>,
    pub top_liquidation_symbols: Vec<String>,
    pub top_book_ticker_symbols: Vec<String>,
    pub top_agg_trade_symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiquidationEventRow {
    pub event_time_ms: i64,
    pub force_side: String,
    pub price: f64,
    pub qty: f64,
    pub notional: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BookTickerRow {
    pub event_time_ms: i64,
    pub bid: f64,
    pub ask: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DerivedKlineRow {
    pub open_time_ms: i64,
    pub close_time_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub quote_volume: f64,
    pub trade_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestDatasetSummary {
    pub mode: BinanceMode,
    pub symbol: String,
    pub symbol_found: bool,
    pub from: String,
    pub to: String,
    pub liquidation_events: u64,
    pub book_ticker_events: u64,
    pub agg_trade_events: u64,
    pub derived_kline_1s_bars: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BacktestRunSummaryRow {
    pub run_id: i64,
    pub created_at: String,
    pub mode: BinanceMode,
    pub template: String,
    pub instrument: String,
    pub from: String,
    pub to: String,
    pub trigger_count: u64,
    pub closed_trades: u64,
    pub open_trades: u64,
    pub wins: u64,
    pub losses: u64,
    pub net_pnl: f64,
    pub ending_equity: f64,
}
