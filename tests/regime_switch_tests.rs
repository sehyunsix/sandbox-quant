use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::regime_switch::RegimeSwitchStrategy;

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
fn regime_switch_emits_actionable_signals() {
    let mut strategy = RegimeSwitchStrategy::new(3, 8, 0);

    for _ in 0..8 {
        let _ = strategy.on_tick(&tick(100.0));
    }

    for p in [95.0, 105.0, 94.0, 106.0, 93.0, 107.0] {
        if strategy.on_tick(&tick(p)) != Signal::Hold {
            return;
        }
    }
    panic!("expected at least one actionable signal");
}

#[test]
fn regime_switch_respects_cooldown() {
    let mut strategy = RegimeSwitchStrategy::new(3, 8, 4);

    for p in [
        100.0, 101.0, 102.0, 104.0, 106.0, 108.0, 110.0, 112.0, 114.0,
    ] {
        let _ = strategy.on_tick(&tick(p));
    }

    // Immediate violent move should be throttled by cooldown.
    let sig = strategy.on_tick(&tick(90.0));
    assert_eq!(sig, Signal::Hold);
}
