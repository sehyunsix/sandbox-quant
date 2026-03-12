use crate::domain::market::Market;
use crate::domain::order_type::OrderType;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawSymbolRules {
    pub min_qty: f64,
    pub max_qty: f64,
    pub step_size: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawCloseOrderAck {
    pub remote_order_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawOpenOrder {
    pub order_id: Option<String>,
    pub client_order_id: String,
    pub symbol: String,
    pub market: Market,
    pub side: &'static str,
    pub orig_qty: f64,
    pub executed_qty: f64,
    pub reduce_only: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawCloseOrderRequest {
    pub symbol: String,
    pub market: Market,
    pub side: &'static str,
    pub qty: String,
    pub order_type: OrderType,
    pub reduce_only: bool,
}
