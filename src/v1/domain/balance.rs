#[derive(Debug, Clone, PartialEq)]
pub struct BalanceSnapshot {
    pub asset: String,
    pub free: f64,
    pub locked: f64,
}

impl BalanceSnapshot {
    pub fn total(&self) -> f64 {
        self.free + self.locked
    }
}
