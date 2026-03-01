use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::ma_crossover::MaCrossover;

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
    let mut strat = MaCrossover::new(2, 3, 0);
    assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
    assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
    assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
    assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
}

#[test]
fn buy_signal_on_bullish_crossover() {
    let mut strat = MaCrossover::new(2, 4, 0);
    for &p in &[100.0, 90.0, 80.0, 70.0] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
    let sig = strat.on_tick(&tick(120.0));
    assert_eq!(sig, Signal::Buy, "Expected Buy signal, got {:?}", sig);
    assert!(strat.is_long());
}

#[test]
fn sell_signal_on_bearish_crossover() {
    let mut strat = MaCrossover::new(2, 4, 0);
    for &p in &[100.0, 90.0, 80.0, 70.0, 120.0, 150.0] {
        strat.on_tick(&tick(p));
    }

    for &p in &[60.0, 40.0, 30.0, 20.0] {
        let sig = strat.on_tick(&tick(p));
        if matches!(sig, Signal::Sell) {
            assert!(!strat.is_long());
            return;
        }
    }
    panic!("Expected Sell signal during price drop");
}

#[test]
fn no_double_buy() {
    let mut strat = MaCrossover::new(2, 4, 0);
    for &p in &[100.0, 90.0, 80.0, 70.0, 120.0, 150.0] {
        strat.on_tick(&tick(p));
    }
    assert!(strat.is_long());

    for &p in &[160.0, 170.0, 180.0, 190.0] {
        let sig = strat.on_tick(&tick(p));
        assert_eq!(
            sig,
            Signal::Hold,
            "Should not double-buy while already long"
        );
    }
}

#[test]
fn cooldown_prevents_rapid_signals() {
    let mut strat = MaCrossover::new(2, 4, 100);

    for &p in &[100.0, 90.0, 80.0, 70.0, 120.0] {
        strat.on_tick(&tick(p));
    }
    let sig = strat.on_tick(&tick(150.0));

    if matches!(sig, Signal::Buy) {
        for &p in &[50.0, 30.0] {
            let sig = strat.on_tick(&tick(p));
            assert_eq!(sig, Signal::Hold, "Cooldown should prevent rapid sell");
        }
    }
}

#[test]
fn deterministic_output() {
    let prices: Vec<f64> = (0..200)
        .map(|i| 100.0 + 20.0 * (i as f64 * 0.1).sin())
        .collect();

    let run = |prices: &[f64]| -> Vec<Signal> {
        let mut strat = MaCrossover::new(5, 15, 0);
        prices.iter().map(|&p| strat.on_tick(&tick(p))).collect()
    };

    let run1 = run(&prices);
    let run2 = run(&prices);
    assert_eq!(run1, run2, "Strategy must be deterministic");
}
