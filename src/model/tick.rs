#[derive(Debug, Clone)]
pub struct Tick {
    pub price: f64,
    pub qty: f64,
    pub timestamp_ms: u64,
    pub is_buyer_maker: bool,
    pub trade_id: u64,
}
