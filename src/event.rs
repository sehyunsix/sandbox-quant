use crate::model::signal::Signal;
use crate::model::tick::Tick;
use crate::order_manager::OrderUpdate;

#[derive(Debug, Clone)]
pub enum WsConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting { attempt: u32, delay_ms: u64 },
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    MarketTick(Tick),
    StrategySignal(Signal),
    StrategyState {
        fast_sma: Option<f64>,
        slow_sma: Option<f64>,
    },
    OrderUpdate(OrderUpdate),
    WsStatus(WsConnectionStatus),
    LogMessage(String),
    Error(String),
}
