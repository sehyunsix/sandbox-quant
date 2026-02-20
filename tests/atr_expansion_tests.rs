use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::atr_expansion::AtrExpansionStrategy;

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
    let mut strat = AtrExpansionStrategy::new(5, 180, 1);
    for p in [100.0, 100.2, 99.9, 100.1] {
        assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
    }
}

#[test]
fn emits_buy_then_sell_on_large_opposite_moves() {
    let mut strat = AtrExpansionStrategy::new(5, 120, 1);
    for p in [100.0, 100.1, 100.2, 100.25, 100.3, 100.35, 100.4] {
        strat.on_tick(&tick(p));
    }
    let buy = strat.on_tick(&tick(104.0));
    assert_eq!(buy, Signal::Buy);

    for p in [103.5, 103.0, 101.0, 98.0, 95.0, 92.0] {
        let sig = strat.on_tick(&tick(p));
        if sig == Signal::Sell {
            return;
        }
    }
    panic!("expected sell signal after large negative expansion");
}
