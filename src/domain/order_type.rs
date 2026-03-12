#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderType {
    Market,
    Limit { price: f64 },
}
