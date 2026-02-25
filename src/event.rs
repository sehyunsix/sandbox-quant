use std::collections::HashMap;

use crate::model::candle::Candle;
use crate::model::order::OrderSide;
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
        symbol: String,
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
    EvSnapshotUpdate {
        symbol: String,
        source_tag: String,
        ev: f64,
        entry_ev: Option<f64>,
        p_win: f64,
        gate_mode: String,
        gate_blocked: bool,
    },
    ExitPolicyUpdate {
        symbol: String,
        source_tag: String,
        stop_price: Option<f64>,
        expected_holding_ms: Option<u64>,
        protective_stop_ok: Option<bool>,
    },
    AssetPnlUpdate {
        by_symbol: HashMap<String, AssetPnlEntry>,
    },
    RiskRateSnapshot {
        global: RateBudgetSnapshot,
        orders: RateBudgetSnapshot,
        account: RateBudgetSnapshot,
        market_data: RateBudgetSnapshot,
    },
    CloseAllRequested {
        job_id: u64,
        total: usize,
        symbols: Vec<String>,
    },
    CloseAllProgress {
        job_id: u64,
        symbol: String,
        completed: usize,
        total: usize,
        failed: usize,
        reason: Option<String>,
    },
    CloseAllFinished {
        job_id: u64,
        completed: usize,
        total: usize,
        failed: usize,
    },
    TickDropped,
    LogRecord(LogRecord),
    LogMessage(String),
    Error(String),
}

#[derive(Debug, Clone, Default)]
pub struct EvSnapshotEntry {
    pub ev: f64,
    pub entry_ev: Option<f64>,
    pub p_win: f64,
    pub gate_mode: String,
    pub gate_blocked: bool,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ExitPolicyEntry {
    pub stop_price: Option<f64>,
    pub expected_holding_ms: Option<u64>,
    pub protective_stop_ok: Option<bool>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AssetPnlEntry {
    pub is_futures: bool,
    pub side: Option<OrderSide>,
    pub position_qty: f64,
    pub entry_price: f64,
    pub realized_pnl_usdt: f64,
    pub unrealized_pnl_usdt: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogDomain {
    Ws,
    Strategy,
    Risk,
    Order,
    Portfolio,
    Ui,
    System,
}

#[derive(Debug, Clone)]
pub struct LogRecord {
    pub ts_ms: u64,
    pub level: LogLevel,
    pub domain: LogDomain,
    pub event: &'static str,
    pub symbol: Option<String>,
    pub strategy_tag: Option<String>,
    pub trace_id: Option<String>,
    pub msg: String,
}

impl LogRecord {
    pub fn new(level: LogLevel, domain: LogDomain, event: &'static str, msg: impl Into<String>) -> Self {
        Self {
            ts_ms: chrono::Utc::now().timestamp_millis() as u64,
            level,
            domain,
            event,
            symbol: None,
            strategy_tag: None,
            trace_id: None,
            msg: msg.into(),
        }
    }
}
