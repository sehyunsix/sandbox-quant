use crate::domain::market::Market;

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
pub struct RawCloseOrderRequest {
    pub symbol: String,
    pub market: Market,
    pub side: &'static str,
    pub qty: f64,
    pub reduce_only: bool,
}
