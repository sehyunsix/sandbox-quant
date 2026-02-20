use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::aroon_trend::AroonTrendStrategy;

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
fn aroon_emits_actionable_signal() {
    let mut strategy = AroonTrendStrategy::new(8, 70, 0);

    for p in [100.0, 99.0, 98.0, 97.0, 96.0, 97.0, 98.0, 99.0, 101.0, 103.0] {
        if strategy.on_tick(&tick(p)) != Signal::Hold {
            return;
        }
    }
    panic!("expected at least one actionable signal");
}

#[test]
fn aroon_respects_cooldown() {
    let mut strategy = AroonTrendStrategy::new(8, 70, 3);
    for p in [100.0, 99.0, 98.0, 97.0, 96.0, 97.0, 98.0, 99.0, 101.0] {
        let _ = strategy.on_tick(&tick(p));
    }
    let early = strategy.on_tick(&tick(95.0));
    assert_eq!(early, Signal::Hold);
}
