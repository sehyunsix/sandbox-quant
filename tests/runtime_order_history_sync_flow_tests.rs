use sandbox_quant::model::order::OrderSide;
use sandbox_quant::model::position::Position;
use sandbox_quant::order_manager::MarketKind;
use sandbox_quant::runtime::order_history_sync_flow::{
    derived_stop_price_for_position, market_kind_from_instrument_label,
};

#[test]
fn detects_market_kind_from_label_suffix() {
    assert_eq!(
        market_kind_from_instrument_label("BTCUSDT"),
        MarketKind::Spot
    );
    assert_eq!(
        market_kind_from_instrument_label("BTCUSDT (FUT)"),
        MarketKind::Futures
    );
}

#[test]
fn computes_stop_price_for_long_and_short() {
    let mut long = Position::new("BTCUSDT".to_string());
    long.side = Some(OrderSide::Buy);
    long.qty = 1.0;
    long.entry_price = 100.0;
    let mut short = Position::new("BTCUSDT".to_string());
    short.side = Some(OrderSide::Sell);
    short.qty = 1.0;
    short.entry_price = 100.0;

    let long_stop = derived_stop_price_for_position(&long, 0.02).expect("long stop");
    let short_stop = derived_stop_price_for_position(&short, 0.02).expect("short stop");

    assert!((long_stop - 98.0).abs() < 1e-12);
    assert!((short_stop - 102.0).abs() < 1e-12);
}
