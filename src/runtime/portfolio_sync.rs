use crate::binance::types::BinanceFuturesPositionRisk;
use crate::event::AssetPnlEntry;
use crate::model::order::OrderSide;
use std::collections::HashMap;

pub fn build_live_futures_positions(
    rows: &[BinanceFuturesPositionRisk],
    normalize_futures_symbol: fn(&str) -> String,
) -> HashMap<String, AssetPnlEntry> {
    let mut out = HashMap::new();
    for p in rows {
        if p.position_amt.abs() <= f64::EPSILON {
            continue;
        }
        let instrument = normalize_futures_symbol(&p.symbol);
        let side = if p.position_amt > 0.0 {
            Some(OrderSide::Buy)
        } else {
            Some(OrderSide::Sell)
        };
        out.insert(
            instrument,
            AssetPnlEntry {
                is_futures: true,
                side,
                position_qty: p.position_amt.abs(),
                entry_price: p.entry_price,
                realized_pnl_usdt: 0.0,
                unrealized_pnl_usdt: if p.unrealized_profit.abs() > f64::EPSILON {
                    p.unrealized_profit
                } else if p.mark_price > f64::EPSILON && p.entry_price > f64::EPSILON {
                    (p.mark_price - p.entry_price) * p.position_amt
                } else {
                    0.0
                },
            },
        );
    }
    out
}
