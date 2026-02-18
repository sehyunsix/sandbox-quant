use std::collections::HashMap;

use crate::model::candle::Candle;
use crate::model::signal::Signal;
use crate::model::tick::Tick;
use crate::order_manager::{OrderHistorySnapshot, OrderHistoryStats, OrderUpdate};
use crate::risk_module::RateBudgetSnapshot;

#[derive(Debug, Clone)]
pub enum WsConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting { attempt: u32, delay_ms: u64 },
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    MarketTick(Tick),
    StrategySignal {
        signal: Signal,
        source_tag: String,
        price: Option<f64>,
        timestamp_ms: u64,
    },
    StrategyState {
        fast_sma: Option<f64>,
        slow_sma: Option<f64>,
    },
    OrderUpdate(OrderUpdate),
    WsStatus(WsConnectionStatus),
    HistoricalCandles {
        candles: Vec<Candle>,
        interval_ms: u64,
        interval: String,
    },
    BalanceUpdate(HashMap<String, f64>),
    OrderHistoryUpdate(OrderHistorySnapshot),
    StrategyStatsUpdate {
        strategy_stats: HashMap<String, OrderHistoryStats>,
    },
    RiskRateSnapshot {
        global: RateBudgetSnapshot,
        orders: RateBudgetSnapshot,
        account: RateBudgetSnapshot,
        market_data: RateBudgetSnapshot,
    },
    TickDropped,
    LogMessage(String),
    Error(String),
}
