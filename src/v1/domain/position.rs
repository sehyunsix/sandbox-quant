use crate::v1::domain::instrument::Instrument;
use crate::v1::domain::market::Market;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionSnapshot {
    pub instrument: Instrument,
    pub market: Market,
    /// Canonical signed quantity.
    ///
    /// Examples:
    /// - `+0.25` means long `0.25`
    /// - `-0.25` means short `0.25`
    /// - `0.0` means flat
    pub signed_qty: f64,
    pub entry_price: Option<f64>,
}

impl PositionSnapshot {
    pub fn side(&self) -> Option<Side> {
        if self.signed_qty > 0.0 {
            Some(Side::Buy)
        } else if self.signed_qty < 0.0 {
            Some(Side::Sell)
        } else {
            None
        }
    }

    pub fn abs_qty(&self) -> f64 {
        self.signed_qty.abs()
    }

    pub fn is_flat(&self) -> bool {
        self.signed_qty.abs() <= f64::EPSILON
    }
}
