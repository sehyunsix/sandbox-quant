use sandbox_quant::binance::types::BinanceMyTrade;
use sandbox_quant::order_store::PersistedTrade;
use sandbox_quant::ui::position_ledger::build_open_order_positions_from_trades;

fn trade(
    symbol: &str,
    trade_id: u64,
    order_id: u64,
    is_buyer: bool,
    qty: f64,
    price: f64,
    source: &str,
) -> PersistedTrade {
    PersistedTrade {
        trade: BinanceMyTrade {
            symbol: symbol.to_string(),
            id: trade_id,
            order_id,
            price,
            qty,
            commission: 0.0,
            commission_asset: "USDT".to_string(),
            time: trade_id,
            is_buyer,
            is_maker: false,
            realized_pnl: 0.0,
        },
        source: source.to_string(),
    }
}

#[test]
fn open_positions_are_grouped_by_entry_order() {
    let rows = vec![
        trade("BTCUSDT", 1, 1001, true, 0.010, 60000.0, "cfg"),
        trade("BTCUSDT", 2, 1001, true, 0.005, 60100.0, "cfg"),
        trade("BTCUSDT", 3, 1002, true, 0.020, 60200.0, "c01"),
    ];

    let out = build_open_order_positions_from_trades(&rows);
    assert_eq!(out.len(), 2);
    let by_order: std::collections::HashMap<u64, f64> =
        out.iter().map(|r| (r.order_id, r.qty_open)).collect();
    assert!((by_order.get(&1001).copied().unwrap_or_default() - 0.015).abs() < 1e-9);
    assert!((by_order.get(&1002).copied().unwrap_or_default() - 0.020).abs() < 1e-9);
}

#[test]
fn sell_trades_reduce_existing_orders_fifo() {
    let rows = vec![
        trade("BTCUSDT", 1, 1001, true, 0.010, 60000.0, "cfg"),
        trade("BTCUSDT", 2, 1002, true, 0.020, 61000.0, "c01"),
        trade("BTCUSDT", 3, 2001, false, 0.015, 62000.0, "sys"),
    ];

    let out = build_open_order_positions_from_trades(&rows);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].order_id, 1002);
    assert!((out[0].qty_open - 0.015).abs() < 1e-9);
}
