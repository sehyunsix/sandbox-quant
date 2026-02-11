use crate::model::signal::Signal;
use crate::model::tick::Tick;
use crate::order_manager::OrderUpdate;

#[derive(Debug, Clone)]
pub enum WsConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting { attempt: u32 },
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    MarketTick(Tick),
    StrategySignal(Signal),
    OrderUpdate(OrderUpdate),
    WsStatus(WsConnectionStatus),
    Error(String),
}
