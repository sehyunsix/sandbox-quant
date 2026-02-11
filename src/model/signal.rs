#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Buy { qty: f64 },
    Sell { qty: f64 },
    Hold,
}
