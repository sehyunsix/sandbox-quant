use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::stochastic_reversion::StochasticReversionStrategy;

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
    let mut strat = StochasticReversionStrategy::new(5, 80, 1);
    for p in [100.0, 100.2, 100.1, 100.3] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
}

#[test]
fn buy_then_sell_on_k_band_cross() {
    let mut strat = StochasticReversionStrategy::new(5, 80, 1);
    let mut bought = false;
    for p in [100.0, 99.0, 98.0, 97.0, 96.0, 95.0] {
        if strat.on_tick(&tick(p)) == Signal::Buy {
            bought = true;
        }
    }
    assert!(bought, "expected buy under oversold threshold");

    for p in [96.0, 98.0, 100.0, 103.0, 106.0] {
        let sig = strat.on_tick(&tick(p));
        if sig == Signal::Sell {
            return;
        }
    }
    panic!("expected sell above overbought threshold");
}
