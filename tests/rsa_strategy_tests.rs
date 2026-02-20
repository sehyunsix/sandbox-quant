use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::rsa::RsaStrategy;

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
fn warmup_returns_hold() {
    let mut strat = RsaStrategy::new(14, 30.0, 70.0, 0);
    for p in [100.0, 99.0, 98.0, 97.0, 96.0, 95.0] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
}

#[test]
fn buy_then_sell_on_rsi_thresholds() {
    let mut strat = RsaStrategy::new(6, 30.0, 70.0, 0);

    let mut bought = false;
    for p in [100.0, 98.0, 96.0, 94.0, 92.0, 90.0, 88.0, 86.0, 84.0] {
        if strat.on_tick(&tick(p)) == Signal::Buy {
            bought = true;
        }
    }
    if strat.on_tick(&tick(82.0)) == Signal::Buy {
        bought = true;
    }
    assert!(bought, "expected at least one buy signal in oversold leg");

    for p in [86.0, 90.0, 95.0, 100.0, 106.0, 112.0, 118.0] {
        let sig = strat.on_tick(&tick(p));
        if sig == Signal::Sell {
            return;
        }
    }
    panic!("expected sell signal after strong rebound");
}

#[test]
fn cooldown_is_respected() {
    let mut strat = RsaStrategy::new(4, 30.0, 70.0, 4);

    let mut bought = false;
    for p in [100.0, 98.0, 96.0, 94.0, 92.0, 90.0, 88.0] {
        if strat.on_tick(&tick(p)) == Signal::Buy {
            bought = true;
        }
    }
    if strat.on_tick(&tick(86.0)) == Signal::Buy {
        bought = true;
    }
    assert!(bought, "expected buy before cooldown check");

    let early = strat.on_tick(&tick(120.0));
    assert_eq!(early, Signal::Hold, "cooldown should block immediate sell");
}
