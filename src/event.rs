use std::collections::HashMap;

use crate::alpaca::rest::OptionChainSnapshot;
use crate::model::candle::Candle;
use crate::model::signal::Signal;
use crate::model::tick::Tick;
use crate::order_manager::{OrderHistorySnapshot, OrderUpdate};
use crate::strategy_stats::StrategyStats;

#[derive(Debug, Clone)]
pub enum WsConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting { attempt: u32, delay_ms: u64 },
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    MarketTick(Tick),
    DataHeartbeat,
    StrategySignal(Signal),
    StrategyState {
        fast_sma: Option<f64>,
        slow_sma: Option<f64>,
    },
    StrategyStatsUpdate(HashMap<String, StrategyStats>),
    OrderUpdate(OrderUpdate),
    WsStatus(WsConnectionStatus),
    HistoricalCandles {
        candles: Vec<Candle>,
        interval_ms: u64,
        interval: String,
    },
    OptionChainUpdate(Option<OptionChainSnapshot>),
    BalanceUpdate(HashMap<String, f64>),
    OrderHistoryUpdate(OrderHistorySnapshot),
    LogMessage(String),
    Error(String),
}
