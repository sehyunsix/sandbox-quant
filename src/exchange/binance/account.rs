use crate::exchange::binance::orders::RawOpenOrder;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RawBalance {
    pub asset: String,
    pub free: f64,
    pub locked: f64,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RawPosition {
    pub symbol: String,
    pub signed_qty: f64,
    pub entry_price: Option<f64>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RawAccountState {
    pub balances: Vec<RawBalance>,
    pub positions: Vec<RawPosition>,
    pub open_orders: Vec<RawOpenOrder>,
}
