use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::opening_range_breakout::OpeningRangeBreakoutStrategy;

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
    let mut strat = OpeningRangeBreakoutStrategy::new(4, 3, 1);
    for p in [100.0, 101.0, 100.5, 101.5, 101.2, 101.1] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
}

#[test]
fn breakout_buy_then_trailing_sell() {
    let mut strat = OpeningRangeBreakoutStrategy::new(4, 3, 1);
    for p in [100.0, 101.0, 102.0, 103.0, 102.5, 102.8, 103.2, 103.5] {
        let sig = strat.on_tick(&tick(p));
        if (p - 103.5).abs() < f64::EPSILON {
            assert_eq!(sig, Signal::Buy);
        }
    }
    for p in [102.9, 102.0, 101.0] {
        if strat.on_tick(&tick(p)) == Signal::Sell {
            return;
        }
    }
    panic!("expected sell when trailing range breaks down");
}
