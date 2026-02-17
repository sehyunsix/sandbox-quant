use sandbox_quant::model::order::{Fill, OrderSide};
use sandbox_quant::model::position::Position;

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

    pos.apply_fill(OrderSide::Buy, &[make_fill(42000.0, 0.001)]);
    assert_eq!(pos.side, Some(OrderSide::Buy));
    assert!((pos.qty - 0.001).abs() < f64::EPSILON);
    assert!((pos.entry_price - 42000.0).abs() < f64::EPSILON);

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
