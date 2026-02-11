use super::order::{Fill, OrderSide};

#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub side: Option<OrderSide>,
    pub qty: f64,
    pub entry_price: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub trade_count: u32,
}

impl Position {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            side: None,
            qty: 0.0,
            entry_price: 0.0,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            trade_count: 0,
        }
    }

    pub fn is_flat(&self) -> bool {
        self.side.is_none() || self.qty <= 0.0
    }

    pub fn apply_fill(&mut self, side: OrderSide, fills: &[Fill]) {
        for fill in fills {
            match (self.side, side) {
                // Opening a new position
                (None, _) => {
                    self.side = Some(side);
                    self.qty = fill.qty;
                    self.entry_price = fill.price;
                }
                // Adding to existing position (same side)
                (Some(pos_side), fill_side) if pos_side == fill_side => {
                    let total_cost = self.entry_price * self.qty + fill.price * fill.qty;
                    self.qty += fill.qty;
                    self.entry_price = total_cost / self.qty;
                }
                // Closing position (opposite side)
                (Some(_pos_side), _fill_side) => {
                    let close_qty = fill.qty.min(self.qty);
                    let pnl = match self.side {
                        Some(OrderSide::Buy) => (fill.price - self.entry_price) * close_qty,
                        Some(OrderSide::Sell) => (self.entry_price - fill.price) * close_qty,
                        None => 0.0,
                    };
                    self.realized_pnl += pnl;
                    self.qty -= close_qty;
                    if self.qty <= f64::EPSILON {
                        self.side = None;
                        self.qty = 0.0;
                        self.entry_price = 0.0;
                    }
                }
            }
        }
        self.trade_count += 1;
    }

    pub fn update_unrealized_pnl(&mut self, current_price: f64) {
        if self.is_flat() {
            self.unrealized_pnl = 0.0;
            return;
        }
        self.unrealized_pnl = match self.side {
            Some(OrderSide::Buy) => (current_price - self.entry_price) * self.qty,
            Some(OrderSide::Sell) => (self.entry_price - current_price) * self.qty,
            None => 0.0,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fill(price: f64, qty: f64) -> Fill {
        Fill {
            price,
            qty,
            commission: 0.0,
            commission_asset: "BNB".to_string(),
        }
    }

    #[test]
    fn open_and_close_long() {
        let mut pos = Position::new("BTCUSDT".to_string());
        assert!(pos.is_flat());

        // Buy 0.001 BTC at 42000
        pos.apply_fill(OrderSide::Buy, &[make_fill(42000.0, 0.001)]);
        assert_eq!(pos.side, Some(OrderSide::Buy));
        assert!((pos.qty - 0.001).abs() < f64::EPSILON);
        assert!((pos.entry_price - 42000.0).abs() < f64::EPSILON);

        // Sell 0.001 BTC at 42100 -> PnL = 0.10
        pos.apply_fill(OrderSide::Sell, &[make_fill(42100.0, 0.001)]);
        assert!(pos.is_flat());
        assert!((pos.realized_pnl - 0.10).abs() < 0.001);
    }

    #[test]
    fn unrealized_pnl_updates() {
        let mut pos = Position::new("BTCUSDT".to_string());
        pos.apply_fill(OrderSide::Buy, &[make_fill(42000.0, 0.001)]);

        pos.update_unrealized_pnl(42500.0);
        assert!((pos.unrealized_pnl - 0.50).abs() < 0.001);

        pos.update_unrealized_pnl(41800.0);
        assert!((pos.unrealized_pnl - (-0.20)).abs() < 0.001);
    }
}
