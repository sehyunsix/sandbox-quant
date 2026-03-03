use std::collections::HashMap;

use sandbox_quant::event::AssetPnlEntry;
use sandbox_quant::model::order::OrderSide;
use sandbox_quant::runtime::portfolio_layer_state::build_portfolio_layer_state;

#[test]
fn portfolio_layer_state_builds_from_live_futures_snapshot() {
    let mut live = HashMap::new();
    live.insert(
        "BTCUSDT (FUT)".to_string(),
        AssetPnlEntry {
            is_futures: true,
            side: Some(OrderSide::Buy),
            position_qty: 0.25,
            entry_price: 100_000.0,
            realized_pnl_usdt: 10.0,
            unrealized_pnl_usdt: -2.0,
        },
    );

    let state = build_portfolio_layer_state(&HashMap::new(), &HashMap::new(), &live);

    assert_eq!(state.by_symbol.len(), 1);
    assert_eq!(state.total_realized_pnl_usdt, 10.0);
    assert_eq!(state.total_unrealized_pnl_usdt, -2.0);
    assert_eq!(state.open_orders_count, 0);
    assert_eq!(state.reserved_cash_usdt, 0.0);
    assert!((state.gross_exposure_usdt - 25_000.0).abs() < 1e-9);
    assert!((state.net_exposure_usdt - 25_000.0).abs() < 1e-9);
}

#[test]
fn portfolio_layer_state_net_exposure_is_negative_for_short() {
    let mut live = HashMap::new();
    live.insert(
        "ETHUSDT (FUT)".to_string(),
        AssetPnlEntry {
            is_futures: true,
            side: Some(OrderSide::Sell),
            position_qty: 2.0,
            entry_price: 3_000.0,
            realized_pnl_usdt: 0.0,
            unrealized_pnl_usdt: 1.0,
        },
    );

    let state = build_portfolio_layer_state(&HashMap::new(), &HashMap::new(), &live);
    assert!((state.gross_exposure_usdt - 6_000.0).abs() < 1e-9);
    assert!((state.net_exposure_usdt + 6_000.0).abs() < 1e-9);
}
