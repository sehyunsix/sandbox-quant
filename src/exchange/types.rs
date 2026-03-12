use crate::domain::balance::BalanceSnapshot;
use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::domain::order::OpenOrder;
use crate::domain::order_type::OrderType;
use crate::domain::position::{PositionSnapshot, Side};
use crate::execution::planner::ExecutionPlan;

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
    pub qty_text: String,
    pub order_type: OrderType,
    pub reduce_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseOrderAccepted {
    pub remote_order_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubmitOrderAccepted {
    pub remote_order_id: String,
}

impl From<ExecutionPlan> for CloseOrderRequest {
    fn from(plan: ExecutionPlan) -> Self {
        Self {
            instrument: plan.instrument,
            market: Market::Spot,
            side: plan.side,
            qty: plan.qty,
            qty_text: plan.qty.to_string(),
            order_type: OrderType::Market,
            reduce_only: plan.reduce_only,
        }
    }
}
