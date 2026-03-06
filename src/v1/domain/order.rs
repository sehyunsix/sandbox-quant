use crate::v1::domain::identifiers::OrderId;
use crate::v1::domain::instrument::Instrument;
use crate::v1::domain::market::Market;
use crate::v1::domain::position::Side;

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
