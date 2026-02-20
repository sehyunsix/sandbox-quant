use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::volatility_compression::VolatilityCompressionStrategy;

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
    let mut strat = VolatilityCompressionStrategy::new(5, 120, 1);
    for p in [100.0, 100.1, 100.0, 100.1] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
}

#[test]
fn compression_breakout_buy_then_mean_revert_sell() {
    let mut strat = VolatilityCompressionStrategy::new(5, 200, 1);
    for p in [100.0, 100.05, 100.00, 100.04, 100.03] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
    let buy = strat.on_tick(&tick(100.30));
    assert_eq!(buy, Signal::Buy);

    for p in [100.2, 100.1, 99.95] {
        if strat.on_tick(&tick(p)) == Signal::Sell {
            return;
        }
    }
    panic!("expected sell after mean breakdown");
}
