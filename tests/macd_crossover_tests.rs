use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::macd_crossover::MacdCrossoverStrategy;

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
fn macd_emits_actionable_signal() {
    let mut strategy = MacdCrossoverStrategy::new(2, 4, 0);
    for p in [
        100.0, 99.0, 98.0, 99.0, 100.0, 101.0, 100.0, 99.0, 98.0, 99.0, 100.0, 101.0,
    ] {
        if strategy.on_tick(&tick(p)) != Signal::Hold {
            return;
        }
    }
    panic!("expected at least one actionable signal");
}

#[test]
fn macd_respects_cooldown() {
    let mut strategy = MacdCrossoverStrategy::new(2, 4, 4);
    for p in [100.0, 99.0, 98.0, 99.0, 100.0, 101.0, 100.0, 99.0] {
        let _ = strategy.on_tick(&tick(p));
    }
    let sig = strategy.on_tick(&tick(95.0));
    assert_eq!(sig, Signal::Hold);
}
