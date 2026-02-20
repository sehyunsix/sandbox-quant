use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::ma_reversion::MaReversionStrategy;

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
    let mut strat = MaReversionStrategy::new(5, 200, 1);
    for p in [100.0, 100.2, 100.1, 100.3] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
}

#[test]
fn buy_on_drop_below_mean_then_sell_on_recovery() {
    let mut strat = MaReversionStrategy::new(5, 100, 1);

    for p in [100.0, 100.2, 100.1, 100.3, 100.2, 96.0] {
        let sig = strat.on_tick(&tick(p));
        if (p - 96.0).abs() < f64::EPSILON {
            assert_eq!(sig, Signal::Buy);
        }
    }

    for p in [98.0, 99.5, 100.5, 101.0] {
        let sig = strat.on_tick(&tick(p));
        if sig == Signal::Sell {
            return;
        }
    }
    panic!("expected sell signal after mean recovery");
}
