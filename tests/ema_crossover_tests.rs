use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::ema_crossover::EmaCrossover;

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
fn insufficient_data_returns_hold() {
    let mut strat = EmaCrossover::new(2, 3, 0);
    assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
    assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
    assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
    assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
}

#[test]
fn emits_buy_on_bullish_crossover() {
    let mut strat = EmaCrossover::new(2, 4, 0);
    for &p in &[100.0, 90.0, 80.0, 70.0, 120.0, 150.0] {
        let sig = strat.on_tick(&tick(p));
        if sig == Signal::Buy {
            return;
        }
    }
    panic!("expected buy signal");
}

#[test]
fn deterministic_output() {
    let prices: Vec<f64> = (0..180)
        .map(|i| 100.0 + 15.0 * (i as f64 * 0.08).sin())
        .collect();
    let run = |prices: &[f64]| {
        let mut strat = EmaCrossover::new(5, 13, 0);
        prices
            .iter()
            .map(|&p| strat.on_tick(&tick(p)))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(&prices), run(&prices));
}
