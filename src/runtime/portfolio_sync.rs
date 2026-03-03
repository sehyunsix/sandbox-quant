use crate::binance::types::{BinanceFuturesAccountUpdatePosition, BinanceFuturesPositionRisk};
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

pub fn build_live_futures_position_deltas_from_account_update(
    rows: &[BinanceFuturesAccountUpdatePosition],
    normalize_futures_symbol: fn(&str) -> String,
) -> Vec<(String, Option<AssetPnlEntry>)> {
    let mut out = Vec::with_capacity(rows.len());
    for p in rows {
        let instrument = normalize_futures_symbol(&p.symbol);
        if p.position_amt.abs() <= f64::EPSILON {
            out.push((instrument, None));
            continue;
        }
        let side = if p.position_amt > 0.0 {
            Some(OrderSide::Buy)
        } else {
            Some(OrderSide::Sell)
        };
        out.push((
            instrument,
            Some(AssetPnlEntry {
                is_futures: true,
                side,
                position_qty: p.position_amt.abs(),
                entry_price: p.entry_price,
                realized_pnl_usdt: 0.0,
                unrealized_pnl_usdt: p.unrealized_pnl,
            }),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_update_positions_emit_upsert_and_remove_deltas() {
        let rows = vec![
            BinanceFuturesAccountUpdatePosition {
                symbol: "BTCUSDT".to_string(),
                position_amt: 0.25,
                entry_price: 65000.0,
                unrealized_pnl: 12.3,
            },
            BinanceFuturesAccountUpdatePosition {
                symbol: "ETHUSDT".to_string(),
                position_amt: 0.0,
                entry_price: 0.0,
                unrealized_pnl: 0.0,
            },
        ];
        let deltas = build_live_futures_position_deltas_from_account_update(&rows, |symbol| {
            format!("{} (FUT)", symbol)
        });
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0].0, "BTCUSDT (FUT)");
        assert!(deltas[0].1.is_some());
        assert_eq!(deltas[1].0, "ETHUSDT (FUT)");
        assert!(deltas[1].1.is_none());
    }
}
