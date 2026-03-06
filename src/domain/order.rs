use crate::domain::identifiers::OrderId;
use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::domain::position::Side;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    PendingSubmit,
    Submitted,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenOrder {
    pub order_id: Option<OrderId>,
    pub client_order_id: String,
    pub instrument: Instrument,
    pub market: Market,
    pub side: Side,
    pub orig_qty: f64,
    pub executed_qty: f64,
    pub reduce_only: bool,
    pub status: OrderStatus,
}
