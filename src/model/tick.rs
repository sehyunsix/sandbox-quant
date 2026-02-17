#[derive(Debug, Clone)]
pub struct Tick {
    pub symbol: String,
    pub price: f64,
    pub qty: f64,
    pub timestamp_ms: u64,
    pub is_buyer_maker: bool,
    pub trade_id: u64,
}

impl Tick {
    /// Create a synthetic tick from a kline close price (for SMA warm-up).
    pub fn from_price(price: f64) -> Self {
        Self {
            symbol: "SYNTH".to_string(),
            price,
            qty: 0.0,
            timestamp_ms: 0,
            is_buyer_maker: false,
            trade_id: 0,
        }
    }
}
