use super::order::{Fill, OrderSide};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FillApplySummary {
    pub realized_pnl_delta: f64,
    pub winning_closes: u32,
    pub losing_closes: u32,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub side: Option<OrderSide>,
    pub qty: f64,
    pub entry_price: f64,
    pub open_fee_quote: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub trade_count: u32,
    pub winning_trade_count: u32,
    pub losing_trade_count: u32,
}

impl Position {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            side: None,
            qty: 0.0,
            entry_price: 0.0,
            open_fee_quote: 0.0,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            trade_count: 0,
            winning_trade_count: 0,
            losing_trade_count: 0,
        }
    }

    fn split_symbol_assets(&self) -> Option<(&str, &str)> {
        for quote in ["USDT", "USDC", "BUSD", "FDUSD", "BTC", "ETH", "BNB"] {
            if let Some(base) = self.symbol.strip_suffix(quote) {
                if !base.is_empty() {
                    return Some((base, quote));
                }
            }
        }
        None
    }

    fn commission_quote(&self, fill: &Fill) -> f64 {
        if fill.commission <= 0.0 {
            return 0.0;
        }
        let Some((base_asset, quote_asset)) = self.split_symbol_assets() else {
            return 0.0;
        };
        let fee_asset = fill.commission_asset.to_ascii_uppercase();
        if fee_asset == quote_asset {
            fill.commission
        } else if fee_asset == base_asset {
            fill.commission * fill.price
        } else {
            0.0
        }
    }

    pub fn is_flat(&self) -> bool {
        self.side.is_none() || self.qty <= 0.0
    }

    pub fn apply_fill(&mut self, side: OrderSide, fills: &[Fill]) -> FillApplySummary {
        let mut summary = FillApplySummary::default();
        for fill in fills {
            let fee_quote = self.commission_quote(fill);
            match (self.side, side) {
                // Opening a new position
                (None, _) => {
                    self.side = Some(side);
                    self.qty = fill.qty;
                    self.entry_price = fill.price;
                    self.open_fee_quote += fee_quote;
                }
                // Adding to existing position (same side)
                (Some(pos_side), fill_side) if pos_side == fill_side => {
                    let total_cost = self.entry_price * self.qty + fill.price * fill.qty;
                    self.qty += fill.qty;
                    self.entry_price = total_cost / self.qty;
                    self.open_fee_quote += fee_quote;
                }
                // Closing position (opposite side)
                (Some(_pos_side), _fill_side) => {
                    let prev_qty = self.qty.max(f64::EPSILON);
                    let close_qty = fill.qty.min(self.qty);
                    let gross_pnl = match self.side {
                        Some(OrderSide::Buy) => (fill.price - self.entry_price) * close_qty,
                        Some(OrderSide::Sell) => (self.entry_price - fill.price) * close_qty,
                        None => 0.0,
                    };
                    let open_fee_alloc = self.open_fee_quote * (close_qty / prev_qty);
                    self.open_fee_quote -= open_fee_alloc;
                    let net_pnl = gross_pnl - open_fee_alloc - fee_quote;

                    self.realized_pnl += net_pnl;
                    summary.realized_pnl_delta += net_pnl;
                    if net_pnl > 0.0 {
                        self.winning_trade_count += 1;
                        summary.winning_closes += 1;
                    } else if net_pnl < 0.0 {
                        self.losing_trade_count += 1;
                        summary.losing_closes += 1;
                    }
                    self.qty -= close_qty;
                    if self.qty <= f64::EPSILON {
                        self.side = None;
                        self.qty = 0.0;
                        self.entry_price = 0.0;
                        self.open_fee_quote = 0.0;
                    }
                }
            }
        }
        self.trade_count += 1;
        summary
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

    pub fn win_rate_percent(&self) -> f64 {
        let total = self.winning_trade_count + self.losing_trade_count;
        if total == 0 {
            return 0.0;
        }
        (self.winning_trade_count as f64 / total as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fill(price: f64, qty: f64, commission: f64, commission_asset: &str) -> Fill {
        Fill {
            price,
            qty,
            commission,
            commission_asset: commission_asset.to_string(),
        }
    }

    #[test]
    fn open_and_close_long() {
        let mut pos = Position::new("BTCUSDT".to_string());
        assert!(pos.is_flat());

        // Buy 0.001 BTC at 42000
        pos.apply_fill(OrderSide::Buy, &[make_fill(42000.0, 0.001, 0.0, "USDT")]);
        assert_eq!(pos.side, Some(OrderSide::Buy));
        assert!((pos.qty - 0.001).abs() < f64::EPSILON);
        assert!((pos.entry_price - 42000.0).abs() < f64::EPSILON);

        // Sell 0.001 BTC at 42100 -> PnL = 0.10
        pos.apply_fill(OrderSide::Sell, &[make_fill(42100.0, 0.001, 0.0, "USDT")]);
        assert!(pos.is_flat());
        assert!((pos.realized_pnl - 0.10).abs() < 0.001);
    }

    #[test]
    fn unrealized_pnl_updates() {
        let mut pos = Position::new("BTCUSDT".to_string());
        pos.apply_fill(OrderSide::Buy, &[make_fill(42000.0, 0.001, 0.0, "USDT")]);

        pos.update_unrealized_pnl(42500.0);
        assert!((pos.unrealized_pnl - 0.50).abs() < 0.001);

        pos.update_unrealized_pnl(41800.0);
        assert!((pos.unrealized_pnl - (-0.20)).abs() < 0.001);
    }

    #[test]
    fn win_rate_tracks_wins_and_losses() {
        let mut pos = Position::new("BTCUSDT".to_string());
        pos.apply_fill(OrderSide::Buy, &[make_fill(100.0, 1.0, 0.0, "USDT")]);
        pos.apply_fill(OrderSide::Sell, &[make_fill(110.0, 1.0, 0.0, "USDT")]); // win

        pos.apply_fill(OrderSide::Buy, &[make_fill(100.0, 1.0, 0.0, "USDT")]);
        pos.apply_fill(OrderSide::Sell, &[make_fill(90.0, 1.0, 0.0, "USDT")]); // loss

        assert_eq!(pos.winning_trade_count, 1);
        assert_eq!(pos.losing_trade_count, 1);
        assert!((pos.win_rate_percent() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn win_loss_uses_net_pnl_after_fees() {
        let mut pos = Position::new("BTCUSDT".to_string());
        // Gross +0.10, but fees 0.15 + 0.15 => net -0.20.
        pos.apply_fill(OrderSide::Buy, &[make_fill(100.0, 1.0, 0.15, "USDT")]);
        pos.apply_fill(OrderSide::Sell, &[make_fill(100.1, 1.0, 0.15, "USDT")]);

        assert_eq!(pos.winning_trade_count, 0);
        assert_eq!(pos.losing_trade_count, 1);
        assert!((pos.realized_pnl - (-0.20)).abs() < 1e-9);
    }

    #[test]
    fn base_asset_fee_is_converted_to_quote() {
        let mut pos = Position::new("BTCUSDT".to_string());
        // Buy fee in base asset (BTC), sell fee in quote (USDT).
        pos.apply_fill(OrderSide::Buy, &[make_fill(10_000.0, 1.0, 0.00001, "BTC")]); // 0.1 USDT
        pos.apply_fill(OrderSide::Sell, &[make_fill(10_000.0, 1.0, 0.1, "USDT")]); // 0.1 USDT

        assert!((pos.realized_pnl - (-0.2)).abs() < 1e-9);
        assert_eq!(pos.losing_trade_count, 1);
    }
}
