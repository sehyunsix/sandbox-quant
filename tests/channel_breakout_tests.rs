use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::channel_breakout::ChannelBreakoutStrategy;

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
    let mut strat = ChannelBreakoutStrategy::new(5, 3, 1);
    for p in [100.0, 100.2, 100.1, 100.3] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
}

#[test]
fn emits_buy_then_sell() {
    let mut strat = ChannelBreakoutStrategy::new(4, 2, 1);
    for p in [100.0, 101.0, 102.0, 103.0, 104.0] {
        let sig = strat.on_tick(&tick(p));
        if p >= 104.0 {
            assert_eq!(sig, Signal::Buy);
        }
    }
    for p in [103.0, 101.0, 99.0] {
        let sig = strat.on_tick(&tick(p));
        if sig == Signal::Sell {
            return;
        }
    }
    panic!("expected sell signal");
}
