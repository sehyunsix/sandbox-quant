use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::bollinger_reversion::BollingerReversionStrategy;

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
    let mut strat = BollingerReversionStrategy::new(5, 200, 1);
    for p in [100.0, 100.1, 100.2, 100.1] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
}

#[test]
fn buy_then_sell_on_reversion() {
    let mut strat = BollingerReversionStrategy::new(5, 150, 1);
    for p in [100.0, 100.2, 100.1, 100.3, 100.2, 95.0] {
        let sig = strat.on_tick(&tick(p));
        if (p - 95.0).abs() < f64::EPSILON {
            assert_eq!(sig, Signal::Buy);
        }
    }
    for p in [97.0, 99.0, 100.2, 101.0] {
        let sig = strat.on_tick(&tick(p));
        if sig == Signal::Sell {
            return;
        }
    }
    panic!("expected sell signal on mean recovery");
}
