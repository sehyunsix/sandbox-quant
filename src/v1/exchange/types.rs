use crate::v1::domain::balance::BalanceSnapshot;
use crate::v1::domain::instrument::Instrument;
use crate::v1::domain::market::Market;
use crate::v1::domain::order::OpenOrder;
use crate::v1::domain::position::{PositionSnapshot, Side};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuthoritativeSnapshot {
    pub balances: Vec<BalanceSnapshot>,
    pub positions: Vec<PositionSnapshot>,
    pub open_orders: Vec<OpenOrder>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloseOrderRequest {
    pub instrument: Instrument,
    pub market: Market,
    pub side: Side,
    pub qty: f64,
    pub reduce_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseOrderAccepted {
    pub remote_order_id: String,
}
