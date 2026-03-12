use crate::app::bootstrap::BinanceMode;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecorderMetrics {
    pub liquidation_events: u64,
    pub book_ticker_events: u64,
    pub agg_trade_events: u64,
    pub derived_kline_1s_bars: u64,
    pub last_liquidation_event_time: Option<String>,
    pub last_book_ticker_event_time: Option<String>,
    pub last_agg_trade_event_time: Option<String>,
    pub top_liquidation_symbols: Vec<String>,
    pub top_book_ticker_symbols: Vec<String>,
    pub top_agg_trade_symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestDatasetSummary {
    pub mode: BinanceMode,
    pub symbol: String,
    pub from: String,
    pub to: String,
    pub liquidation_events: u64,
    pub book_ticker_events: u64,
    pub agg_trade_events: u64,
    pub derived_kline_1s_bars: u64,
}
