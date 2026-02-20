use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::roc_momentum::RocMomentumStrategy;

fn tick(price: f64) -> Tick {
    Tick {
        symbol: "BTCUSDT".to_string(),
        price,
        qty: 1.0,
        timestamp_ms: 0,
        is_buyer_maker: false,
        trade_id: 0,
    }
}

#[test]
fn roc_buy_then_sell() {
    let mut strategy = RocMomentumStrategy::new(3, 10, 0);
    let mut saw_buy = false;
    let mut saw_sell = false;
    for p in [
        100.0, 100.2, 100.4, 100.6, 100.8, 101.0, 100.6, 100.2, 99.8, 99.4, 99.0,
    ] {
        match strategy.on_tick(&tick(p)) {
            Signal::Buy => saw_buy = true,
            Signal::Sell => saw_sell = true,
            Signal::Hold => {}
        }
    }
    assert!(saw_buy, "expected at least one buy");
    assert!(saw_sell, "expected at least one sell");
}

#[test]
fn roc_respects_cooldown() {
    let mut strategy = RocMomentumStrategy::new(3, 10, 3);
    for p in [100.0, 100.2, 100.4, 100.6] {
        let _ = strategy.on_tick(&tick(p));
    }
    let _ = strategy.on_tick(&tick(100.8));
    let early = strategy.on_tick(&tick(98.8));
    assert_eq!(early, Signal::Hold);
}
